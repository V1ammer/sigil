//! Devices settings — list devices, add new device via QR provisioning, revoke devices.
//!
//! ## Flow
//!
//! 1. Lists all devices for the current user (real API).
//! 2. "Add Device" button opens a modal dialog.
//! 3. QR scanner (manual input + optional BarcodeDetector).
//! 4. Decode QR → validate server/user → confirm dialog.
//! 5. On confirm:
//!    a. Generate new device permanent keys.
//!    b. Build AGE-encrypted bootstrap blob.
//!    c. Sign device authorization with identity key.
//!    d. `POST /v1/provisioning/requests/:id/approve`.
//! 6. "Revoke" button on non-current, non-revoked devices:
//!    a. AlertDialog confirmation.
//!    b. Sign revocation message with identity key.
//!    c. `POST /v1/devices/me/:id/revoke`.
//!    d. Server handles MLS Remove internally; refresh device list.

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_core::ed25519::Ed25519Pair;
#[cfg(any(feature = "native", feature = "wasm-mls"))]
use messenger_core::mls::group::MlsRuntime;
#[cfg(any(feature = "native", feature = "wasm-mls"))]
use messenger_core::mls::keypackage::generate_keypackage_bundle;
use messenger_core::prov::{decode_qr, QrPayload};
use messenger_proto::keypackages::PublishKeyPackagesRequest;
use messenger_proto::mls::{MemberChange, PostCommitRequest, WelcomePayload};
use messenger_proto::provisioning::ApproveProvisioningRequest;
use rand::RngCore;
use uuid::Uuid;

use crate::components::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter,
    AlertDialogHeader, AlertDialogTitle,
};
use crate::components::badge::Badge;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::dialog::{Dialog, DialogHeader, DialogTitle, DialogDescription, DialogFooter};
use crate::components::qr_scanner::QrScanner;
use crate::components::separator::Separator;
use crate::i18n::I18n;
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::session::{build_api_client, load_server_url, use_session, SessionState, UserRole};
use crate::t;

/// Current step in the provisioning approve flow.
#[derive(Clone, Debug, PartialEq)]
enum ProvisioningStep {
    Scan,
    Confirm {
        qr: QrPayload,
        nonce_fingerprint: String,
    },
    Approving,
    Success,
    Error(String),
}

/// Data for a device row.
#[derive(Clone, Debug)]
struct DeviceRowData {
    id: Uuid,
    is_current: bool,
    created_at: i64,
    #[allow(dead_code)]
    revoked_at: Option<i64>,
}

/// Devices settings — list of devices with current device badge and Add button.
#[must_use]
#[component]
pub fn DevicesSettings() -> impl IntoView {
    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let session = use_session();
    let notifications = use_context::<NotificationsState>()
        .expect("NotificationsState must be provided");

    // Device list state
    let devices = RwSignal::new(Vec::<DeviceRowData>::new());
    let loading = RwSignal::new(true);

    // Add-device dialog state
    let show_add_dialog = RwSignal::new(false);
    let step = RwSignal::new(ProvisioningStep::Scan);
    // The scanned QR, kept out of the step enum: the Confirm→Approving
    // transition replaces the step (dropping the qr it carried), so the
    // approve task reads it from here instead.
    let pending_qr: RwSignal<Option<QrPayload>> = RwSignal::new(None);

    // QR decode intermediate: when non-empty, triggers processing
    let qr_decode_trigger = RwSignal::new(Option::<String>::None);

    // Revoke dialog state
    let show_revoke_dialog = RwSignal::new(false);
    let revoke_target = RwSignal::new(Option::<DeviceRowData>::None);
    let revoking = RwSignal::new(false);

    // Fetch devices on mount
    let sess_fetch = session.clone();
    let notif_fetch = notifications.clone();
    spawn_local({
        let devs = devices;
        let loading = loading;
        let notif = notif_fetch;
        async move {
            let api = build_api_client();
            match api {
                Some(client) => match client.list_devices().await {
                    Ok(resp) => {
                        let current_id = sess_fetch.current_device_id().unwrap_or_default();
                        let rows: Vec<DeviceRowData> = resp.devices.into_iter().map(|d| DeviceRowData {
                            id: d.id,
                            is_current: d.id == current_id,
                            created_at: d.created_at,
                            revoked_at: d.revoked_at,
                        }).collect();
                        devs.set(rows);
                        loading.set(false);
                    }
                    Err(e) => {
                        notif.push(ToastKind::Error, format!("Failed to load devices: {e}"));
                        loading.set(false);
                    }
                },
                None => {
                    loading.set(false);
                }
            }
        }
    });

    // Effect: when qr_decode_trigger gets a value, process the QR
    Effect::new({
        let st = step;
        let sess = session.clone();
        let nf = notifications.clone();
        // t!() panics inside the spawn_local below (no leptos owner).
        let i18n = i18n.clone();
        move |_| {
            if let Some(qr_text) = qr_decode_trigger.get() {
                let st = st;
                let sess = sess.clone();
                let nf = nf.clone();
                let i18n = i18n.clone();
                spawn_local(async move {
                    // Parse QR payload
                    let qr = match decode_qr(&qr_text) {
                        Ok(p) => p,
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("Invalid QR code: {e}")));
                            return;
                        }
                    };

                    // Validate server_url matches current session
                    let current_url = load_server_url()
                        .or_else(|| sess.state.with(|s| {
                            if let SessionState::ServerConfigured { url } = s {
                                Some(url.clone())
                            } else {
                                None
                            }
                        }));
                    if let Some(url) = current_url {
                        if qr.server_url != url && !qr.server_url.is_empty() {
                            st.set(ProvisioningStep::Error(i18n.t("scan.error.wrongServer")));
                            nf.push(ToastKind::Error, i18n.t("scan.error.wrongServer"));
                            return;
                        }
                    }

                    // Validate user_id matches current user
                    let current_uid = sess.current_user_id();
                    if let Some(uid) = current_uid {
                        if qr.user_id != uid {
                            st.set(ProvisioningStep::Error(i18n.t("scan.error.wrongUser")));
                            nf.push(ToastKind::Error, i18n.t("scan.error.wrongUser"));
                            return;
                        }
                    }

                    // Build nonce fingerprint (first 4 bytes in hex)
                    let nonce_fp = qr.nonce.iter()
                        .take(4)
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(":");

                    st.set(ProvisioningStep::Confirm {
                        qr,
                        nonce_fingerprint: nonce_fp,
                    });
                });
            }
        }
    });

    // The QR decode callback just sets the trigger signal
    let on_qr_text = move |text: String| {
        qr_decode_trigger.set(Some(text));
    };

    // Effect: when step becomes Approving, run the approve flow
    Effect::new({
        let sess = session.clone();
        let nf = notifications.clone();
        let devs = devices;
        let st = step;
        // Captured handle: t!() panics inside spawn_local (no leptos owner).
        let i18n = i18n.clone();
        move |_| {
            if st.get() == ProvisioningStep::Approving {
                let sess = sess.clone();
                let nf = nf.clone();
                let devs = devs;
                let st = st;
                let i18n = i18n.clone();

                #[cfg(any(feature = "native", feature = "wasm-mls"))]
                spawn_local(async move {
                    // AGE bootstrap encryption runs in the native Tauri backend
                    // (via tauri_bridge); a plain browser has no such backend.
                    if !crate::tauri_bridge::is_tauri_context() {
                        let msg = i18n.t("scan.error.browserOnly");
                        nf.push(ToastKind::Error, msg.clone());
                        st.set(ProvisioningStep::Error(msg));
                        return;
                    }
                    // The QR captured at confirm time (the step is now Approving).
                    let qr = match pending_qr.get_untracked() {
                        Some(q) => q,
                        None => {
                            st.set(ProvisioningStep::Error("missing QR payload".into()));
                            return;
                        }
                    };

                    // Build API client
                    let api = match build_api_client() {
                        Some(a) => a,
                        None => {
                            st.set(ProvisioningStep::Error("Not authenticated".to_string()));
                            return;
                        }
                    };

                    // Get current identity + account role (so the new device
                    // inherits admin/user instead of always landing as a user).
                    let (identity, account_role) = match sess.state.get_untracked() {
                        SessionState::Authenticated { ref identity, ref role } => {
                            let r = match role {
                                UserRole::Admin => "admin",
                                UserRole::User => "user",
                            };
                            (identity.clone(), r.to_string())
                        }
                        _ => {
                            st.set(ProvisioningStep::Error("Not authenticated".to_string()));
                            return;
                        }
                    };

                    // Verify provisioning request is still pending
                    match api.get_provisioning_request(qr.provisioning_id).await {
                        Ok(req) => {
                            if req.status != "pending" {
                                st.set(ProvisioningStep::Error(
                                    i18n.t("scan.error.expired")
                                ));
                                nf.push(ToastKind::Warning, i18n.t("scan.error.expired"));
                                return;
                            }
                        }
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("{e}")));
                            nf.push(ToastKind::Error, format!("{}: {e}", i18n.t("error.network")));
                            return;
                        }
                    }

                    // ── Step A: Generate permanent keys for the new device ──
                    let new_device_signing = Ed25519Pair::generate();
                    let new_device_signing_pk = new_device_signing.public_bytes();
                    let new_device_signing_sk = new_device_signing.secret_bytes();

                    let mut new_device_hpke_seed = [0u8; 32];
                    rand::thread_rng().fill_bytes(&mut new_device_hpke_seed);
                    let hpke_secret = x25519_dalek::StaticSecret::from(new_device_hpke_seed);
                    let new_device_hpke_pub = x25519_dalek::PublicKey::from(&hpke_secret).to_bytes();

                    // ── Step A2: Generate MLS KeyPackageBundle for new device ──
                    let local_store: Arc<dyn messenger_storage::traits::MessengerLocalStore> = match messenger_storage::init_storage("default").await {
                        Ok(s) => s.into(),
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("Storage init failed: {e}")));
                            return;
                        }
                    };
                    let runtime = MlsRuntime::new(local_store.clone(), identity.device_id);
                    let (kp_bundle_bytes, gen_kp) = match generate_keypackage_bundle(
                        &messenger_core::mls::provider::OpenMlsRustCrypto::default(),
                        &identity,
                        86400, // 24 hours
                        false,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("KeyPackage generation failed: {e}")));
                            return;
                        }
                    };

                    // ── Step B: Build and encrypt bootstrap payload ──
                    // Sealed with a pure-wasm box (x25519 + AES-GCM) so the new
                    // device decrypts it whether it's the app or a browser — no
                    // native AGE backend required on either side.
                    let bootstrap_payload = crate::tauri_bridge::BootstrapPayload {
                        user_id: identity.user_id,
                        username: identity.username.clone(),
                        identity_signing_seed: identity.identity_signing_key.secret_bytes(),
                        device_signing_seed: new_device_signing_sk,
                        device_hpke_seed: new_device_hpke_seed,
                        key_package_bundle: kp_bundle_bytes,
                        role: account_role.clone(),
                        avatar: crate::state::avatar_store::load_own_avatar(identity.user_id),
                    };

                    // Encrypt under the new device's temp X25519 pub key (from QR).
                    let temp_x25519_bytes = qr.new_device_temp_x25519_pub;
                    let payload_bytes = match rmp_serde::to_vec_named(&bootstrap_payload) {
                        Ok(b) => b,
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("Encode failed: {e}")));
                            return;
                        }
                    };
                    let blob = match messenger_core::bootstrap_seal::seal(
                        &temp_x25519_bytes,
                        &payload_bytes,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("Encryption failed: {e}")));
                            nf.push(ToastKind::Error, format!("Encryption failed: {e}"));
                            return;
                        }
                    };

                    // ── Step C: Create device_authorization_signature ──
                    // msg = new_device_signing_pk || new_device_hpke_pk || ts_le
                    // now_secs() is WASM-safe; std::time::SystemTime panics on
                    // wasm32 ("time not implemented on this platform").
                    let ts = messenger_core::api::signing::now_secs();
                    let ts_bytes = ts.to_le_bytes();
                    let mut auth_msg = Vec::new();
                    auth_msg.extend_from_slice(&new_device_signing_pk);
                    auth_msg.extend_from_slice(&new_device_hpke_pub);
                    auth_msg.extend_from_slice(&ts_bytes);
                    let auth_sig = identity.identity_signing_key.sign(&auth_msg);

                    // ── Step D: POST approve (now returns device_id) ──
                    let approve_req = ApproveProvisioningRequest {
                        encrypted_bootstrap_blob: blob,
                        new_device_hpke_public_key: new_device_hpke_pub.to_vec(),
                        new_device_signing_public_key: new_device_signing_pk.to_vec(),
                        device_authorization_signature: auth_sig.to_vec(),
                        device_authorization_timestamp: ts,
                    };

                    let new_device_id = match api.approve_provisioning_request(qr.provisioning_id, &approve_req).await {
                        Ok(resp) => resp.device_id,
                        Err(e) => {
                            st.set(ProvisioningStep::Error(format!("Approve failed: {e}")));
                            nf.push(ToastKind::Error, format!("Approve failed: {e}"));
                            return;
                        }
                    };

                    // ── Step E: Publish KeyPackage and add to all groups ──
                    let kp_bytes = gen_kp.key_package_bytes.clone();
                    let pub_req = PublishKeyPackagesRequest {
                        key_packages: vec![messenger_proto::keypackages::KeyPackageUpload {
                            key_package: kp_bytes.clone(),
                            init_key_hash: gen_kp.init_key_hash.clone(),
                            expires_at: gen_kp.expires_at,
                            is_last_resort: gen_kp.is_last_resort,
                        }],
                    };
                    if let Err(e) = api.publish_keypackages(&pub_req).await {
                        nf.push(ToastKind::Warning, format!("KeyPackage publish failed: {e}"));
                    }

                    // List groups and add new device to each
                    if let Ok(groups) = api.list_groups(None).await {
                        for g in &groups.groups {
                            match runtime.propose_add(g.id, &identity, &[kp_bytes.clone()]).await {
                                Ok(pc) => {
                                    let commit_req = PostCommitRequest {
                                        expected_epoch: pc.epoch as i64,
                                        commit: pc.commit,
                                        welcomes: pc.welcomes.into_iter().map(|w| WelcomePayload {
                                            recipient_device_id: new_device_id,
                                            welcome_ciphertext: w,
                                        }).collect(),
                                        member_changes: vec![MemberChange {
                                            kind: "add".into(),
                                            user_id: identity.user_id,
                                            device_id: new_device_id,
                                            leaf_index: None,
                                            role_in_chat: Some("member".into()),
                                        }],
                                    };
                                    if let Err(e) = api.post_commit(g.id, &commit_req).await {
                                        // Per-group; log instead of spamming a toast
                                        // for every group the device couldn't be
                                        // committed to (epoch races, etc.).
                                        web_sys::console::warn_1(
                                            &format!("[provision] add to group {} failed: {e}", g.id).into(),
                                        );
                                    }
                                }
                                Err(e) => {
                                    // "group not found" just means this device holds
                                    // no local MLS state for that group, so it can't
                                    // add the new device there. Expected for many
                                    // groups — log it, don't toast one per group.
                                    web_sys::console::warn_1(
                                        &format!("[provision] propose_add for group {} skipped: {e}", g.id).into(),
                                    );
                                }
                            }
                        }
                    }

                    nf.push(ToastKind::Success, i18n.t("scan.success"));

                    // Refresh device list
                    if let Some(client) = build_api_client() {
                        if let Ok(resp) = client.list_devices().await {
                            let current_id = sess.current_device_id().unwrap_or_default();
                            let rows: Vec<DeviceRowData> = resp.devices.into_iter().map(|d| DeviceRowData {
                                id: d.id,
                                is_current: d.id == current_id,
                                created_at: d.created_at,
                                revoked_at: d.revoked_at,
                            }).collect();
                            devs.set(rows);
                        }
                    }

                    st.set(ProvisioningStep::Success);
                });
                #[cfg(not(any(feature = "native", feature = "wasm-mls")))]
                {
                    nf.push(ToastKind::Error, t!("scan.error.browserOnly"));
                }
            }
        }
    });

    // ── Revoke handler ─────────────────────────────────────────────
    let do_revoke = Arc::new({
        let sess = session.clone();
        let nf = notifications.clone();
        let devs = devices;
        let target = revoke_target;
        let show = show_revoke_dialog;
        let rev = revoking;
        // t!() panics inside the spawn_local below (no leptos owner after the
        // await) — that panic aborted the task before rev.set(false) ran, so
        // the "Revoking…" spinner hung even though the revoke had succeeded.
        let i18n = i18n.clone();
        move || {
            let sess = sess.clone();
            let nf = nf.clone();
            let devs = devs;
            let target = target;
            let show = show;
            let rev = rev;
            let i18n = i18n.clone();
            spawn_local(async move {
                let device_id = match target.get_untracked() {
                    Some(ref d) => d.id,
                    None => return,
                };

                // Get identity key for signing
                let identity = match sess.state.get_untracked() {
                    SessionState::Authenticated { ref identity, .. } => identity.clone(),
                    _ => {
                        nf.push(ToastKind::Error, "Not authenticated".to_string());
                        show.set(false);
                        rev.set(false);
                        return;
                    }
                };

                let api = match build_api_client() {
                    Some(a) => a,
                    None => {
                        nf.push(ToastKind::Error, "Not authenticated".to_string());
                        show.set(false);
                        rev.set(false);
                        return;
                    }
                };

                rev.set(true);

                // Build revocation signature over the SAME bytes the server
                // verifies: "revoke:" || device_id raw bytes || ":" || ts string.
                // (Previously signed the UUID *string*, which never matched.)
                let ts = messenger_core::api::signing::now_secs();
                let mut msg = b"revoke:".to_vec();
                msg.extend_from_slice(device_id.as_bytes());
                msg.push(b':');
                msg.extend_from_slice(ts.to_string().as_bytes());
                let revocation_signature = identity.identity_signing_key.sign(&msg);

                let req = messenger_proto::users::RevokeDeviceRequest {
                    revocation_signature: revocation_signature.to_vec(),
                    revocation_timestamp: ts,
                };

                match api.revoke_device(device_id, &req).await {
                    Ok(_) => {
                        nf.push(ToastKind::Success, i18n.t("settings.devices.revokedToast"));
                        // Refresh device list
                        if let Some(client) = build_api_client() {
                            if let Ok(resp) = client.list_devices().await {
                                let current_id = sess.current_device_id().unwrap_or_default();
                                let rows: Vec<DeviceRowData> = resp.devices.into_iter().map(|d| DeviceRowData {
                                    id: d.id,
                                    is_current: d.id == current_id,
                                    created_at: d.created_at,
                                    revoked_at: d.revoked_at,
                                }).collect();
                                devs.set(rows);
                            }
                        }
                    }
                    Err(e) => {
                        nf.push(ToastKind::Error, format!("Revoke failed: {e}"));
                    }
                }

                show.set(false);
                rev.set(false);
                target.set(None);
            });
        }
    });

    let open_revoke_dialog = Arc::new({
        let target = revoke_target;
        let show = show_revoke_dialog;
        move |d: DeviceRowData| {
            target.set(Some(d));
            show.set(true);
        }
    });

    let close_revoke = {
        let show = show_revoke_dialog;
        let target = revoke_target;
        move || {
            show.set(false);
            target.set(None);
        }
    };

    // Clone handles for the revoke dialog children (avoid FnOnce in move closure)
    let do_revoke_for_action = do_revoke.clone();
    let close_revoke_for_cancel = close_revoke.clone();
    let revoking_for_status = revoking;
    let show_for_action = show_revoke_dialog;

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                    <h3 class="text-lg font-medium text-foreground">{t!("settings.devices.title")}</h3>
                    <p class="text-sm text-muted-foreground">{t!("settings.devices.description")}</p>
                </div>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Outline)
                    size=Signal::derive(move || ButtonSize::Sm)
                    class="shrink-0".to_string()
                    on_click=Box::new({
                        let st = step;
                        let show = show_add_dialog;
                        move |_| {
                            st.set(ProvisioningStep::Scan);
                            show.set(true);
                        }
                    })
                >
                    {t!("settings.devices.addDevice")}
                </Button>
            </div>

            <Separator />

            <div class="space-y-3">
                {move || {
                    if loading.get() {
                        view! {
                            <div class="flex items-center justify-center py-8">
                                <span class="h-6 w-6 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                            </div>
                        }.into_any()
                    } else if devices.get().is_empty() {
                        view! {
                            <p class="text-sm text-muted-foreground text-center py-4">
                                {t!("settings.devices.noDevices")}
                            </p>
                        }.into_any()
                    } else {
                        let on_revoke = open_revoke_dialog.clone();
                        devices.get().into_iter().map(move |d| {
                            let on_revoke = on_revoke.clone();
                            view! { <DeviceRow device=d on_revoke=on_revoke /> }
                        }).collect::<Vec<_>>().into_any()
                    }
                }}
            </div>
        </div>

        // ── Revoke Confirmation Dialog ─────────────────────────────
        <AlertDialog
            is_open=Signal::derive(move || show_for_action.get())
            on_close=Box::new(close_revoke_for_cancel.clone())
        >
            <RevokeDialogBody
                revoking=revoking_for_status
                do_revoke=do_revoke_for_action.clone()
                on_cancel=Arc::new(close_revoke_for_cancel.clone())
            />
        </AlertDialog>

        // ── Add Device Dialog ─────────────────────────────────────
        <Dialog
            is_open=Signal::derive(move || show_add_dialog.get())
            on_close=Box::new({
                let st = step;
                let show = show_add_dialog;
                move || {
                    show.set(false);
                    st.set(ProvisioningStep::Scan);
                }
            })
        >
            {move || {
                let current_step = step.get();
                match current_step {
                    ProvisioningStep::Scan => {
                        view! {
                            <>
                                <DialogHeader>
                                    <DialogTitle>{t!("scan.title")}</DialogTitle>
                                    <DialogDescription>{t!("settings.devices.description")}</DialogDescription>
                                </DialogHeader>
                                <QrScanner on_decode=Box::new(on_qr_text.clone()) />
                                <DialogFooter>
                                    <Button
                                        variant=Signal::derive(move || ButtonVariant::Outline)
                                        on_click=Box::new({
                                            let st = step;
                                            let show = show_add_dialog;
                                            move |_| {
                                                show.set(false);
                                                st.set(ProvisioningStep::Scan);
                                            }
                                        })
                                    >
                                        {t!("common.close")}
                                    </Button>
                                </DialogFooter>
                            </>
                        }.into_any()
                    }
                    ProvisioningStep::Confirm { ref qr, ref nonce_fingerprint } => {
                        let qr_clone = qr.clone();
                        let fp = nonce_fingerprint.clone();
                        view! {
                            <>
                                <DialogHeader>
                                    <DialogTitle>{t!("scan.confirm.title")}</DialogTitle>
                                    <DialogDescription>
                                        {t!("settings.devices.addDeviceFor")}
                                        {qr_clone.user_id.to_string().chars().take(8).collect::<String>()}
                                        "..."
                                    </DialogDescription>
                                </DialogHeader>
                                <div class="space-y-3 py-4">
                                    <div class="rounded-lg border p-4 space-y-2">
                                        <div class="flex justify-between text-sm">
                                            <span class="text-muted-foreground">{t!("scan.nonce")}</span>
                                            <code class="font-mono text-xs">{fp}</code>
                                        </div>
                                        <div class="flex justify-between text-sm">
                                            <span class="text-muted-foreground">"Provisioning ID"</span>
                                            <code class="font-mono text-xs">{qr_clone.provisioning_id.to_string()}</code>
                                        </div>
                                    </div>
                                </div>
                                <DialogFooter>
                                    <Button
                                        variant=Signal::derive(move || ButtonVariant::Outline)
                                        on_click=Box::new({
                                            let st = step;
                                            move |_| st.set(ProvisioningStep::Scan)
                                        })
                                    >
                                        {t!("common.cancel")}
                                    </Button>
                                    <Button
                                        variant=Signal::derive(move || ButtonVariant::Default)
                                        on_click=Box::new({
                                            let st = step;
                                            let qr = qr_clone.clone();
                                            move |_| {
                                                // Stash the QR before the step
                                                // change discards the Confirm
                                                // variant that held it.
                                                pending_qr.set(Some(qr.clone()));
                                                st.set(ProvisioningStep::Approving);
                                            }
                                        })
                                    >
                                        {t!("scan.confirm")}
                                    </Button>
                                </DialogFooter>
                            </>
                        }.into_any()
                    }
                    ProvisioningStep::Approving => {
                        view! {
                            <>
                                <DialogHeader>
                                    <DialogTitle>{t!("scan.confirm.title")}</DialogTitle>
                                </DialogHeader>
                                <div class="flex flex-col items-center justify-center py-8 space-y-4">
                                    <span class="h-8 w-8 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                                    <p class="text-sm text-muted-foreground">{t!("scan.progress.approve")}</p>
                                </div>
                            </>
                        }.into_any()
                    }
                    ProvisioningStep::Success => {
                        view! {
                            <>
                                <DialogHeader>
                                    <DialogTitle>{t!("scan.success")}</DialogTitle>
                                </DialogHeader>
                                <div class="flex flex-col items-center justify-center py-8 space-y-4">
                                    <svg class="h-12 w-12 text-green-500" xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                                    <p class="text-sm text-muted-foreground">{t!("scan.success")}</p>
                                </div>
                                <DialogFooter>
                                    <Button
                                        variant=Signal::derive(move || ButtonVariant::Default)
                                        on_click=Box::new({
                                            let st = step;
                                            let show = show_add_dialog;
                                            move |_| {
                                                show.set(false);
                                                st.set(ProvisioningStep::Scan);
                                            }
                                        })
                                    >
                                        {t!("common.close")}
                                    </Button>
                                </DialogFooter>
                            </>
                        }.into_any()
                    }
                    ProvisioningStep::Error(ref err_msg) => {
                        let msg = err_msg.clone();
                        view! {
                            <>
                                <DialogHeader>
                                    <DialogTitle>{t!("error.title")}</DialogTitle>
                                </DialogHeader>
                                <div class="py-4">
                                    <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                        <p class="text-sm">{msg}</p>
                                    </div>
                                </div>
                                <DialogFooter>
                                    <Button
                                        variant=Signal::derive(move || ButtonVariant::Outline)
                                        on_click=Box::new({
                                            let st = step;
                                            move |_| st.set(ProvisioningStep::Scan)
                                        })
                                    >
                                        {t!("common.cancel")}
                                    </Button>
                                    <Button
                                        variant=Signal::derive(move || ButtonVariant::Default)
                                        on_click=Box::new({
                                            let st = step;
                                            move |_| st.set(ProvisioningStep::Scan)
                                        })
                                    >
                                        {t!("error.retry")}
                                    </Button>
                                </DialogFooter>
                            </>
                        }.into_any()
                    }
                }
            }}
        </Dialog>
    }
}

/// Inner content of the revoke confirmation dialog — extracted as a component
/// to avoid closure FnOnce issues.
#[must_use]
#[component]
fn RevokeDialogBody(
    revoking: RwSignal<bool>,
    do_revoke: Arc<dyn Fn() + Send + Sync + 'static>,
    on_cancel: Arc<dyn Fn() + Send + Sync + 'static>,
) -> impl IntoView {
    let is_revoking = move || revoking.get();

    view! {
        <AlertDialogHeader>
            <AlertDialogTitle>{t!("settings.devices.revokeTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
                {t!("settings.devices.revokeDesc")}
            </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
            <AlertDialogCancel
                on_click=Box::new({
                    let on_cancel = on_cancel.clone();
                    move || on_cancel()
                })
            >
                {t!("common.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
                class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                on_click=Box::new({
                    let do_revoke = do_revoke.clone();
                    move || do_revoke()
                })
            >
                {move || if is_revoking() {
                    view! { <span class="flex items-center gap-2">
                        <span class="h-4 w-4 block rounded-full border-2 border-current border-t-transparent animate-spin"/>
                        {t!("settings.devices.revoking")}
                    </span> }.into_any()
                } else {
                    view! { {t!("settings.devices.revoke")} }.into_any()
                }}
            </AlertDialogAction>
        </AlertDialogFooter>
    }
}

/// Single device row component.
#[must_use]
#[component]
fn DeviceRow(
    device: DeviceRowData,
    /// Called when the user clicks the Revoke button.
    #[prop(optional)]
    on_revoke: Option<Arc<dyn Fn(DeviceRowData) + Send + Sync + 'static>>,
) -> impl IntoView {
    let is_current = device.is_current;
    let is_revoked = device.revoked_at.is_some();
    let show_revoke = !is_current && !is_revoked;

    view! {
        <div class="flex items-center justify-between rounded-lg border p-4">
            <div class="flex items-center gap-3">
                <div class="flex h-10 w-10 items-center justify-center rounded-full bg-muted">
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
                        <rect x="5" y="2" width="14" height="20" rx="2" ry="2" />
                        <line x1="12" y1="18" x2="12.01" y2="18" />
                    </svg>
                </div>
                <div class="space-y-0.5 min-w-0">
                    <div class="flex items-center gap-2">
                        <span class="text-sm font-medium text-foreground truncate">
                            {move || format!("{} {}", t!("settings.devices.label"), &device.id.to_string()[..8])}
                        </span>
                        {move || if is_current {
                            view! {
                                <Badge variant=String::from("secondary") class="shrink-0".to_string()>
                                    {t!("settings.devices.currentDevice")}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                        {move || if is_revoked {
                            view! {
                                <Badge variant=String::from("destructive") class="shrink-0".to_string()>
                                    {t!("settings.devices.revoked")}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                    </div>
                    <p class="text-xs text-muted-foreground">
                        {t!("settings.devices.added")}
                        " "
                        {move || format_timestamp(device.created_at)}
                    </p>
                </div>
            </div>
            {move || if show_revoke {
                let on_revoke = on_revoke.clone();
                let dev = device.clone();
                view! {
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Destructive)
                        size=Signal::derive(move || ButtonSize::Sm)
                        on_click=Box::new(move |_| {
                            if let Some(ref f) = on_revoke {
                                f(dev.clone());
                            }
                        })
                    >
                        {t!("settings.devices.revoke")}
                    </Button>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
        </div>
    }
}

/// Format a Unix timestamp as a readable date string.
fn format_timestamp(ts: i64) -> String {
    let ms = (ts as f64) * 1000.0;
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    format!(
        "{}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

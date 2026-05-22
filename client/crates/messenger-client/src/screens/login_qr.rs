use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use rand::RngCore;
use messenger_core::ed25519::Ed25519Pair;
use messenger_core::prov::{encode_qr, QrPayload};
use messenger_proto::provisioning::CreateProvisioningRequest;
use uuid::Uuid;
use crate::i18n::I18n;
use crate::session::restore::persist_session;
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::session::{build_api_client, persist_auth_credentials, use_session, SessionState, UserRole};
use crate::t;

#[must_use]
#[component]
pub fn LoginQrScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let session = use_session();
    let navigate = use_navigate();
    let notifications = use_context::<NotificationsState>()
        .expect("NotificationsState must be provided");

    // Step 1: Username input
    let username = RwSignal::new(String::new());
    let is_creating = RwSignal::new(false);

    // QR state
    let qr_svg = RwSignal::new(Option::<String>::None);
    let provisioning_id = RwSignal::new(Option::<Uuid>::None);
    let expires_at = RwSignal::new(0i64);
    let time_left = RwSignal::new(300i32);
    let is_waiting = RwSignal::new(false);
    let is_success = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);

    // Temp keys stored for polling
    let temp_signing_secret = RwSignal::new(Option::<[u8; 32]>::None);
    let temp_signing_public = RwSignal::new(Option::<[u8; 32]>::None);

    // Countdown timer
    let tick_tl = time_left;
    leptos::task::spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            let v = tick_tl.get_untracked();
            if v <= 1 {
                break;
            }
            tick_tl.set(v - 1);
        }
    });

    use std::sync::{Arc, Mutex};
    let nav_for_view = navigate.clone();
    let on_create_request = Arc::new(Mutex::new(Some(Box::new(move || {
        let uname = username.get().trim().to_string();
        if uname.is_empty() {
            error_msg.set(Some(t!("register.username.hint").to_string()));
            return;
        }

        is_creating.set(true);
        error_msg.set(None);

        let sess = session.clone();
        let notif = notifications.clone();
        let nav = navigate.clone();
        let qr_svg_clone = qr_svg;
        let prov_id = provisioning_id;
        let exp_at = expires_at;
        let tl = time_left;
        let waiting = is_waiting;
        let success = is_success;
        let err_msg = error_msg;
        let temp_sk = temp_signing_secret;
        let temp_pk = temp_signing_public;

        spawn_local(async move {
            let api = match build_api_client() {
                Some(a) => a,
                None => {
                    err_msg.set(Some(t!("error.network").to_string()));
                    is_creating.set(false);
                    return;
                }
            };

            // Step 1: Lookup username → user_id
            let lookup_resp = match api.lookup_user_by_username(&uname).await {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("{}: {e}", t!("error.network"));
                    notif.push(ToastKind::Error, &msg);
                    err_msg.set(Some(msg));
                    is_creating.set(false);
                    return;
                }
            };

            // Step 2: Generate temp keypairs
            let mut temp_x25519_seed = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut temp_x25519_seed);
            let temp_x25519_secret = x25519_dalek::StaticSecret::from(temp_x25519_seed);
            let temp_x25519_pub = x25519_dalek::PublicKey::from(&temp_x25519_secret);
            let temp_ed_pair = Ed25519Pair::generate();
            let temp_ed_pub = temp_ed_pair.public_bytes();
            let temp_ed_sec = temp_ed_pair.secret_bytes();

            temp_sk.set(Some(temp_ed_sec));
            temp_pk.set(Some(temp_ed_pub));

            // Step 3: Create provisioning request
            let nonce = Uuid::now_v7().as_bytes().to_vec();
            let prov_req = CreateProvisioningRequest {
                user_id: lookup_resp.user_id,
                new_device_temp_public_key: temp_x25519_pub.to_bytes().to_vec(),
                new_device_temp_signing_public_key: temp_ed_pub.to_vec(),
                nonce,
            };

            let prov_resp = match api.create_provisioning_request(&prov_req).await {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("{}: {e}", t!("error.network"));
                    notif.push(ToastKind::Error, &msg);
                    err_msg.set(Some(msg));
                    is_creating.set(false);
                    return;
                }
            };

            prov_id.set(Some(prov_resp.provisioning_id));
            exp_at.set(prov_resp.expires_at);

            // Step 4: Generate QR code
            let qr_payload = QrPayload {
                server_url: String::new(), // Filled in from session
                user_id: lookup_resp.user_id,
                provisioning_id: prov_resp.provisioning_id,
                new_device_temp_x25519_pub: temp_x25519_pub.to_bytes(),
                new_device_temp_ed25519_pub: temp_ed_pub,
                nonce: prov_req.nonce,
            };

            let qr_server_url = sess.state.with(|s| match s {
                SessionState::ServerConfigured { url } => Some(url.clone()),
                SessionState::Authenticated { .. } => None,
                SessionState::Disconnected => None,
            });
            let mut qr_payload_with_url = qr_payload;
            if let Some(url) = qr_server_url {
                qr_payload_with_url.server_url = url;
            }

            match encode_qr(&qr_payload_with_url) {
                Ok(b64) => {
                    match qrcode::QrCode::new(b64.as_bytes()) {
                        Ok(qr) => {
                            let svg = qr.render::<qrcode::render::svg::Color>()
                                .min_dimensions(256, 256)
                                .build();
                            qr_svg_clone.set(Some(svg));
                        }
                        Err(_) => {
                            err_msg.set(Some(t!("error.network").to_string()));
                        }
                    }
                }
                Err(_) => {
                    err_msg.set(Some(t!("error.network").to_string()));
                }
            }

            // Set countdown
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let remaining = (prov_resp.expires_at - now).max(0).min(300) as i32;
            tl.set(remaining);

            is_creating.set(false);
            waiting.set(true);

            // Step 5: Start polling bootstrap
            let api_clone = api;
            let prov_id_val = prov_resp.provisioning_id;
            let temp_sk_val = temp_ed_sec;
            let temp_pk_val = temp_ed_pub;
            let sess_clone = sess;
            let notif_clone = notif;
            let nav_clone = nav;
            let waiting_clone = waiting;
            let success_clone = success;
            let err_msg_clone = err_msg;

            spawn_local(async move {
                let max_polls = 150; // 150 * 2s = 5 minutes
                for _i in 0..max_polls {
                    gloo_timers::future::TimeoutFuture::new(2000).await;

                    // Check if still waiting
                    if !waiting_clone.get_untracked() {
                        return;
                    }

                    match api_clone
                        .get_bootstrap_with_temp_key(prov_id_val, &temp_sk_val, &temp_pk_val)
                        .await
                    {
                        Ok(resp) => {
                            // Bootstrap blob received!
                            success_clone.set(true);

                            // Decrypt AGE blob (native only)
                            #[cfg(feature = "native")]
                            {
                                match decrypt_bootstrap_blob(
                                    &resp.encrypted_bootstrap_blob,
                                    &temp_x25519_secret,
                                )
                                .await
                                {
                                    Ok((user_id, username_val, identity_seed, device_signing_seed, device_hpke_seed, kp_bundle)) => {
                                        // Create identity for new device using pre-generated seeds
                                        let identity = messenger_core::identity::ClientIdentity::generate_new_device_from_seeds(
                                            user_id,
                                            username_val,
                                            resp.device_id,
                                            identity_seed,
                                            device_signing_seed,
                                            device_hpke_seed,
                                        );

                                        // Save to storage
                                        let url = sess_clone.state.with(|s| match s {
                                            SessionState::ServerConfigured { url } => Some(url.clone()),
                                            _ => None,
                                        }).unwrap_or_default();

                                        if let Ok(local) = messenger_storage::init_storage("default").await {
                                            let encrypted = messenger_storage::EncryptedIdentity {
                                                identity_secret_key_wrapped: identity.identity_signing_key.secret_bytes().to_vec(),
                                                identity_public_key: identity.identity_signing_key.public_bytes().to_vec(),
                                                device_signing_secret_key_wrapped: identity.device_signing_key.secret_bytes().to_vec(),
                                                device_signing_public_key: identity.device_signing_key.public_bytes().to_vec(),
                                                device_hpke_secret_key_wrapped: identity.device_hpke_seed.to_vec(),
                                                device_hpke_public_key: identity.device_hpke_public.to_vec(),
                                            };
                                            let _ = local.save_identity(user_id, &encrypted).await;
                                            let _ = local.set_setting("current_user_id", &user_id.to_string()).await;
                                        }

                                        // Persist session
                                        // Encode full identity for blob storage
                                        let identity_blob = encode_identity_for_qr_blob(&identity);
                                        persist_session(&url, user_id, resp.device_id, &identity_blob, UserRole::User);

                                        // Update session
                                        let identity_arc = Arc::new(identity);
                                        sess_clone.state.set(SessionState::Authenticated {
                                            identity: identity_arc.clone(),
                                            role: UserRole::User,
                                        });

                                        // ── Store KeyPackageBundle in provider ──
                                        if !kp_bundle.is_empty() {
                                            if let Ok(local) = messenger_storage::init_storage("default").await {
                                                let local: Arc<dyn messenger_storage::traits::MessengerLocalStore> = local.into();
                                                use messenger_core::mls::group::MlsRuntime;
                                                let runtime = MlsRuntime::new(local.clone(), resp.device_id);
                                                if let Err(e) = runtime.store_keypackage_bundle(&kp_bundle).await {
                                                    notif_clone.push(ToastKind::Warning, format!("Failed to store keypackage: {e}"));
                                                }

                                                // ── Process pending Welcome messages ──
                                                if let Some(api) = build_api_client() {
                                                    if let Ok(welcomes) = api.list_welcomes(None).await {
                                                        for entry in &welcomes.welcomes {
                                                            match runtime.join_via_welcome(&identity_arc, &entry.welcome_ciphertext).await {
                                                                Ok(_) => {
                                                                    let _ = api.ack_welcome(entry.id).await;
                                                                }
                                                                Err(e) => {
                                                                    notif_clone.push(ToastKind::Warning, format!("Join welcome {} failed: {e}", entry.id));
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        notif_clone.push(ToastKind::Success, t!("qr.success"));
                                        nav_clone("/chats", Default::default());
                                    }
                                    Err(e) => {
                                        err_msg_clone.set(Some(format!("{e}")));
                                        notif_clone.push(ToastKind::Error, format!("{}: {e}", t!("error.network")));
                                    }
                                }
                            }

                            #[cfg(not(feature = "native"))]
                            {
                                // On WASM, AGE decryption is not available.
                                err_msg_clone.set(Some(t!("error.network").to_string()));
                                notif_clone.push(ToastKind::Error, t!("error.network"));
                            }

                            waiting_clone.set(false);
                            return;
                        }
                        Err(e) => {
                            // Check if expired
                            if e.error_code() == Some("ERR_PROVISIONING_EXPIRED")
                                || e.error_code() == Some("ERR_PROVISIONING_NOT_FOUND")
                            {
                                err_msg_clone.set(Some(t!("token.error.expired").to_string()));
                                notif_clone.push(ToastKind::Warning, t!("token.error.expired"));
                                waiting_clone.set(false);
                                return;
                            }
                            // 202 Accepted — continue polling
                            // Other errors — continue polling
                            continue;
                        }
                    }
                }

                // Timed out
                err_msg_clone.set(Some(t!("token.error.expired").to_string()));
                notif_clone.push(ToastKind::Warning, t!("token.error.expired"));
                waiting_clone.set(false);
            });
        });
    }) as Box<dyn FnOnce() + Send>)));

    let on_reset = move || {
        qr_svg.set(None);
        provisioning_id.set(None);
        time_left.set(300);
        is_waiting.set(false);
        is_success.set(false);
        error_msg.set(None);
        temp_signing_secret.set(None);
        temp_signing_public.set(None);
    };

    let format_time = move || {
        let secs = time_left.get();
        format!("{}:{:02}", secs / 60, secs % 60)
    };

    view! {
        <div class="flex min-h-screen flex-col bg-background">
            <header class="flex items-center gap-4 border-b border-border p-4">
                <button
                    class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                    on:click=move |_| nav_for_view("/login", Default::default())
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
                </button>
            </header>

            <main class="flex flex-1 flex-col items-center justify-center p-4">
                <div class="w-full max-w-md space-y-8">
                    <div class="space-y-2 text-center">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">{t!("qr.title")}</h1>
                        <p class="text-sm leading-relaxed text-muted-foreground">{t!("qr.instruction")}</p>
                    </div>

                    {move || {
                        if qr_svg.get().is_none() && !is_waiting.get() && !is_success.get() {
                            // Show username input form
                            view! {
                                <div class="space-y-4">
                                    <div class="space-y-2">
                                        <label class="text-sm font-medium text-foreground">{t!("register.username")}</label>
                                        <input
                                            type="text"
                                            placeholder="johndoe"
                                            class="flex h-12 w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                                            disabled=is_creating
                                            prop:value=username
                                            on:input=move |ev| username.set(event_target_value(&ev))
                                        />
                                    </div>

                                    {move || error_msg.get().map(|e| {
                                        view! {
                                            <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                                <p class="text-sm">{e}</p>
                                            </div>
                                        }
                                    })}

                                    <button
                                        class="inline-flex h-12 w-full items-center justify-center rounded-md bg-primary text-sm font-medium text-primary-foreground ring-offset-background transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
                                        disabled=move || is_creating.get() || username.get().trim().is_empty()
                                        on:click={
                                            let req = on_create_request.clone();
                                            move |_| {
                                                let cb = req.lock().unwrap().take();
                                                if let Some(f) = cb { f(); }
                                            }
                                        }
                                    >
                                        {move || if is_creating.get() { t!("loading") } else { t!("qr.create") }}
                                    </button>
                                </div>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }
                    }}

                    {move || qr_svg.get().map(|svg| {
                        view! {
                            <div class="flex flex-col items-center space-y-4">
                                <div class="relative">
                                    <div class="flex h-64 w-64 items-center justify-center rounded-2xl border border-border bg-card">
                                        <div inner_html=svg.clone()/>

                                        {move || if is_success.get() {
                                            view! {
                                                <div class="absolute inset-0 flex items-center justify-center rounded-2xl bg-background/90 backdrop-blur-sm">
                                                    <div class="flex flex-col items-center gap-2">
                                                        <svg class="h-12 w-12 text-green-500" xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                                                        <span class="text-sm font-medium text-foreground">{t!("qr.success")}</span>
                                                    </div>
                                                </div>
                                            }.into_any()
                                        } else if is_waiting.get() {
                                            view! {
                                                <div class="absolute inset-0 flex items-center justify-center rounded-2xl bg-background/90 backdrop-blur-sm">
                                                    <div class="flex flex-col items-center gap-2">
                                                        <span class="h-8 w-8 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                                                        <span class="text-sm text-muted-foreground">{t!("qr.waiting")}</span>
                                                    </div>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }}
                                    </div>
                                </div>

                                {move || if !is_success.get() {
                                    view! {
                                        <>
                                            {move || error_msg.get().map(|e| {
                                                view! {
                                                    <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                                        <p class="text-sm">{e}</p>
                                                    </div>
                                                }
                                            })}

                                            <div class="flex items-center gap-2 text-sm text-muted-foreground">
                                                <span>{t!("qr.validFor")}</span>
                                                <span class="font-mono">{format_time()}</span>
                                            </div>
                                            <button
                                                class="inline-flex items-center justify-center gap-2 rounded-md border border-input bg-background h-10 px-4 py-2 text-sm font-medium hover:bg-accent"
                                                on:click=move |_| on_reset()
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M3 21v-5h5"/></svg>
                                                {t!("qr.refresh")}
                                            </button>
                                        </>
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }}

                                {move || provisioning_id.get().map(|id| {
                                    view! {
                                        <div class="text-center">
                                            <span class="text-xs text-muted-foreground">{t!("qr.requestId")}: </span>
                                            <code class="font-mono text-xs text-muted-foreground">{id.to_string()}</code>
                                        </div>
                                    }
                                })}
                            </div>
                        }
                    })}
                </div>
            </main>
        </div>
    }
}

/// Decrypt an AGE bootstrap blob using the temp X25519 secret key (native only).
/// Returns (user_id, username, identity_seed, device_signing_seed, device_hpke_seed, key_package_bundle).
#[cfg(feature = "native")]
async fn decrypt_bootstrap_blob(
    blob: &[u8],
    temp_secret: &x25519_dalek::StaticSecret,
) -> Result<(Uuid, String, [u8; 32], [u8; 32], [u8; 32], Vec<u8>), Box<dyn std::error::Error>> {
    use messenger_core::bootstrap::open_bootstrap_raw_secret;

    let payload = open_bootstrap_raw_secret(blob, &temp_secret.to_bytes())?;
    Ok((
        payload.user_id,
        payload.username,
        payload.identity_signing_seed,
        payload.device_signing_seed,
        payload.device_hpke_seed,
        payload.key_package_bundle,
    ))
}

/// Encode identity for blob storage (CBOR).
fn encode_identity_for_qr_blob(identity: &messenger_core::identity::ClientIdentity) -> Vec<u8> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct IdentityBlob<'a> {
        user_id: Uuid,
        username: &'a str,
        identity_seed: [u8; 32],
        device_id: Uuid,
        device_signing_seed: [u8; 32],
        device_hpke_seed: [u8; 32],
        device_hpke_public: [u8; 32],
    }
    let blob = IdentityBlob {
        user_id: identity.user_id,
        username: &identity.username,
        identity_seed: identity.identity_signing_key.secret_bytes(),
        device_id: identity.device_id,
        device_signing_seed: identity.device_signing_key.secret_bytes(),
        device_hpke_seed: identity.device_hpke_seed,
        device_hpke_public: identity.device_hpke_public,
    };
    rmp_serde::to_vec_named(&blob).unwrap_or_default()
}

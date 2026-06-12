use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_query};
use leptos_router::params::Params;
use leptos_router::NavigateOptions;
use rand::RngCore;
use messenger_core::api::client::{ApiClient, AuthCredentials};
use messenger_core::ed25519::Ed25519Pair;
use messenger_core::identity::ClientIdentity;
use messenger_proto::auth::{RedeemKind, RedeemRequest};
use messenger_proto::keypackages::PublishKeyPackagesRequest;
use uuid::Uuid;
use crate::i18n::I18n;
use crate::session::restore::persist_session;
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::ws_manager::WsManager;
use crate::state::session::{build_api_client, persist_auth_credentials, use_session, SessionState, UserRole};
use crate::t;

/// Log a message to browser console (WASM only, no-op on native).
fn js_log(msg: impl std::fmt::Display) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&msg.to_string()));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = msg;
}

/// Query parameters for the register screen.
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
struct RegisterQuery {
    token: Option<String>,
}

impl Params for RegisterQuery {
    fn from_map(map: &leptos_router::params::ParamsMap) -> Result<Self, leptos_router::params::ParamsError> {
        let token = map.get("token");
        Ok(Self { token })
    }
}

#[must_use]
#[component]
pub fn RegisterScreen() -> impl IntoView {
    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let session = use_session();
    let navigate = use_navigate();
    let notifications = use_context::<NotificationsState>()
        .expect("NotificationsState must be provided");

    let query = use_query::<RegisterQuery>();

    // If already authenticated, redirect to chats
    let nav_on_mount = navigate.clone();
    let sess_on_mount = session.clone();
    Effect::new(move |_| {
        if sess_on_mount.is_authenticated() {
            nav_on_mount("/chats", NavigateOptions { replace: true, ..Default::default() });
        }
    });

    let username = RwSignal::new(String::new());
    let display_name = RwSignal::new(String::new());
    let avatar = RwSignal::new(Option::<String>::None);
    let username_status = RwSignal::new("idle".to_string());
    let is_submitting = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    let on_username_change = move |value: &str| {
        let cleaned: String = value
            .chars()
            .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        username.set(cleaned);
    };

    let is_valid = move || {
        let u = username.get();
        let d = display_name.get();
        u.len() >= 3 && u.len() <= 32 && !d.is_empty() && d.len() <= 64
    };

    let nav_for_view = navigate.clone();
    let on_submit = move || {
        let token_raw = query.with(|q| {
            q.as_ref()
                .ok()
                .and_then(|q| q.token.clone())
        });
        let token: String = match token_raw {
            // Bootstrap/admin tokens are base64url-no-pad, so '-' and '_' are part of
            // the token alphabet. Drop only whitespace to allow accidental wrapping/copy
            // without mangling the payload.
            Some(t) if !t.is_empty() => t.chars().filter(|c| !c.is_whitespace()).collect(),
            _ => {
                error.set(Some(t!("token.error.invalid").to_string()));
                return;
            }
        };

        let uname = username.get();
        let dname = display_name.get();

        if uname.len() < 3 || dname.is_empty() {
            return;
        }

        is_submitting.set(true);
        error.set(None);

        let nav = navigate.clone();
        let sess = session.clone();
        let notif = notifications.clone();
        let i18n_clone = i18n.clone();
        let is_submit = is_submitting;
        let err_signal = error;
        let timeout_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        spawn_local(async move {
            js_log("[register] async block started");
            
            // Safety timeout: if registration takes > 20s, show error and re-enable form.
            let timeout_submit = is_submit.clone();
            let timeout_err = err_signal.clone();
            let timeout_notif = notif.clone();
            let timeout_cancel = timeout_cancelled.clone();
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(20_000).await;
                if !timeout_cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    timeout_err.set(Some("Registration timed out after 20s — check server connection".into()));
                    timeout_notif.push(ToastKind::Error, "Registration timed out");
                    timeout_submit.set(false);
                }
            });
            let mut api = match build_api_client() {
                Some(c) => c,
                None => {
                    js_log("[register] ERROR: build_api_client() returned None");
                    timeout_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    notif.push(ToastKind::Error, i18n_clone.t("error.network"));
                    is_submitting.set(false);
                    return;
                }
            };
            js_log("[register] build_api_client() OK");

            // Step 1: Generate key material
            let identity_signing_key = Ed25519Pair::generate();
            let device_signing_key = Ed25519Pair::generate();
            let mut hpke_seed = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut hpke_seed);
            let hpke_secret = x25519_dalek::StaticSecret::from(hpke_seed);
            let hpke_public = x25519_dalek::PublicKey::from(&hpke_secret).to_bytes();

            let identity_pub = identity_signing_key.public_bytes();
            let device_signing_pub = device_signing_key.public_bytes();
            js_log("[register] keys generated");

            // Step 2: Build device authorization signature
            let ts = messenger_core::api::signing::now_secs();
            let mut auth_msg = Vec::new();
            auth_msg.extend_from_slice(&device_signing_pub);
            auth_msg.extend_from_slice(&hpke_public);
            auth_msg.extend_from_slice(&ts.to_le_bytes());
            let device_auth_sig = identity_signing_key.sign(&auth_msg);

            // Step 3: Build credential bytes (simple len-prefixed identity public key)
            // Full MLS credential serialization (tls_codec) is deferred until MLS integration.
            let credential_bytes: Vec<u8> = {
                let mut buf = Vec::with_capacity(2 + identity_pub.len());
                buf.extend_from_slice(&(identity_pub.len() as u16).to_be_bytes());
                buf.extend_from_slice(&identity_pub);
                buf
            };
            js_log("[register] credential built, sending redeem request...");

            // Step 5: Send redeem request
            let req = RedeemRequest {
                token: token.clone(),
                kind: RedeemKind::NewUser,
                identity_credential: Some(credential_bytes),
                signature_public_key: Some(identity_pub.to_vec()),
                username: Some(uname.clone()),
                existing_identity_proof: None,
                device_init_public_key: hpke_public.to_vec(),
                device_signing_public_key: device_signing_pub.to_vec(),
                device_authorization_signature: device_auth_sig.to_vec(),
                device_authorization_timestamp: ts,
            };

            let resp = match api.redeem_invite(&req).await {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = match e.error_code() {
                        Some("ERR_INVITE_INVALID") => i18n_clone.t("error.invite_invalid"),
                        Some("ERR_INVITE_EXPIRED") => i18n_clone.t("error.invite_expired"),
                        Some("ERR_INVITE_EXHAUSTED") => i18n_clone.t("error.invite_exhausted"),
                        Some("ERR_USERNAME_TAKEN") => i18n_clone.t("error.username_taken"),
                        Some(code) => format!("{code}: {e}"),
                        None => format!("{e}"),
                    };
                    js_log(&format!("[register] redeem failed: {err_msg}"));
                    notif.push(ToastKind::Error, &err_msg);
                    timeout_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    error.set(Some(err_msg));
                    is_submitting.set(false);
                    return;
                }
            };
            js_log(&format!("[register] redeem SUCCESS: user_id={}, device_id={}", resp.user_id, resp.device_id));

            // Step 6: Create ClientIdentity with server-assigned IDs
            // Extract device secret before moving into ClientIdentity
            let device_secret = device_signing_key.secret_bytes();
            let identity = ClientIdentity {
                user_id: resp.user_id,
                username: uname.clone(),
                identity_signing_key,
                device_id: resp.device_id,
                device_signing_key,
                device_hpke_seed: hpke_seed,
                device_hpke_public: hpke_public,
            };
            js_log("[register] identity created, saving to storage...");

            // Step 7: Save identity to local storage
            let server_url = sess.state.with(|s| match s {
                SessionState::ServerConfigured { url } => Some(url.clone()),
                _ => None,
            });
            let url = server_url
                .filter(|u| !u.is_empty())
                .or_else(crate::state::session::load_server_url)
                .unwrap_or_default();

            if let Ok(local) = messenger_storage::init_storage("default").await {
                let encrypted = messenger_storage::EncryptedIdentity {
                    identity_secret_key_wrapped: identity.identity_signing_key.secret_bytes().to_vec(),
                    identity_public_key: identity.identity_signing_key.public_bytes().to_vec(),
                    device_signing_secret_key_wrapped: identity.device_signing_key.secret_bytes().to_vec(),
                    device_signing_public_key: identity.device_signing_key.public_bytes().to_vec(),
                    device_hpke_secret_key_wrapped: identity.device_hpke_seed.to_vec(),
                    device_hpke_public_key: identity.device_hpke_public.to_vec(),
                };
                let _ = local.save_identity(resp.user_id, &encrypted).await;
                let _ = local.set_setting("current_user_id", &resp.user_id.to_string()).await;
                let _ = local.set_setting("user_role", &resp.role).await;
            }

            // Persist to localStorage for session restore
            let role = if resp.role == "admin" {
                UserRole::Admin
            } else {
                UserRole::User
            };
            persist_session(
                &url,
                resp.user_id,
                resp.device_id,
                &encode_identity_for_blob(&identity),
                role,
            );

            // Step 8: Configure ApiClient with auth
            let auth = AuthCredentials {
                device_id: resp.device_id,
                device_signing_secret: device_secret,
            };
            api.set_auth(Some(auth));
            persist_auth_credentials(resp.device_id, &device_secret);
            js_log("[register] auth configured");

            // Step 9: Publish initial KeyPackages (native only — too slow on WASM)
            #[cfg(feature = "native")]
            {
                use messenger_core::mls::keypackage::generate_keypackage;
                use openmls_rust_crypto::OpenMlsRustCrypto;

                let mls_provider = OpenMlsRustCrypto::default();
                // Generate 5 initial key packages (1 last-resort + 4 regular).
                let mut key_packages = Vec::new();
                // Last-resort KP
                if let Ok(kp) = generate_keypackage(&mls_provider, &identity, 2_592_000, true) {
                    key_packages.push(kp.key_package_bytes.into());
                }
                // Regular KPs (7-day lifetime)
                for _ in 0..4 {
                    if let Ok(kp) = generate_keypackage(&mls_provider, &identity, 604_800, false) {
                        key_packages.push(kp.key_package_bytes.into());
                    }
                }
                if !key_packages.is_empty() {
                    let _ = api
                        .publish_keypackages(&PublishKeyPackagesRequest {
                            key_packages,
                        })
                        .await;
                }
            }
            #[cfg(not(feature = "native"))]
            {
                js_log("[register] WASM: skipping keypackages publish (MLS too slow on WASM)");
            }

            // Step 10: Persist chosen avatar locally (broadcast happens when
            // chats are created — a fresh account has no groups yet).
            if let Some(data_url) = avatar.get_untracked() {
                crate::state::avatar_store::save_own_avatar(resp.user_id, &data_url);
            }

            // Step 11: Update session state
            sess.state.set(SessionState::Authenticated {
                identity: Arc::new(identity),
                role,
            });

            // Step 12: Start WebSocket connection for real-time notifications
            if let Some(ws) = use_context::<WsManager>() {
                let url = url.clone();
                let auth = AuthCredentials {
                    device_id: resp.device_id,
                    device_signing_secret: device_secret,
                };
                ws.connect(&url, auth);
            }

            timeout_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            notif.push(ToastKind::Success, i18n_clone.t("register.success"));
            is_submit.set(false);
            js_log("[register] navigating to /chats");
            nav("/chats", NavigateOptions { replace: true, ..Default::default() });
            js_log("[register] nav() called");
        });
    };
    view! {
        <div class="flex h-screen-safe flex-col bg-background overflow-hidden">
            <header class="flex items-center gap-4 border-b border-border p-4">
                <button
                    class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                    on:click={
                        let nav = nav_for_view.clone();
                        move |_| nav("/login/token", Default::default())
                    }
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
                </button>
            </header>

            <main class="flex flex-1 flex-col items-center justify-center p-4">
                <div class="w-full max-w-md space-y-8">
                    <div class="space-y-2 text-center">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">{t!("register.title")}</h1>
                    </div>

                    <div class="space-y-6">
                        <div class="flex flex-col items-center space-y-3">
                            <crate::components::avatar_picker::AvatarPicker value=avatar size_class="h-24 w-24"/>
                            <p class="text-sm text-muted-foreground">{t!("register.avatar.hint")}</p>
                        </div>

                        <div class="space-y-2">
                            <label class="text-sm font-medium text-foreground">{t!("register.username")}</label>
                            <div class="relative">
                                <input
                                    type="text"
                                    placeholder="johndoe"
                                    maxlength=32u32
                                    class="flex h-12 w-full rounded-md border border-input bg-background px-3 py-2 pr-10 text-sm font-mono ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                    disabled=is_submitting
                                    prop:value=username
                                    on:input=move |ev| on_username_change(&event_target_value(&ev))
                                />
                                <div class="absolute right-3 top-1/2 -translate-y-1/2">
                                    {move || match username_status.get().as_str() {
                                        "checking" => view! { <span class="h-5 w-5 block rounded-full border-2 border-muted-foreground border-t-transparent animate-spin"/> }.into_any(),
                                        _ => view! {}.into_any(),
                                    }}
                                </div>
                            </div>
                            <p class="text-xs text-muted-foreground">
                                {move || match username_status.get().as_str() {
                                    "taken" => t!("register.username.taken").to_string(),
                                    "available" => t!("register.username.available").to_string(),
                                    _ => t!("register.username.hint").to_string(),
                                }}
                            </p>
                        </div>

                        <div class="space-y-2">
                            <label class="text-sm font-medium text-foreground">{t!("register.displayName")}</label>
                            <input
                                type="text"
                                placeholder="John Doe"
                                maxlength=64u32
                                class="flex h-12 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                disabled=is_submitting
                                prop:value=display_name
                                on:input=move |ev| display_name.set(event_target_value(&ev))
                            />
                        </div>

                        {move || error.get().map(|e| {
                            view! {
                                <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                    <p class="text-sm">{e}</p>
                                </div>
                            }
                        })}

                        <button
                            class="inline-flex h-12 w-full items-center justify-center rounded-md bg-primary text-sm font-medium text-primary-foreground ring-offset-background transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
                            disabled={move || is_submitting.get() || !is_valid()}
                            on:click=move |_| on_submit()
                        >
                            {move || if is_submitting.get() { t!("loading") } else { t!("register.create") }}
                        </button>

                        <p class="text-center text-xs text-muted-foreground">{t!("register.privacy")}</p>
                    </div>
                </div>
            </main>
        </div>
    }
}

/// Encode identity for blob storage (CBOR → base64).
fn encode_identity_for_blob(identity: &ClientIdentity) -> Vec<u8> {
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

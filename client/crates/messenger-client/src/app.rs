use std::sync::Arc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_core::api::client::AuthCredentials;
use messenger_proto::ws::ClientFrame;
use crate::routes::AppRoutes;
use crate::t;
use crate::theme::{provide_theme, restore_theme, restore_locale, persist_locale};
use crate::i18n::{Locale, I18n};
use crate::state::message_service::MessageService;
use crate::state::provide_app_state;
use crate::state::session::{persist_auth_credentials, persist_server_url, load_server_url, Session, SessionState, UserRole};
use crate::state::sync_service::SyncService;
use crate::state::ws_manager::WsManager;
use crate::session::restore::try_restore_session;
use crate::components::toast_container::ToastContainer;

/// Error boundary fallback — shown on any child panic.
#[component]
fn ErrorFallback(errors: ArcRwSignal<Errors>) -> impl IntoView {
    let msg = errors
        .get()
        .iter()
        .next()
        .map(|(_, e)| format!("{:?}", e))
        .unwrap_or_else(|| "Unknown error".to_string());
    web_sys::console::error_1(
        &wasm_bindgen::JsValue::from_str(&format!("ErrorBoundary caught: {}", msg)),
    );
    view! {
        <div class="flex h-screen-safe items-center justify-center bg-background p-8">
            <div class="max-w-md text-center space-y-4">
                <div class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-destructive/10">
                    <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-destructive"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
                </div>
                <h2 class="text-lg font-semibold text-foreground">{t!("error.title")}</h2>
                <p class="text-sm text-muted-foreground break-all font-mono">{msg}</p>
            </div>
        </div>
    }
}

/// Start the WebSocket connection with stored auth credentials.
fn start_ws_connection(ws: &WsManager) {
    let device_id_str = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|s| s.get_item("messenger_device_id").ok().flatten());
    let secret_b64 = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|s| s.get_item("messenger_device_signing_secret").ok().flatten());
    let server_url = load_server_url();

    if let (Some(device_id_str), Some(secret_b64), Some(server_url)) = (device_id_str, secret_b64, server_url) {
        use base64::Engine;
        if let (Ok(device_id), Ok(secret_bytes)) = (device_id_str.parse::<uuid::Uuid>(), base64::engine::general_purpose::STANDARD.decode(&secret_b64)) {
            if secret_bytes.len() == 32 {
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&secret_bytes);
                let auth = AuthCredentials {
                    device_id,
                    device_signing_secret: secret,
                };
                ws.connect(&server_url, auth);
            }
        }
    }
}

#[must_use]
#[component]
pub fn App() -> impl IntoView {
    // 0. Log app start
    web_sys::console::log_1(&"[App] Starting...".into());

    // 1. Restore persisted theme before first render.
    let initial_theme = restore_theme();
    let _theme = provide_theme(initial_theme);

    // 2. Provide all application state.
    provide_app_state();

    // 3. Provide reactive i18n.
    let initial_locale = restore_locale().unwrap_or(Locale::System);
    let i18n = I18n::new();
    i18n.locale.set(initial_locale);
    provide_context(i18n.clone());

    // 4. Persist locale on change.
    Effect::new(move |_| {
        let loc = i18n.locale.get();
        persist_locale(&loc);
    });

    // 5. Provide WebSocket manager (starts disconnected).
    let ws = WsManager::new();
    provide_context(ws.clone());

    // 5b. Provide SyncService (starts stopped — started after session restore).
    let sync = SyncService::new();
    provide_context(sync.clone());

    // 5c. Bring up MLS + sync + WS whenever the session becomes Authenticated,
    // however that happens: session restore, fresh registration, OR QR device
    // provisioning. Previously this only ran inside the restore path, so a
    // provisioned/registered device had no live MLS runtime or sync loop until
    // the app was restarted — it would log in but never receive group messages.
    {
        let session = use_context::<Session>().expect("Session must be provided");
        let msg_svc = use_context::<MessageService>().expect("MessageService must be provided");
        let sync = sync.clone();
        let ws = ws.clone();
        let started = StoredValue::new(false);
        Effect::new(move |_| {
            let device_id = match session.state.get() {
                SessionState::Authenticated { ref identity, .. } => identity.device_id,
                _ => return,
            };
            if started.get_value() {
                return;
            }
            started.set_value(true);
            start_ws_connection(&ws);
            let msg_svc = msg_svc.clone();
            let sync = sync.clone();
            spawn_local(async move {
                msg_svc.init_mls(device_id).await;
                sync.start();
            });
        });
    }

    // 6. Try session restore in background — full identity recovery.
    let session = use_context::<Session>().expect("Session must be provided");
    let msg_svc = use_context::<MessageService>().expect("MessageService must be provided");
    spawn_local(async move {
        // On Android, sync the Keystore copy of credentials back into the
        // WebView's localStorage if it was cleared (e.g. data-wipe-on-update).
        crate::state::session::restore_credentials_from_keystore().await;
        web_sys::console::log_1(&"[App] Attempting session restore...".into());
        if let Some(restored) = try_restore_session().await {
            web_sys::console::log_1(&"[App] Session data found, restoring identity...".into());
            if let Some(identity) = restored.restore_identity() {
                let device_id = identity.device_id;
                persist_server_url(&restored.server_url);
                persist_auth_credentials(
                    device_id,
                    &identity.device_signing_key.secret_bytes(),
                );

                session.state.set(SessionState::Authenticated {
                    identity: Arc::new(identity),
                    role: restored.role,
                });

                web_sys::console::log_1(&"[App] Session restored".into());
                // WS, MLS and sync are brought up by the Authenticated-session
                // effect above (5c) — shared with the registration and QR
                // provisioning paths.
                let _ = device_id;
            } else {
                web_sys::console::log_1(
                    &"[App] Identity blob malformed, setting ServerConfigured".into(),
                );
                persist_server_url(&restored.server_url);
                session.state.set(SessionState::ServerConfigured {
                    url: restored.server_url,
                });
            }
        } else {
            web_sys::console::log_1(&"[App] No saved session found".into());
        }
    });

    web_sys::console::log_1(&"[App] Rendering view...".into());

    view! {
        <ErrorBoundary fallback=|errors| view! { <ErrorFallback errors=errors/> }>
            <main class="h-full bg-background text-foreground">
                <AppRoutes/>
                <ToastContainer/>
            </main>
        </ErrorBoundary>
    }
}

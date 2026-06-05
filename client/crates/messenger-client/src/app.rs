use std::sync::Arc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::routes::AppRoutes;
use crate::theme::{provide_theme, restore_theme, restore_locale, persist_locale};
use crate::i18n::{Locale, I18n};
use crate::state::provide_app_state;
use crate::state::session::{persist_auth_credentials, persist_server_url, Session, SessionState, UserRole};
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
                <h2 class="text-lg font-semibold text-foreground">"Something went wrong"</h2>
                <p class="text-sm text-muted-foreground break-all font-mono">{msg}</p>
            </div>
        </div>
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

    // 5. Try session restore in background — full identity recovery.
    let session = use_context::<Session>().expect("Session must be provided");
    spawn_local(async move {
        web_sys::console::log_1(&"[App] Attempting session restore...".into());
        if let Some(restored) = try_restore_session().await {
            web_sys::console::log_1(&"[App] Session data found, restoring identity...".into());
            // If we have a full identity blob, reconstruct ClientIdentity
            // and set Authenticated state.
            if let Some(identity) = restored.restore_identity() {
                // Persist server URL + auth credentials for future API calls.
                persist_server_url(&restored.server_url);
                persist_auth_credentials(
                    identity.device_id,
                    &identity.device_signing_key.secret_bytes(),
                );

                session.state.set(SessionState::Authenticated {
                    identity: Arc::new(identity),
                    role: restored.role,
                });

                web_sys::console::log_1(&"[App] Session restored, navigating to /chats".into());

                // Navigate to chats so the user doesn't see ConnectScreen.
                let navigate = leptos_router::hooks::use_navigate();
                navigate("/chats", Default::default());
            } else {
                web_sys::console::log_1(
                    &"[App] Identity blob malformed, setting ServerConfigured".into(),
                );
                // Identity blob is malformed; just set server URL so the user
                // sees the login screen instead of re-entering the URL.
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

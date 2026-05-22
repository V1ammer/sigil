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

#[must_use]
#[component]
pub fn App() -> impl IntoView {
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
        if let Some(restored) = try_restore_session().await {
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
            } else {
                // Identity blob is malformed; just set server URL so the user
                // sees the login screen instead of re-entering the URL.
                persist_server_url(&restored.server_url);
                session.state.set(SessionState::ServerConfigured {
                    url: restored.server_url,
                });
            }
        }
    });

    view! {
        <main class="h-full bg-background text-foreground">
            <AppRoutes/>
            <ToastContainer/>
        </main>
    }
}

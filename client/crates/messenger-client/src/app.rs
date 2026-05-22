use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::routes::AppRoutes;
use crate::theme::{self, Theme, provide_theme, restore_theme, restore_locale, persist_locale};
use crate::i18n::{self, Locale, I18n};
use crate::state::provide_app_state;
use crate::state::session::{Session, SessionState, UserRole};
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

    // 5. Try session restore in background.
    let session = use_context::<Session>().expect("Session must be provided");
    spawn_local(async move {
        if let Some(restored) = try_restore_session().await {
            // We have a stored session — mark as authenticated.
            session.state.set(SessionState::ServerConfigured {
                url: restored.server_url,
            });
            // NOTE: full identity restoration requires a real
            // `ClientIdentity` from the blob, which will be wired in C07+
            // when we have the decryption logic. For now, we just set the
            // server URL so the user lands on /chats.
        }
    });

    view! {
        <main class="h-full bg-background text-foreground">
            <AppRoutes/>
            <ToastContainer/>
        </main>
    }
}

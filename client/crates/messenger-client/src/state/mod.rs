//! Global application state — signals and contexts for the whole app.
//!
//! Each submodule defines a piece of state that is `provide_context`'d at the
//! top of the component tree and accessed via `use_context` in screens.

pub mod session;
pub mod chats;
pub mod messages;
pub mod threads;
pub mod ui;
pub mod settings;
pub mod connectivity;
pub mod notifications;

pub use session::*;
pub use chats::*;
pub use messages::*;
pub use threads::*;
pub use ui::*;
pub use settings::*;
pub use connectivity::*;
pub use notifications::*;

use leptos::prelude::*;

/// Provide every piece of global state into the context hierarchy.
/// Must be called once at the top of `<App />`.
pub fn provide_app_state() {
    session::provide_session();
    provide_context(ChatsState::new());
    provide_context(MessagesState::new());
    provide_context(ThreadsState::new());
    provide_context(UiState::new());
    provide_context(SettingsState::new());
    provide_context(ConnectivityState::new());
    provide_context(NotificationsState::new());
}

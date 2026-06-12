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
pub mod users;
pub mod ws_manager;
pub mod message_service;
pub mod sync_service;
pub mod back_stack;
pub mod avatar_store;

pub use session::*;
pub use chats::*;
pub use messages::*;
pub use threads::*;
pub use ui::*;
pub use settings::*;
pub use connectivity::*;
pub use notifications::*;
pub use users::*;

use crate::state::message_service::{init_message_service_context, MessageService};
use leptos::prelude::*;

/// Provide every piece of global state into the context hierarchy.
/// Must be called once at the top of `<App />`.
pub fn provide_app_state() {
    let session = session::provide_session();
    let chats = ChatsState::new();
    provide_context(chats.clone());
    provide_context(MessagesState::new());
    provide_context(ThreadsState::new());
    provide_context(UiState::new());
    provide_context(SettingsState::new());
    provide_context(ConnectivityState::new());
    provide_context(NotificationsState::new());
    let users = UsersState::new();
    provide_context(users.clone());
    provide_context(MessageService::new());
    // Wire up state for code paths that run outside the leptos owner
    // (nested spawn_local tasks in the voice/attachment pipelines).
    init_message_service_context(&session, users, chats);
}

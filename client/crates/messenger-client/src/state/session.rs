//! Session state — auth context, current user, current device.

use leptos::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

use messenger_core::identity::ClientIdentity;

#[derive(Clone, Debug)]
pub enum SessionState {
    /// No server URL configured yet — first-launch state.
    Disconnected,
    /// Server URL is known but not authenticated.
    ServerConfigured { url: String },
    /// Authenticated with a full identity.
    Authenticated {
        identity: Arc<ClientIdentity>,
        role: UserRole,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UserRole {
    User,
    Admin,
}

/// Top-level session handle.
///
/// Holds the session state signal.
/// An `ApiClient` reference will be added in C07+ when transport is wired.
#[derive(Clone)]
pub struct Session {
    pub state: RwSignal<SessionState>,
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwSignal::new(SessionState::Disconnected),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        matches!(self.state.get(), SessionState::Authenticated { .. })
    }

    pub fn is_admin(&self) -> bool {
        matches!(
            self.state.get(),
            SessionState::Authenticated {
                role: UserRole::Admin,
                ..
            }
        )
    }

    pub fn current_user_id(&self) -> Option<Uuid> {
        match self.state.get() {
            SessionState::Authenticated { identity, .. } => Some(identity.user_id),
            _ => None,
        }
    }

    pub fn current_device_id(&self) -> Option<Uuid> {
        match self.state.get() {
            SessionState::Authenticated { identity, .. } => Some(identity.device_id),
            _ => None,
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a `Session`, provide it as context, and return it.
pub fn provide_session() -> Session {
    let s = Session::new();
    provide_context(s.clone());
    s
}

/// Retrieve the `Session` from the context hierarchy.
///
/// # Panics
///
/// Panics if `Session` was not provided via [`provide_session`] at the app root.
pub fn use_session() -> Session {
    use_context::<Session>().expect("Session must be provided via provide_session()")
}

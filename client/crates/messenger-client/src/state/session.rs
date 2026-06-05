use std::sync::Arc;

use leptos::prelude::*;
use uuid::Uuid;

use messenger_core::api::client::{ApiClient, AuthCredentials};
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

/// Build a fresh `ApiClient` from persisted credentials in local storage.
///
/// Creates a new client each time — avoids `Send + Sync` issues that arise
/// from storing `ApiClient` (which contains `Box<dyn HttpTransport>`) in
/// reactive signals or statics.
pub fn build_api_client() -> Option<ApiClient> {
    let url = load_server_url()?;
    if url.is_empty() {
        return None;
    }
    let mut client = ApiClient::new(url);
    if let Some(auth) = load_auth_credentials() {
        client.set_auth(Some(auth));
    }
    Some(client)
}

/// Persist auth credentials to local storage.
pub fn persist_auth_credentials(device_id: Uuid, device_signing_secret: &[u8; 32]) {
    use base64::Engine;
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("messenger_device_id", &device_id.to_string());
        let _ = storage.set_item(
            "messenger_device_signing_secret",
            &base64::engine::general_purpose::STANDARD.encode(device_signing_secret),
        );
    }
}

/// Persist server URL to local storage.
pub fn persist_server_url(url: &str) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("messenger_server_url", url);
    }
}

/// Load auth credentials from local storage.
fn load_auth_credentials() -> Option<AuthCredentials> {
    use base64::Engine;
    let window = web_sys::window()?;
    let storage = window.local_storage().ok().flatten()?;

    let device_id_str = storage.get_item("messenger_device_id").ok().flatten()?;
    let secret_b64 = storage
        .get_item("messenger_device_signing_secret")
        .ok()
        .flatten()?;

    let device_id: Uuid = device_id_str.parse().ok()?;
    let secret_bytes = base64::engine::general_purpose::STANDARD
        .decode(&secret_b64)
        .ok()?;
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&secret_bytes);

    Some(AuthCredentials {
        device_id,
        device_signing_secret: secret,
    })
}

/// Load server URL from local storage.
pub fn load_server_url() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok().flatten()?;
    storage.get_item("messenger_server_url").ok().flatten()
}

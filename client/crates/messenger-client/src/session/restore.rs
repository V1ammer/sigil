//! Session restore — try to recover a previous session from local storage.
//!
//! Called once at app startup. If a stored identity and server URL are found,
//! the session is restored to `ServerConfigured` or `Authenticated` state
//! without re-authentication.

use messenger_core::ed25519::Ed25519Pair;
use messenger_core::identity::ClientIdentity;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::session::UserRole;

/// Data recovered from local storage that can be used to restore a session.
#[derive(Debug, Clone)]
pub struct RestoredSession {
    pub server_url: String,
    pub identity_blob: Vec<u8>,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub role: UserRole,
}

impl RestoredSession {
    /// Try to reconstruct a `ClientIdentity` from the stored blob.
    ///
    /// Returns `None` if the blob is malformed or missing.
    #[must_use]
    pub fn restore_identity(&self) -> Option<ClientIdentity> {
        let blob: IdentityBlob = rmp_serde::from_slice(&self.identity_blob).ok()?;
        Some(ClientIdentity {
            user_id: blob.user_id,
            username: blob.username,
            identity_signing_key: Ed25519Pair::from_seed(&blob.identity_seed),
            device_id: blob.device_id,
            device_signing_key: Ed25519Pair::from_seed(&blob.device_signing_seed),
            device_hpke_seed: blob.device_hpke_seed,
            device_hpke_public: blob.device_hpke_public,
        })
    }
}

/// Serializable identity data for localStorage persistence.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct IdentityBlob {
    user_id: Uuid,
    username: String,
    identity_seed: [u8; 32],
    device_id: Uuid,
    device_signing_seed: [u8; 32],
    device_hpke_seed: [u8; 32],
    device_hpke_public: [u8; 32],
}

/// Attempt to restore a previous session from `localStorage`.
///
/// Returns `None` if no persisted session is found or if any required data is
/// missing (first launch).
pub async fn try_restore_session() -> Option<RestoredSession> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok().flatten()?;

    let server_url = storage.get_item("messenger_server_url").ok().flatten()?;
    let user_id_str = storage.get_item("messenger_user_id").ok().flatten()?;
    let device_id_str = storage.get_item("messenger_device_id").ok().flatten()?;
    let identity_b64 = storage.get_item("messenger_identity").ok().flatten()?;
    let role_str = storage
        .get_item("messenger_user_role")
        .ok()
        .flatten()
        .unwrap_or_else(|| "user".into());

    let user_id: Uuid = user_id_str.parse().ok()?;
    let device_id: Uuid = device_id_str.parse().ok()?;
    let identity_blob = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &identity_b64,
    )
    .ok()?;

    let role = if role_str == "admin" {
        UserRole::Admin
    } else {
        UserRole::User
    };

    Some(RestoredSession {
        server_url,
        identity_blob,
        user_id,
        device_id,
        role,
    })
}

/// Persist session data to `localStorage` for future restoration.
pub fn persist_session(
    url: &str,
    user_id: Uuid,
    device_id: Uuid,
    identity_blob: &[u8],
    role: UserRole,
) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("messenger_server_url", url);
        let _ = storage.set_item("messenger_user_id", &user_id.to_string());
        let _ = storage.set_item("messenger_device_id", &device_id.to_string());
        let identity_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, identity_blob);
        let _ = storage.set_item("messenger_identity", &identity_b64);
        let role_str = match role {
            UserRole::Admin => "admin",
            UserRole::User => "user",
        };
        let _ = storage.set_item("messenger_user_role", role_str);
    }
}

/// Clear persisted session data (logout).
pub fn clear_persisted_session() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.remove_item("messenger_server_url");
        let _ = storage.remove_item("messenger_user_id");
        let _ = storage.remove_item("messenger_device_id");
        let _ = storage.remove_item("messenger_identity");
        let _ = storage.remove_item("messenger_user_role");
    }
}

//! Typed API endpoint methods.

pub mod admin;
pub mod attachments;
pub mod invites;
pub mod keypackages;
pub mod mls;
pub mod provisioning;
pub mod reactions;
pub mod server;
pub mod users;

/// Minimal response from the plaintext username lookup endpoint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsernameLookupResponse {
    pub user_id: uuid::Uuid,
}

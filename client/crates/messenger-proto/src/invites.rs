use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to create a new invite token.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateInviteRequest {
    pub role_to_grant: String,
    pub max_uses: i32,
    pub ttl_seconds: i64,
}

/// Response after creating an invite token.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateInviteResponse {
    pub id: Uuid,
    #[serde(with = "serde_bytes")]
    pub token: Vec<u8>,
    pub expires_at: i64,
}

/// Summary of an existing invite token.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InviteSummary {
    pub id: Uuid,
    pub role_to_grant: String,
    pub max_uses: i32,
    pub uses_count: i32,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

/// Response listing active invites.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListInvitesResponse {
    pub invites: Vec<InviteSummary>,
}

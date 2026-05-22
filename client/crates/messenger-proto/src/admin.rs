use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Summary of a single user for the admin users list.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AdminUserSummary {
    pub id: Uuid,
    pub role: String,
    pub status: String,
    pub created_at: i64,
    pub devices_count: i32,
}

/// Response for listing all users (admin only).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListUsersResponse {
    pub users: Vec<AdminUserSummary>,
}

/// Request to suspend a user.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SuspendUserRequest {
    pub reason: Option<String>,
}

/// Request to unsuspend a user.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnsuspendUserRequest {
    pub reason: Option<String>,
}

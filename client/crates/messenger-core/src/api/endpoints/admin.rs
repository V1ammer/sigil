use crate::api::{client::ApiClient, ApiError};
use messenger_proto::admin::*;
use uuid::Uuid;

impl ApiClient {
    /// List all users (admin only).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn list_users(&self) -> Result<ListUsersResponse, ApiError> {
        self.send::<(), _>("GET", "/v1/admin/users", None).await
    }

    /// Suspend a user (admin only).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn suspend_user(
        &self,
        user_id: Uuid,
        req: &SuspendUserRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/admin/users/{}/suspend", user_id);
        self.send("POST", &path, Some(req)).await
    }

    /// Unsuspend a user (admin only).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn unsuspend_user(
        &self,
        user_id: Uuid,
        req: &UnsuspendUserRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/admin/users/{}/unsuspend", user_id);
        self.send("POST", &path, Some(req)).await
    }
}

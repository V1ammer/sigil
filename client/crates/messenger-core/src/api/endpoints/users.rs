use crate::api::{client::ApiClient, ApiError};
use crate::api::endpoints::UsernameLookupResponse;
use messenger_proto::users::*;
use uuid::Uuid;

impl ApiClient {
    /// Look up a user by username blind index.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if user not found.
    pub async fn lookup_user(&self, blind_index: &str) -> Result<LookupResponse, ApiError> {
        let path = format!("/v1/users/lookup?blind_index={}", blind_index);
        self.send::<(), _>("GET", &path, None).await
    }

    /// Get identity credential for a user.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if user not found.
    pub async fn user_identity(&self, user_id: Uuid) -> Result<Vec<u8>, ApiError> {
        let path = format!("/v1/users/{}/identity", user_id);
        self.send::<(), Vec<u8>>("GET", &path, None).await
    }

    /// Change own username.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if username taken.
    pub async fn change_username(
        &self,
        req: &ChangeUsernameRequest,
    ) -> Result<(), ApiError> {
        self.send("PATCH", "/v1/users/me/username", Some(req)).await
    }

    /// List own devices.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn list_devices(&self) -> Result<ListDevicesResponse, ApiError> {
        self.send::<(), _>("GET", "/v1/devices/me", None).await
    }

    /// Revoke a device.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn revoke_device(
        &self,
        device_id: Uuid,
        req: &RevokeDeviceRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/devices/me/{}/revoke", device_id);
        self.send("POST", &path, Some(req)).await
    }

    /// List active devices for a user.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if user not found.
    pub async fn list_user_devices(&self, user_id: Uuid) -> Result<ListDevicesResponse, ApiError> {
        let path = format!("/v1/users/{}/devices", user_id);
        self.send::<(), _>("GET", &path, None).await
    }

    /// Look up a user by plaintext username.
    ///
    /// The server computes the blind index internally and returns the `user_id`.
    /// Used during QR provisioning (new device needs user's ID before auth).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network errors or 404.
    pub async fn lookup_user_by_username(
        &self,
        username: &str,
    ) -> Result<UsernameLookupResponse, ApiError> {
        // Usernames are restricted to `[a-z0-9_]` so no URL encoding needed.
        let path = format!("/v1/users/lookup?username={username}");
        self.send::<(), UsernameLookupResponse>("GET", &path, None).await
    }
}

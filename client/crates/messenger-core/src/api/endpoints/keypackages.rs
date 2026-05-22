use crate::api::{client::ApiClient, ApiError};
use messenger_proto::keypackages::*;
use uuid::Uuid;

impl ApiClient {
    /// Publish a batch of KeyPackages.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or validation failure.
    pub async fn publish_keypackages(
        &self,
        req: &PublishKeyPackagesRequest,
    ) -> Result<PublishKeyPackagesResponse, ApiError> {
        self.send("POST", "/v1/keypackages", Some(req)).await
    }

    /// Get remaining KeyPackage count for own device.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn keypackage_count(&self) -> Result<PoolStats, ApiError> {
        self.send::<(), _>("GET", "/v1/keypackages/me/count", None).await
    }

    /// Claim a KeyPackage for a specific device.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if pool exhausted.
    pub async fn claim_keypackage(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<ClaimKeyPackageResponse, ApiError> {
        let path = format!(
            "/v1/users/{}/devices/{}/keypackage/claim",
            user_id, device_id
        );
        self.send::<(), _>("POST", &path, None).await
    }
}

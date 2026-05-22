use crate::api::{client::ApiClient, ApiError};
use messenger_proto::provisioning::*;
use uuid::Uuid;

impl ApiClient {
    /// Create a provisioning request (new device, no auth).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn create_provisioning_request(
        &self,
        req: &CreateProvisioningRequest,
    ) -> Result<CreateProvisioningResponse, ApiError> {
        self.send("POST", "/v1/provisioning/requests", Some(req)).await
    }

    /// Get a provisioning request by ID (old device scans QR).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if request not found.
    pub async fn get_provisioning_request(
        &self,
        id: Uuid,
    ) -> Result<GetProvisioningResponse, ApiError> {
        let path = format!("/v1/provisioning/requests/{}", id);
        self.send::<(), _>("GET", &path, None).await
    }

    /// Approve a provisioning request (old device).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn approve_provisioning_request(
        &self,
        id: Uuid,
        req: &ApproveProvisioningRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/provisioning/requests/{}/approve", id);
        self.send("POST", &path, Some(req)).await
    }

    /// Fetch the bootstrap blob for a provisioning request (new device).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if not yet approved.
    pub async fn get_bootstrap(
        &self,
        id: Uuid,
    ) -> Result<GetBootstrapResponse, ApiError> {
        let path = format!("/v1/provisioning/requests/{}/bootstrap", id);
        self.send::<(), _>("GET", &path, None).await
    }
}

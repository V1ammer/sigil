use crate::api::{client::ApiClient, signing, ApiError};
use crate::ed25519::Ed25519Pair;
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
    /// Returns the new device's ID assigned by the server.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn approve_provisioning_request(
        &self,
        id: Uuid,
        req: &ApproveProvisioningRequest,
    ) -> Result<ApproveProvisioningResponse, ApiError> {
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

    /// Fetch bootstrap blob authenticated with a **temporary** Ed25519 signing key.
    ///
    /// Used by the new device during QR provisioning before it has a real
    /// identity. The request is signed with the temp Ed25519 key that was
    /// included in the provisioning request.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or API errors.
    pub async fn get_bootstrap_with_temp_key(
        &self,
        provisioning_id: Uuid,
        temp_signing_secret: &[u8; 32],
        _temp_signing_public: &[u8; 32],
    ) -> Result<GetBootstrapResponse, ApiError> {
        let path = format!("/v1/provisioning/requests/{provisioning_id}/bootstrap");
        let method = "GET";
        let body_bytes = Vec::new();

        let mut headers = vec![
            ("Content-Type".to_string(), "application/msgpack".to_string()),
            ("Accept".to_string(), "application/msgpack".to_string()),
        ];

        let ts = signing::now_secs();
        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce).map_err(|e| ApiError::Crypto(e.to_string()))?;
        let canonical = signing::build_signed_message(method, &path, ts, &nonce, &body_bytes);
        let pair = Ed25519Pair::from_secret_bytes(temp_signing_secret);
        let sig = pair.sign(&canonical);
        // Provisioning signature format: <ts>:<nonce_hex>:<sig_hex> (3 parts, no device id)
        let auth_header = format!(
            "{}:{}:{}",
            ts,
            hex::encode(&nonce),
            hex::encode(&sig)
        );
        headers.push(("X-Provisioning-Signature".to_string(), auth_header));

        let url = format!("{}{}", self.base_url, path);
        let resp = self.transport.request(method, &url, headers, body_bytes).await?;
        // parse_response is pub(crate) in client.rs
        crate::api::client::parse_response(resp)
    }
}

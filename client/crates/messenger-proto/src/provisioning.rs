use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to create a provisioning request from a new device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateProvisioningRequest {
    pub user_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub new_device_temp_public_key: Vec<u8>,
    /// Temporary Ed25519 signing public key (32 bytes) — used for polling auth.
    #[serde(with = "serde_bytes")]
    pub new_device_temp_signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
}

/// Response after creating a provisioning request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateProvisioningResponse {
    pub provisioning_id: Uuid,
    pub expires_at: i64,
}

/// Response when fetching a provisioning request (old device scans QR).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetProvisioningResponse {
    pub user_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub new_device_temp_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    pub status: String,
    pub expires_at: i64,
}

/// Request to approve a provisioning request (from old device).
///
/// Server expects flat fields (not nested), with a mandatory timestamp
/// that is part of the authorization signature message:
/// `msg = new_device_signing_pk || new_device_hpke_pk || ts_le`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApproveProvisioningRequest {
    #[serde(with = "serde_bytes")]
    pub encrypted_bootstrap_blob: Vec<u8>,
    /// New device's permanent X25519 HPKE public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub new_device_hpke_public_key: Vec<u8>,
    /// New device's permanent Ed25519 signing public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub new_device_signing_public_key: Vec<u8>,
    /// Identity key signature over `(new_device_signing_pk || new_device_hpke_pk || ts_le)`.
    #[serde(with = "serde_bytes")]
    pub device_authorization_signature: Vec<u8>,
    /// Unix timestamp (seconds) used in the authorization signature message.
    pub device_authorization_timestamp: i64,
}

/// Response after approving a provisioning request (old device gets `device_id`).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApproveProvisioningResponse {
    pub device_id: Uuid,
}

/// Response when fetching bootstrap blob (new device).
/// Field name must match the server's `BootstrapResponse.new_device_id`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBootstrapResponse {
    pub new_device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub encrypted_bootstrap_blob: Vec<u8>,
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to create a provisioning request from a new device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateProvisioningRequest {
    pub user_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub new_device_temp_public_key: Vec<u8>,
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

/// Public keys of the new device submitted during approval.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewDevicePublicKeys {
    #[serde(with = "serde_bytes")]
    pub init_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub signing_public_key: Vec<u8>,
}

/// Request to approve a provisioning request (from old device).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApproveProvisioningRequest {
    #[serde(with = "serde_bytes")]
    pub encrypted_bootstrap_blob: Vec<u8>,
    pub new_device_public_keys: NewDevicePublicKeys,
    #[serde(with = "serde_bytes")]
    pub device_authorization_signature: Vec<u8>,
}

/// Response when fetching bootstrap blob (new device).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBootstrapResponse {
    pub device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub encrypted_bootstrap_blob: Vec<u8>,
}

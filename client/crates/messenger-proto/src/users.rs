use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Response for username lookup by blind index.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LookupResponse {
    pub user_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub identity_credential: Vec<u8>,
}

/// Request to change own username.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChangeUsernameRequest {
    #[serde(with = "serde_bytes")]
    pub new_username_blind_index: Vec<u8>,
}

/// Information about a single device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceInfo {
    pub id: Uuid,
    #[serde(with = "serde_bytes")]
    pub hpke_init_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_signing_public_key: Vec<u8>,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
}

/// Response listing own devices.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceInfo>,
}

/// Request to revoke a device.
///
/// `revocation_signature = Ed25519(identity_sk, "revoke:" || device_id_bytes
/// || ":" || ts_string)` — `device_id_bytes` is the raw 16-byte UUID, and the
/// server requires `revocation_timestamp` (±300s window).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RevokeDeviceRequest {
    #[serde(with = "serde_bytes")]
    pub revocation_signature: Vec<u8>,
    pub revocation_timestamp: i64,
}

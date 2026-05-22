use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to redeem an invite token.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RedeemRequest {
    #[serde(with = "serde_bytes")]
    pub token: Vec<u8>,
    pub kind: String, // "new_user" | "new_device"
    #[serde(with = "serde_bytes")]
    pub identity_credential: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub signature_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub username_blind_index: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_init_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_authorization_signature: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub existing_identity_proof: Vec<u8>,
}

/// Response after redeeming an invite token.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RedeemResponse {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub role: String,
    #[serde(with = "serde_bytes")]
    pub server_challenge: Vec<u8>,
}

use serde::{Deserialize, Serialize};

/// Public server information (no auth required).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerInfo {
    #[serde(with = "serde_bytes")]
    pub server_identity_public_key: Vec<u8>,
    pub mls_ciphersuite: u16,
    pub schema_version: i32,
    pub username_hash_version: i32,
    pub supports_provisioning: bool,
    /// Blind index key for username HMAC (32 bytes).
    /// Added in protocol v2 — old servers return empty bytes.
    #[serde(default)]
    #[serde(with = "serde_bytes")]
    pub username_blind_index_key: Vec<u8>,
}

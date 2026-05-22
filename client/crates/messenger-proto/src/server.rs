use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Public server information (no auth required).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerInfo {
    #[serde(with = "serde_bytes")]
    pub server_identity_public_key: Vec<u8>,
    pub mls_ciphersuite: u16,
    pub schema_version: i32,
    pub username_hash_version: i32,
    pub supports_provisioning: bool,
}

use serde::{Deserialize, Serialize};

/// One `KeyPackage` upload — must match the server's `KeyPackageUpload`,
/// which validates `init_key_hash` length and `expires_at > now`. Sending a
/// bare byte blob (the old shape) is rejected with 400 ERR_BAD_REQUEST.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyPackageUpload {
    #[serde(with = "serde_bytes")]
    pub key_package: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub init_key_hash: Vec<u8>,
    pub expires_at: i64,
    pub is_last_resort: bool,
}

/// Request to publish a batch of `KeyPackage`s.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublishKeyPackagesRequest {
    pub key_packages: Vec<KeyPackageUpload>,
}

/// Response after publishing `KeyPackage`s.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublishKeyPackagesResponse {
    pub stored_count: i32,
    pub current_pool_size: i32,
}

/// Response for remaining `KeyPackage` count.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PoolStats {
    pub remaining: i32,
}

/// Response when claiming a `KeyPackage`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClaimKeyPackageResponse {
    #[serde(with = "serde_bytes")]
    pub key_package: Vec<u8>,
}

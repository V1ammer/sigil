use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use uuid::Uuid;

/// Request to publish a batch of KeyPackages.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublishKeyPackagesRequest {
    pub key_packages: Vec<ByteBuf>,
}

/// Response after publishing KeyPackages.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublishKeyPackagesResponse {
    pub stored_count: i32,
    pub current_pool_size: i32,
}

/// Response for remaining KeyPackage count.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PoolStats {
    pub remaining: i32,
}

/// Response when claiming a KeyPackage.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClaimKeyPackageResponse {
    #[serde(with = "serde_bytes")]
    pub key_package: Vec<u8>,
}

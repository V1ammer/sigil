//! MLS KeyPackage generation and management.

use openmls::prelude::{KeyPackageBuilder, Lifetime};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::{signatures::Signer as OmlsSigner, types::SignatureScheme};
use uuid::Uuid;

use crate::error::CryptoError;
use crate::identity::ClientIdentity;

use super::ciphersuite::CIPHERSUITE;
use super::credentials::build_credential;

/// A generated key package with all metadata needed for local tracking.
#[derive(Debug, Clone)]
pub struct GeneratedKeyPackage {
    /// Key package ID (local).
    pub id: Uuid,
    /// Serialized MLS KeyPackage bytes.
    pub key_package_bytes: Vec<u8>,
    /// BLAKE3 hash of the init key.
    pub init_key_hash: Vec<u8>,
    /// Secret keys (HPKE init secret + leaf secret) serialized for storage.
    pub secret_keys: Vec<u8>,
    /// Expiration timestamp (seconds since epoch).
    pub expires_at: i64,
    /// Whether this is a last-resort key package.
    pub is_last_resort: bool,
}

/// Generate a new MLS KeyPackage for the given identity.
///
/// # Errors
///
/// Returns `CryptoError::Mls` on openmls failure.
pub fn generate_keypackage(
    provider: &OpenMlsRustCrypto,
    identity: &ClientIdentity,
    lifetime_secs: u64,
    is_last_resort: bool,
) -> Result<GeneratedKeyPackage, CryptoError> {
    let credential_with_key = build_credential(identity);
    let signer = IdentitySigner(identity);

    let kp = KeyPackageBuilder::new()
        .key_package_lifetime(Lifetime::new(lifetime_secs))
        .build(CIPHERSUITE, provider, &signer, credential_with_key)
        .map_err(|e| CryptoError::Mls(format!("keypackage build: {e:?}")))?;

    let serialized = rmp_serde::to_vec_named(kp.key_package())
        .map_err(|e| CryptoError::Serialization(e.to_string()))?;

    let init_key_hash = blake3::hash(kp.key_package().hpke_init_key().as_slice()).as_bytes().to_vec();

    // Secret keys: we store a placeholder since openmls manages HPKE secrets
    // internally through the provider's storage. The caller should ensure
    // the provider's storage is persisted alongside the key package.
    let secret_keys = Vec::new();

    let now = now_secs();

    Ok(GeneratedKeyPackage {
        id: Uuid::now_v7(),
        key_package_bytes: serialized,
        init_key_hash,
        secret_keys,
        expires_at: now + i64::try_from(lifetime_secs).unwrap_or(i64::MAX),
        is_last_resort,
    })
}

/// Generate a `KeyPackageBundle` and return it alongside the metadata.
///
/// Unlike [`generate_keypackage`], this returns the full bundle (including
/// HPKE private init key) serialized for transfer to another device, plus
/// the public [`GeneratedKeyPackage`] for local tracking / upload.
///
/// # Errors
///
/// Returns `CryptoError::Mls` on openmls failure.
pub fn generate_keypackage_bundle(
    provider: &OpenMlsRustCrypto,
    identity: &ClientIdentity,
    lifetime_secs: u64,
    is_last_resort: bool,
) -> Result<(Vec<u8>, GeneratedKeyPackage), CryptoError> {
    let credential_with_key = build_credential(identity);
    let signer = IdentitySigner(identity);

    let bundle = KeyPackageBuilder::new()
        .key_package_lifetime(Lifetime::new(lifetime_secs))
        .build(CIPHERSUITE, provider, &signer, credential_with_key)
        .map_err(|e| CryptoError::Mls(format!("keypackage build: {e:?}")))?;

    // Serialize the full bundle (includes HPKE private init key)
    let bundle_bytes = rmp_serde::to_vec_named(&bundle)
        .map_err(|e| CryptoError::Serialization(e.to_string()))?;

    let serialized = rmp_serde::to_vec_named(bundle.key_package())
        .map_err(|e| CryptoError::Serialization(e.to_string()))?;

    let init_key_hash = blake3::hash(bundle.key_package().hpke_init_key().as_slice()).as_bytes().to_vec();

    let now = now_secs();

    let gen = GeneratedKeyPackage {
        id: Uuid::now_v7(),
        key_package_bytes: serialized,
        init_key_hash,
        secret_keys: Vec::new(),
        expires_at: now + i64::try_from(lifetime_secs).unwrap_or(i64::MAX),
        is_last_resort,
    };

    Ok((bundle_bytes, gen))
}

/// Wrapper to implement openmls `Signer` trait for `ClientIdentity`.
struct IdentitySigner<'a>(&'a ClientIdentity);

impl OmlsSigner for IdentitySigner<'_> {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, openmls_traits::signatures::SignerError> {
        Ok(self.0.identity_signing_key.sign(payload).to_vec())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

/// Current Unix timestamp in seconds.
fn now_secs() -> i64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .try_into()
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Device provisioning generates key packages on wasm too — a 0 here
        // would stamp not_before at the epoch and make the package look
        // long-expired. Use the real wall clock.
        (js_sys::Date::now() / 1000.0) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypackage() {
        let provider = OpenMlsRustCrypto::default();
        let identity = ClientIdentity::generate_new_user(Uuid::now_v7(), "alice".into(), Uuid::now_v7());
        let kp = generate_keypackage(&provider, &identity, 86_400, false).unwrap();
        assert_eq!(kp.init_key_hash.len(), 32);
        assert!(!kp.key_package_bytes.is_empty());
        assert!(!kp.is_last_resort);
    }

    #[test]
    fn test_last_resort_keypackage() {
        let provider = OpenMlsRustCrypto::default();
        let identity = ClientIdentity::generate_new_user(Uuid::now_v7(), "bob".into(), Uuid::now_v7());
        let kp = generate_keypackage(&provider, &identity, 86_400, true).unwrap();
        assert!(kp.is_last_resort);
    }

    #[test]
    fn test_init_key_hash_stable() {
        let provider = OpenMlsRustCrypto::default();
        let identity = ClientIdentity::generate_new_user(Uuid::now_v7(), "carol".into(), Uuid::now_v7());
        let kp1 = generate_keypackage(&provider, &identity, 86_400, false).unwrap();
        let kp2 = generate_keypackage(&provider, &identity, 86_400, false).unwrap();
        // Different key packages should have different init key hashes
        assert_ne!(kp1.init_key_hash, kp2.init_key_hash);
    }
}

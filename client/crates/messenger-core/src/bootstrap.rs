//! Bootstrap blob generation and parsing for device provisioning.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::age_wrap::{decrypt_with_x25519, encrypt_to_x25519, identity_from_raw_secret};
use crate::error::CryptoError;

/// Payload encrypted inside a bootstrap blob.
///
/// Contains identity seed (shared across all devices of a user) plus
/// pre-generated permanent device keys and MLS key package for the new device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapPayload {
    /// User ID.
    pub user_id: Uuid,
    /// Username.
    pub username: String,
    /// Identity signing seed (32 bytes) — shared across all devices.
    pub identity_signing_seed: [u8; 32],
    /// Pre-generated Ed25519 signing secret seed for the new device (32 bytes).
    pub device_signing_seed: [u8; 32],
    /// Pre-generated X25519 HPKE secret seed for the new device (32 bytes).
    pub device_hpke_seed: [u8; 32],
    /// Serialized MLS `KeyPackageBundle` (includes HPKE init private key).
    /// The new device deserializes and stores this in its provider to
    /// decrypt Welcome messages.
    #[serde(with = "serde_bytes")]
    pub key_package_bundle: Vec<u8>,
}

/// Build an encrypted bootstrap blob for a new device.
///
/// # Errors
///
/// Returns `CryptoError` on serialization or encryption failure.
pub fn build_bootstrap(
    payload: &BootstrapPayload,
    recipient: &age::x25519::Recipient,
) -> Result<Vec<u8>, CryptoError> {
    let serialized = rmp_serde::to_vec_named(payload)?;
    encrypt_to_x25519(recipient, &serialized)
}

/// Open a bootstrap blob using the new device's identity.
///
/// # Errors
///
/// Returns `CryptoError` on decryption or deserialization failure.
pub fn open_bootstrap(
    blob: &[u8],
    identity: &age::x25519::Identity,
) -> Result<BootstrapPayload, CryptoError> {
    let plaintext = decrypt_with_x25519(identity, blob)?;
    Ok(rmp_serde::from_slice(&plaintext)?)
}

/// Open a bootstrap blob using raw X25519 secret key bytes.
///
/// Constructs an `age::x25519::Identity` internally from the raw secret
/// and delegates to [`open_bootstrap`].
///
/// # Errors
///
/// Returns `CryptoError` on decryption or deserialization failure.
pub fn open_bootstrap_raw_secret(
    blob: &[u8],
    secret_seed: &[u8; 32],
) -> Result<BootstrapPayload, CryptoError> {
    let identity = identity_from_raw_secret(secret_seed);
    open_bootstrap(blob, &identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_roundtrip() {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public();

        let payload = BootstrapPayload {
            user_id: Uuid::now_v7(),
            username: "alice".into(),
            identity_signing_seed: [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
            ],
            device_signing_seed: [33u8; 32],
            device_hpke_seed: [66u8; 32],
            key_package_bundle: vec![1, 2, 3],
        };

        let blob = build_bootstrap(&payload, &recipient).unwrap();
        let back = open_bootstrap(&blob, &identity).unwrap();
        assert_eq!(back.user_id, payload.user_id);
        assert_eq!(back.username, payload.username);
        assert_eq!(back.identity_signing_seed, payload.identity_signing_seed);
        assert_eq!(back.device_signing_seed, payload.device_signing_seed);
        assert_eq!(back.device_hpke_seed, payload.device_hpke_seed);
    }

    #[test]
    fn test_open_bootstrap_raw_secret() {
        let payload = BootstrapPayload {
            user_id: Uuid::now_v7(),
            username: "alice".into(),
            identity_signing_seed: [42u8; 32],
            device_signing_seed: [77u8; 32],
            device_hpke_seed: [88u8; 32],
            key_package_bundle: vec![4, 5, 6],
        };

        let raw_secret: [u8; 32] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let known_identity = identity_from_raw_secret(&raw_secret);
        let known_recipient = known_identity.to_public();

        let blob = build_bootstrap(&payload, &known_recipient).unwrap();
        let back = open_bootstrap_raw_secret(&blob, &raw_secret).unwrap();
        assert_eq!(back.user_id, payload.user_id);
        assert_eq!(back.username, payload.username);
        assert_eq!(back.identity_signing_seed, payload.identity_signing_seed);
        assert_eq!(back.device_signing_seed, payload.device_signing_seed);
        assert_eq!(back.device_hpke_seed, payload.device_hpke_seed);
    }
}

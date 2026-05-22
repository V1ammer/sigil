//! Bootstrap blob generation and parsing for device provisioning.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::age_wrap::{decrypt_with_x25519, encrypt_to_x25519};
use crate::error::CryptoError;

/// Payload encrypted inside a bootstrap blob.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapPayload {
    /// User ID.
    pub user_id: Uuid,
    /// Username.
    pub username: String,
    /// Identity signing seed (32 bytes).
    pub identity_signing_seed: [u8; 32],
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
        };

        let blob = build_bootstrap(&payload, &recipient).unwrap();
        let back = open_bootstrap(&blob, &identity).unwrap();
        assert_eq!(back.user_id, payload.user_id);
        assert_eq!(back.username, payload.username);
        assert_eq!(back.identity_signing_seed, payload.identity_signing_seed);
    }
}

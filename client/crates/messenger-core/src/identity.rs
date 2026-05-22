//! Client identity: user-level and device-level keys.

use rand::RngCore;
use uuid::Uuid;

use crate::ed25519::Ed25519Pair;

/// Client identity holds all key material for a user on a specific device.
#[derive(Debug, Clone)]
pub struct ClientIdentity {
    /// User ID.
    pub user_id: Uuid,
    /// Plaintext username (never sent except during registration).
    pub username: String,
    /// Main identity signing key — replicated across devices via bootstrap.
    pub identity_signing_key: Ed25519Pair,
    /// Device ID.
    pub device_id: Uuid,
    /// Per-device signing key for request authentication.
    pub device_signing_key: Ed25519Pair,
    /// X25519 secret for MLS init key.
    pub device_hpke_seed: [u8; 32],
    /// X25519 public key for MLS init key.
    pub device_hpke_public: [u8; 32],
}

impl ClientIdentity {
    /// Generate a brand-new user with a fresh identity and first device.
    #[must_use]
    pub fn generate_new_user(user_id: Uuid, username: String, device_id: Uuid) -> Self {
        let identity_signing_key = Ed25519Pair::generate();
        let device_signing_key = Ed25519Pair::generate();
        let mut hpke_seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut hpke_seed);
        let hpke_secret = x25519_dalek::StaticSecret::from(hpke_seed);
        let hpke_public = x25519_dalek::PublicKey::from(&hpke_secret).to_bytes();
        Self {
            user_id,
            username,
            identity_signing_key,
            device_id,
            device_signing_key,
            device_hpke_seed: hpke_seed,
            device_hpke_public: hpke_public,
        }
    }

    /// Generate a new device for an existing user (provisioning).
    ///
    /// The identity signing key is recovered from `identity_seed`;
    /// everything else is generated fresh.
    #[must_use]
    pub fn generate_new_device(
        user_id: Uuid,
        username: String,
        device_id: Uuid,
        identity_seed: [u8; 32],
    ) -> Self {
        let device_signing_key = Ed25519Pair::generate();
        let mut hpke_seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut hpke_seed);
        let hpke_secret = x25519_dalek::StaticSecret::from(hpke_seed);
        let hpke_public = x25519_dalek::PublicKey::from(&hpke_secret).to_bytes();
        Self {
            user_id,
            username,
            identity_signing_key: Ed25519Pair::from_seed(&identity_seed),
            device_id,
            device_signing_key,
            device_hpke_seed: hpke_seed,
            device_hpke_public: hpke_public,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_new_user() {
        let id = ClientIdentity::generate_new_user(Uuid::now_v7(), "alice".into(), Uuid::now_v7());
        assert_eq!(id.username, "alice");
        // Keys should be non-zero (with overwhelming probability)
        assert_ne!(id.identity_signing_key.public_bytes(), [0u8; 32]);
        assert_ne!(id.device_signing_key.public_bytes(), [0u8; 32]);
    }

    #[test]
    fn test_generate_new_device_preserves_identity() {
        let user_id = Uuid::now_v7();
        let device1 = ClientIdentity::generate_new_user(user_id, "bob".into(), Uuid::now_v7());
        let seed = device1.identity_signing_key.secret_bytes();
        let device2 = ClientIdentity::generate_new_device(
            user_id,
            "bob".into(),
            Uuid::now_v7(),
            seed,
        );
        assert_eq!(
            device1.identity_signing_key.public_bytes(),
            device2.identity_signing_key.public_bytes()
        );
        // Device keys should differ
        assert_ne!(
            device1.device_signing_key.public_bytes(),
            device2.device_signing_key.public_bytes()
        );
    }
}

//! Ed25519 sign/verify wrappers.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::error::CryptoError;

/// An Ed25519 key pair.
#[derive(Debug, Clone)]
pub struct Ed25519Pair {
    sk: SigningKey,
    pk: VerifyingKey,
}

impl Ed25519Pair {
    /// Generate a fresh random key pair.
    #[must_use]
    pub fn generate() -> Self {
        let sk = SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key();
        Self { sk, pk }
    }

    /// Restore from a 32-byte seed.
    ///
    /// # Errors
    ///
    /// Returns `InvalidKeyLength` if `seed` is not 32 bytes (handled by array type).
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let sk = SigningKey::from_bytes(seed);
        let pk = sk.verifying_key();
        Self { sk, pk }
    }

    /// Alias for `from_seed`.
    #[must_use]
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self::from_seed(bytes)
    }

    /// Return the 32-byte secret key.
    #[must_use]
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.sk.to_bytes()
    }

    /// Return the 32-byte public key.
    #[must_use]
    pub fn public_bytes(&self) -> [u8; 32] {
        self.pk.to_bytes()
    }

    /// Sign a message, returning a 64-byte signature.
    #[must_use]
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.sk.sign(msg).to_bytes()
    }
}

/// Verify an Ed25519 signature.
///
/// # Errors
///
/// - `InvalidKey` if `public_key` is not a valid verifying key.
/// - `VerificationFailed` if the signature does not verify.
pub fn verify(public_key: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> Result<(), CryptoError> {
    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| CryptoError::InvalidKey)?;
    let signature = Signature::from_bytes(sig);
    vk.verify(msg, &signature)
        .map_err(|_| CryptoError::VerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let pair = Ed25519Pair::generate();
        let msg = b"hello world";
        let sig = pair.sign(msg);
        assert!(verify(&pair.public_bytes(), msg, &sig).is_ok());
    }

    #[test]
    fn test_verify_bad_signature_fails() {
        let pair = Ed25519Pair::generate();
        let msg = b"hello world";
        let mut sig = pair.sign(msg);
        sig[0] ^= 0xFF;
        assert!(verify(&pair.public_bytes(), msg, &sig).is_err());
    }

    #[test]
    fn test_from_seed_deterministic() {
        let seed = [42u8; 32];
        let a = Ed25519Pair::from_seed(&seed);
        let b = Ed25519Pair::from_seed(&seed);
        assert_eq!(a.public_bytes(), b.public_bytes());
        assert_eq!(a.secret_bytes(), b.secret_bytes());
    }
}

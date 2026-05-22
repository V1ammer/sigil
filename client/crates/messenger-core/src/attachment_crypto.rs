//! Symmetric encryption of attachment payloads.
//!
//! Uses AES-256-GCM with a random 12-byte nonce prepended to ciphertext.
//! The decryption key is 32 random bytes (AES-256 key).

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

use crate::error::CryptoError;

/// Generate a random 32-byte AES-256 key.
#[must_use]
pub fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

/// Encrypt plaintext with AES-256-GCM.
///
/// Returns `iv (12 bytes) || ciphertext || tag (16 bytes)`.
///
/// # Errors
///
/// Returns `CryptoError` if encryption fails.
pub fn encrypt_attachment(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Crypto(format!("aes key init: {e}")))?;

    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::Crypto(format!("aes encrypt: {e}")))?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt ciphertext produced by [`encrypt_attachment`].
///
/// Input format: `iv (12 bytes) || ciphertext || tag (16 bytes)`.
///
/// # Errors
///
/// Returns `CryptoError` if decryption fails (wrong key or corrupted data).
pub fn decrypt_attachment(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < 12 {
        return Err(CryptoError::Crypto("attachment data too short".into()));
    }
    let (iv_bytes, ct) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Crypto(format!("aes key init: {e}")))?;
    let nonce = Nonce::from_slice(iv_bytes);

    cipher
        .decrypt(nonce, ct)
        .map_err(|e| CryptoError::Crypto(format!("aes decrypt: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_encryption_key();
        let plaintext = b"hello voice message!";
        let encrypted = encrypt_attachment(&key, plaintext).unwrap();
        let decrypted = decrypt_attachment(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
        // Should have iv (12) + ciphertext + tag (16)
        assert!(encrypted.len() > 12);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_encryption_key();
        let key2 = generate_encryption_key();
        let plaintext = b"secret data";
        let encrypted = encrypt_attachment(&key1, plaintext).unwrap();
        let result = decrypt_attachment(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let key = generate_encryption_key();
        let encrypted = encrypt_attachment(&key, b"").unwrap();
        let decrypted = decrypt_attachment(&key, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }
}

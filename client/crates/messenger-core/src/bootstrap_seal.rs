//! WASM-compatible sealed box for device-provisioning bootstrap blobs.
//!
//! The previous design encrypted the bootstrap with the `age` crate, which
//! only builds for native targets — so QR device provisioning (and QR sign-in)
//! could never complete in the browser. This module replaces it with a small
//! anonymous sealed box built from primitives that compile to wasm32 and are
//! already used elsewhere in the client (x25519-dalek + SHA-256 + AES-256-GCM
//! via [`crate::attachment_crypto`]), so both the app and the browser can
//! decrypt.
//!
//! Wire format: `ephemeral_x25519_pub (32) || aes_gcm(iv || ct || tag)`.
//!
//! Scheme (anonymous ECIES / NaCl `box_seal` style):
//! - sender: ephemeral X25519 keypair `e`; `shared = ECDH(e_sk, recipient_pk)`;
//!   `key = SHA256(shared || e_pk || recipient_pk)`; AES-256-GCM the plaintext.
//! - recipient: `shared = ECDH(recipient_sk, e_pk)`; same KDF; decrypt.

use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::attachment_crypto::{decrypt_attachment, encrypt_attachment};
use crate::error::CryptoError;

/// Derive the symmetric key from the ECDH shared secret and both public keys.
fn derive_key(shared: &[u8; 32], eph_pub: &[u8; 32], recipient_pub: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"messenger-bootstrap-seal-v1");
    h.update(shared);
    h.update(eph_pub);
    h.update(recipient_pub);
    let digest = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// Encrypt `plaintext` so only the holder of the secret for `recipient_pub`
/// (a raw 32-byte X25519 public key) can read it.
///
/// # Errors
///
/// Returns `CryptoError` if AES-GCM encryption fails.
pub fn seal(recipient_pub: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut eph_seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut eph_seed);
    let eph_secret = x25519_dalek::StaticSecret::from(eph_seed);
    let eph_pub = x25519_dalek::PublicKey::from(&eph_secret).to_bytes();

    let recipient = x25519_dalek::PublicKey::from(*recipient_pub);
    let shared = eph_secret.diffie_hellman(&recipient).to_bytes();

    let key = derive_key(&shared, &eph_pub, recipient_pub);
    let ct = encrypt_attachment(&key, plaintext)?;

    let mut out = Vec::with_capacity(32 + ct.len());
    out.extend_from_slice(&eph_pub);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt a blob produced by [`seal`] using the recipient's 32-byte X25519
/// secret seed.
///
/// # Errors
///
/// Returns `CryptoError` if the blob is malformed or decryption fails.
pub fn open(recipient_secret: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < 32 {
        return Err(CryptoError::InvalidState("bootstrap blob too short".into()));
    }
    let mut eph_pub = [0u8; 32];
    eph_pub.copy_from_slice(&blob[..32]);
    let ct = &blob[32..];

    let secret = x25519_dalek::StaticSecret::from(*recipient_secret);
    let recipient_pub = x25519_dalek::PublicKey::from(&secret).to_bytes();
    let shared = secret
        .diffie_hellman(&x25519_dalek::PublicKey::from(eph_pub))
        .to_bytes();

    let key = derive_key(&shared, &eph_pub, &recipient_pub);
    decrypt_attachment(&key, ct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        let secret = x25519_dalek::StaticSecret::from(seed);
        let public = x25519_dalek::PublicKey::from(&secret).to_bytes();

        let msg = b"bootstrap payload bytes";
        let blob = seal(&public, msg).unwrap();
        let out = open(&seed, &blob).unwrap();
        assert_eq!(out, msg);
    }

    #[test]
    fn wrong_secret_fails() {
        let mut s1 = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut s1);
        let public = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(s1)).to_bytes();
        let mut s2 = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut s2);

        let blob = seal(&public, b"secret").unwrap();
        assert!(open(&s2, &blob).is_err());
    }
}

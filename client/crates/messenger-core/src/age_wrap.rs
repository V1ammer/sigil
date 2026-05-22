//! AGE wrappers for bootstrap blob encryption.

use std::io::{Read, Write};

use crate::error::CryptoError;

/// Encrypt a plaintext to an X25519 recipient.
///
/// # Errors
///
/// Returns `CryptoError::Age` on encryption failure.
pub fn encrypt_to_x25519(
    recipient: &age::x25519::Recipient,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let encryptor = age::Encryptor::with_recipients(vec![Box::new(recipient.clone())])
        .ok_or_else(|| CryptoError::Age("no recipients".into()))?;
    let mut out = Vec::new();
    let mut writer = encryptor.wrap_output(&mut out)?;
    writer.write_all(plaintext)?;
    writer.finish()?;
    Ok(out)
}

/// Decrypt a ciphertext using an X25519 identity.
///
/// # Errors
///
/// Returns `CryptoError::Age` on decryption failure.
pub fn decrypt_with_x25519(
    identity: &age::x25519::Identity,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let decryptor = match age::Decryptor::new(ciphertext)? {
        age::Decryptor::Recipients(d) => d,
        _ => return Err(CryptoError::AgeUnexpectedKind),
    };
    let mut out = Vec::new();
    let mut reader = decryptor.decrypt(std::iter::once(identity as &dyn age::Identity))?;
    reader.read_to_end(&mut out)?;
    Ok(out)
}

/// Construct an `age::x25519::Identity` from raw 32-byte secret key material.
///
/// The secret is encoded into the AGE bech32 secret-key format and then parsed
/// back, producing a valid `Identity` that can be used with `decrypt_with_x25519`.
///
/// # Panics
///
/// Panics if the internal bech32 encoding/decoding fails (should not happen
/// with valid 32-byte input).
pub fn identity_from_raw_secret(secret: &[u8; 32]) -> age::x25519::Identity {
    use bech32::{ToBase32, Variant};

    let base32 = secret.to_base32();
    let encoded =
        bech32::encode("age-secret-key-", base32, Variant::Bech32).expect("valid bech32 HRP");
    encoded
        .to_uppercase()
        .parse::<age::x25519::Identity>()
        .expect("valid age secret key")
}

/// Construct an `age::x25519::Recipient` from raw 32-byte X25519 public key
/// material.
///
/// The public key is encoded into the AGE bech32 recipient format and then
/// parsed back, producing a valid `Recipient` that can be used with
/// `encrypt_to_x25519`.
///
/// # Panics
///
/// Panics if the internal bech32 encoding/decoding fails (should not happen
/// with valid 32-byte input).
pub fn recipient_from_raw_public(pubkey: &[u8; 32]) -> age::x25519::Recipient {
    use bech32::{ToBase32, Variant};

    let base32 = pubkey.to_base32();
    let encoded =
        bech32::encode("age", base32, Variant::Bech32).expect("valid bech32 HRP");
    encoded
        .parse::<age::x25519::Recipient>()
        .expect("valid age recipient")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_age_round_trip() {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public();

        let plaintext = b"hello bootstrap world";
        let encrypted = encrypt_to_x25519(&recipient, plaintext).unwrap();
        let decrypted = decrypt_with_x25519(&identity, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_age_empty_plaintext() {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public();

        let encrypted = encrypt_to_x25519(&recipient, b"").unwrap();
        let decrypted = decrypt_with_x25519(&identity, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_identity_from_raw_secret_roundtrip() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut secret = [0u8; 32];
        rng.fill(&mut secret);

        let identity = identity_from_raw_secret(&secret);
        let recipient = identity.to_public();

        let plaintext = b"test from raw secret";
        let encrypted = encrypt_to_x25519(&recipient, plaintext).unwrap();
        let decrypted = decrypt_with_x25519(&identity, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_recipient_from_raw_public() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut pubkey = [0u8; 32];
        rng.fill(&mut pubkey);
        // Just verify that recipient_from_raw_public doesn't panic
        // with valid 32-byte input (creates a valid age Recipient).
        let _recipient = recipient_from_raw_public(&pubkey);
    }
}

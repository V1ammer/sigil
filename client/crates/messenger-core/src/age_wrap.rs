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
}

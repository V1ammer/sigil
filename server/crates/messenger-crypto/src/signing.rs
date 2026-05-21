#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::error::CryptoError;

/// Верифицирует Ed25519 подпись.
///
/// # Errors
///
/// Возвращает `CryptoError` если:
/// - `public_key` не 32 байта.
/// - `signature` не 64 байта.
/// - Ключ или подпись невалидны (десериализация или верификация).
pub fn verify_ed25519(
    public_key: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<(), CryptoError> {
    if public_key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength(public_key.len()));
    }
    if signature.len() != 64 {
        return Err(CryptoError::InvalidSignatureLength(signature.len()));
    }

    let vk = VerifyingKey::from_bytes(
        public_key.try_into().map_err(|_| CryptoError::InvalidKey)?,
    )
    .map_err(|_| CryptoError::InvalidKey)?;

    let sig = Signature::from_bytes(
        signature.try_into().map_err(|_| CryptoError::InvalidSignatureLength(signature.len()))?,
    );

    vk.verify(message, &sig).map_err(|_| CryptoError::VerificationFailed)?;

    Ok(())
}

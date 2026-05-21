#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

/// Ошибки криптографических операций.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// Некорректная длина ключа (должен быть 32 байта).
    #[error("invalid key length: expected 32 bytes, got {0}")]
    InvalidKeyLength(usize),

    /// Некорректная длина подписи (должна быть 64 байта).
    #[error("invalid signature length: expected 64 bytes, got {0}")]
    InvalidSignatureLength(usize),

    /// Ключ не удалось десериализовать.
    #[error("invalid public key")]
    InvalidKey,

    /// Подпись не прошла верификацию.
    #[error("signature verification failed")]
    VerificationFailed,

    /// Ошибка декодирования hex.
    #[error("hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),
}

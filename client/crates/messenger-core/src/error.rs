//! Cryptographic error types.

/// Errors that can occur during cryptographic operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// Invalid key length.
    #[error("invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },

    /// Invalid signature length.
    #[error("invalid signature length: expected {expected}, got {got}")]
    InvalidSignatureLength { expected: usize, got: usize },

    /// Key could not be deserialized.
    #[error("invalid public key")]
    InvalidKey,

    /// Signature verification failed.
    #[error("signature verification failed")]
    VerificationFailed,

    /// Hex decode error.
    #[error("hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// MLS operation failed.
    #[cfg(feature = "native")]
    #[error("MLS error: {0}")]
    Mls(String),

    /// AGE encryption/decryption error.
    #[cfg(feature = "native")]
    #[error("AGE error: {0}")]
    Age(String),

    /// Unexpected AGE decryptor kind.
    #[cfg(feature = "native")]
    #[error("unexpected AGE decryptor kind")]
    AgeUnexpectedKind,

    /// Storage error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Invalid state — group not found, etc.
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// General encoding error.
    #[error("encoding error: {0}")]
    Encoding(String),

    /// Symmetric crypto error.
    #[error("crypto error: {0}")]
    Crypto(String),
}

impl From<messenger_storage::error::StorageError> for CryptoError {
    fn from(e: messenger_storage::error::StorageError) -> Self {
        Self::Storage(e.to_string())
    }
}

#[cfg(feature = "native")]
impl From<tls_codec::Error> for CryptoError {
    fn from(e: tls_codec::Error) -> Self {
        Self::Serialization(format!("tls_codec: {e}"))
    }
}

impl From<rmp_serde::encode::Error> for CryptoError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        Self::Serialization(format!("msgpack encode: {e}"))
    }
}

impl From<rmp_serde::decode::Error> for CryptoError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        Self::Serialization(format!("msgpack decode: {e}"))
    }
}

#[cfg(feature = "native")]
impl From<std::io::Error> for CryptoError {
    fn from(e: std::io::Error) -> Self {
        Self::Age(e.to_string())
    }
}

#[cfg(feature = "native")]
impl From<age::EncryptError> for CryptoError {
    fn from(e: age::EncryptError) -> Self {
        Self::Age(e.to_string())
    }
}

#[cfg(feature = "native")]
impl From<age::DecryptError> for CryptoError {
    fn from(e: age::DecryptError) -> Self {
        Self::Age(e.to_string())
    }
}

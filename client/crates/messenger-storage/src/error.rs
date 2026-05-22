//! Storage error types.

/// Errors that can occur during storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Requested item was not found.
    #[error("not found")]
    NotFound,
    /// Access to the storage was denied (e.g. wrong decryption key).
    #[error("access denied")]
    AccessDenied,
    /// Cryptographic operation failed.
    #[error("crypto error: {0}")]
    Crypto(String),
    /// Database operation failed.
    #[error("db error: {0}")]
    Database(String),
    /// I/O operation failed.
    #[error("io error: {0}")]
    Io(String),
    /// Platform-specific operation failed.
    #[error("platform error: {0}")]
    Platform(String),
}

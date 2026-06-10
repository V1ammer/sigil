use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranscriptionError {
    #[error("model already exists")]
    AlreadyExists,
    #[error("model not found: {0}")]
    NotFound(String),
    #[error("invalid model path")]
    InvalidPath,
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("network: {0}")]
    Network(String),
    #[error("io: {0}")]
    Io(String),
    #[error("audio decode: {0}")]
    AudioDecode(String),
    #[error("model load: {0}")]
    ModelLoad(String),
    #[error("transcribe: {0}")]
    Internal(String),
}

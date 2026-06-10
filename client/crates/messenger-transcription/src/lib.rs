//! On-device voice-message transcription using whisper.cpp via whisper-rs.
//!
//! The frontend lives in WASM and can't link `whisper-rs` directly. All public
//! entry points here run only on native targets; the Tauri main binary exposes
//! them as `invoke` commands that the WASM UI calls. See `src-tauri/src/transcription.rs`.

pub mod audio;
pub mod downloader;
pub mod error;
pub mod models;
pub mod transcriber;

pub use error::TranscriptionError;
pub use models::{available_models, ModelQuality, WhisperModel};
pub use transcriber::{TranscriptSegment, Transcriber, TranscriptionResult};

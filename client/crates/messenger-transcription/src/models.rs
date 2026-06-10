//! Static catalogue of Whisper models available for download.
//!
//! URLs come from whisper.cpp's `download-ggml-model.sh`. Size/RAM figures
//! come from whisper.cpp's README.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelQuality {
    Fast,
    Balanced,
    Best,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhisperModel {
    pub id: &'static str,
    pub display_name: &'static str,
    pub size_mb: u32,
    pub ram_mb: u32,
    pub quality: ModelQuality,
    pub url: &'static str,
    pub sha256: Option<&'static str>,
    pub multilingual: bool,
}

#[must_use]
pub fn available_models() -> &'static [WhisperModel] {
    MODELS
}

#[must_use]
pub fn find(id: &str) -> Option<&'static WhisperModel> {
    MODELS.iter().find(|m| m.id == id)
}

static MODELS: &[WhisperModel] = &[
    WhisperModel {
        id: "tiny",
        display_name: "Tiny",
        size_mb: 75,
        ram_mb: 273,
        quality: ModelQuality::Fast,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        sha256: None,
        multilingual: true,
    },
    WhisperModel {
        id: "tiny.en",
        display_name: "Tiny (English)",
        size_mb: 75,
        ram_mb: 273,
        quality: ModelQuality::Fast,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        sha256: None,
        multilingual: false,
    },
    WhisperModel {
        id: "base",
        display_name: "Base",
        size_mb: 142,
        ram_mb: 388,
        quality: ModelQuality::Fast,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        sha256: None,
        multilingual: true,
    },
    WhisperModel {
        id: "base.en",
        display_name: "Base (English)",
        size_mb: 142,
        ram_mb: 388,
        quality: ModelQuality::Fast,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        sha256: None,
        multilingual: false,
    },
    WhisperModel {
        id: "small",
        display_name: "Small",
        size_mb: 466,
        ram_mb: 852,
        quality: ModelQuality::Balanced,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        sha256: None,
        multilingual: true,
    },
    WhisperModel {
        id: "small.en",
        display_name: "Small (English)",
        size_mb: 466,
        ram_mb: 852,
        quality: ModelQuality::Balanced,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        sha256: None,
        multilingual: false,
    },
    WhisperModel {
        id: "medium",
        display_name: "Medium",
        size_mb: 1500,
        ram_mb: 1500,
        quality: ModelQuality::Best,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        sha256: None,
        multilingual: true,
    },
    WhisperModel {
        id: "medium.en",
        display_name: "Medium (English)",
        size_mb: 1500,
        ram_mb: 1500,
        quality: ModelQuality::Best,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        sha256: None,
        multilingual: false,
    },
    WhisperModel {
        id: "large-v3",
        display_name: "Large v3",
        size_mb: 2900,
        ram_mb: 2900,
        quality: ModelQuality::Best,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        sha256: None,
        multilingual: true,
    },
];

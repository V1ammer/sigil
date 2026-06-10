//! Async model downloader with progress callback and atomic rename on success.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::error::TranscriptionError;
use crate::models::WhisperModel;

#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// Stream a model from `model.url` into `dest_path`. Progress is delivered
/// via `on_progress` (use a channel sender to thread it through Tauri events).
pub async fn download_model<F>(
    model: &WhisperModel,
    dest_path: &Path,
    mut on_progress: F,
) -> Result<(), TranscriptionError>
where
    F: FnMut(DownloadProgress) + Send,
{
    if dest_path.exists() {
        return Err(TranscriptionError::AlreadyExists);
    }
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| TranscriptionError::Io(e.to_string()))?;
    }

    let tmp_path = dest_path.with_extension("download");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 60))
        .build()
        .map_err(|e| TranscriptionError::Network(e.to_string()))?;
    let resp = client
        .get(model.url)
        .send()
        .await
        .map_err(|e| TranscriptionError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(TranscriptionError::Network(format!(
            "HTTP {}",
            resp.status()
        )));
    }
    let total_bytes = resp.content_length().unwrap_or(0);

    let mut file = File::create(&tmp_path)
        .await
        .map_err(|e| TranscriptionError::Io(e.to_string()))?;
    let mut stream = resp.bytes_stream();
    let mut downloaded = 0_u64;
    let mut hasher = Sha256::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| TranscriptionError::Network(e.to_string()))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| TranscriptionError::Io(e.to_string()))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        on_progress(DownloadProgress {
            downloaded_bytes: downloaded,
            total_bytes,
        });
    }

    file.flush()
        .await
        .map_err(|e| TranscriptionError::Io(e.to_string()))?;
    drop(file);

    if let Some(expected_hex) = model.sha256 {
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected_hex {
            tokio::fs::remove_file(&tmp_path).await.ok();
            return Err(TranscriptionError::ChecksumMismatch);
        }
    }

    tokio::fs::rename(&tmp_path, dest_path)
        .await
        .map_err(|e| TranscriptionError::Io(e.to_string()))?;
    Ok(())
}

pub async fn delete_model(model_path: &Path) -> Result<(), TranscriptionError> {
    if model_path.exists() {
        tokio::fs::remove_file(model_path)
            .await
            .map_err(|e| TranscriptionError::Io(e.to_string()))?;
    }
    Ok(())
}

/// Where models are stored on disk. Uses the platform data dir, falling back
/// to the system temp dir if the data dir can't be resolved.
#[must_use]
pub fn model_storage_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("messenger")
        .join("whisper_models")
}

#[must_use]
pub fn model_path(id: &str) -> PathBuf {
    model_storage_dir().join(format!("ggml-{id}.bin"))
}

/// Returns the list of model ids whose `.bin` file is present on disk.
#[must_use]
pub fn list_downloaded() -> Vec<String> {
    let dir = model_storage_dir();
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(rest) = name.strip_prefix("ggml-") {
                    if let Some(id) = rest.strip_suffix(".bin") {
                        out.push(id.to_string());
                    }
                }
            }
        }
    }
    out
}

//! Tauri commands that bridge the WASM UI to `messenger-transcription`.
//!
//! Audio decoding is the frontend's job (WebView `AudioContext` already ships
//! an Opus decoder); these commands receive PCM `f32` samples plus the source
//! sample rate, then resample and run whisper.cpp on a blocking worker.

use std::path::PathBuf;
use std::sync::Mutex;

use messenger_transcription::downloader::{
    delete_model, download_model, list_downloaded, model_path, DownloadProgress,
};
use messenger_transcription::models::{available_models, find, WhisperModel};
use messenger_transcription::transcriber::{Transcriber, TranscriptionResult};
use messenger_transcription::{audio, TranscriptionError};
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Clone, serde::Serialize)]
pub struct ProgressEvent {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// Latest progress of the in-flight model download, so the WASM UI can poll it
/// (the WebView can't subscribe to native Tauri events without fragile callback
/// plumbing). `None` means no download is running.
static DOWNLOAD_PROGRESS: Mutex<Option<ProgressEvent>> = Mutex::new(None);

/// Current download progress, polled by the settings UI.
#[tauri::command]
pub fn transcription_download_progress() -> Option<ProgressEvent> {
    DOWNLOAD_PROGRESS.lock().ok().and_then(|g| g.clone())
}

#[tauri::command]
pub fn transcription_list_models() -> Vec<WhisperModel> {
    available_models().to_vec()
}

#[tauri::command]
pub fn transcription_list_downloaded() -> Vec<String> {
    list_downloaded()
}

#[tauri::command]
pub async fn transcription_download_model<R: Runtime>(
    app: AppHandle<R>,
    model_id: String,
) -> Result<String, String> {
    let model = find(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    let dest = model_path(&model_id);
    let id_for_emit = model_id.clone();
    let app_emit = app.clone();
    *DOWNLOAD_PROGRESS.lock().unwrap() = Some(ProgressEvent {
        model_id: model_id.clone(),
        downloaded_bytes: 0,
        total_bytes: 0,
    });
    let result = download_model(model, &dest, move |p: DownloadProgress| {
        let ev = ProgressEvent {
            model_id: id_for_emit.clone(),
            downloaded_bytes: p.downloaded_bytes,
            total_bytes: p.total_bytes,
        };
        // Stored for polling and emitted for any future event listener.
        *DOWNLOAD_PROGRESS.lock().unwrap() = Some(ev.clone());
        let _ = app_emit.emit("transcription://progress", ev);
    })
    .await;
    *DOWNLOAD_PROGRESS.lock().unwrap() = None;
    match result {
        Ok(()) => Ok(dest.to_string_lossy().into_owned()),
        Err(TranscriptionError::AlreadyExists) => Ok(dest.to_string_lossy().into_owned()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn transcription_delete_model(model_id: String) -> Result<(), String> {
    let path = model_path(&model_id);
    delete_model(&path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn transcription_get_active<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    let p = active_model_marker(&app)?;
    std::fs::read_to_string(&p).ok().map(|s| s.trim().to_string())
}

#[tauri::command]
pub fn transcription_set_active<R: Runtime>(
    app: AppHandle<R>,
    model_id: String,
) -> Result<(), String> {
    let p = active_model_marker(&app).ok_or("no app data dir")?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, &model_id).map_err(|e| e.to_string())
}

/// Run transcription on PCM `f32` samples already decoded by the WebView.
/// `samples_bytes` is the raw `f32` little-endian byte representation of the
/// channel — keeps the invoke payload compact (one `Uint8Array` instead of a
/// JSON number array which would balloon to multi-MB strings for a minute of
/// audio).
#[tauri::command]
pub async fn transcription_transcribe<R: Runtime>(
    app: AppHandle<R>,
    samples_bytes: Vec<u8>,
    sample_rate: u32,
    language: Option<String>,
) -> Result<TranscriptionResult, String> {
    let active = transcription_get_active(app.clone())
        .ok_or_else(|| "no active model selected".to_string())?;
    let model_path = model_path(&active);
    if !model_path.exists() {
        return Err(format!("active model not downloaded: {active}"));
    }
    if samples_bytes.len() % 4 != 0 {
        return Err("samples_bytes length is not a multiple of 4 (f32 bytes)".into());
    }
    let samples: Vec<f32> = samples_bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    // Whisper is CPU-bound; keep it off the async runtime thread.
    let resampled = audio::to_16k_mono(samples, sample_rate).map_err(|e| e.to_string())?;
    let result = tokio::task::spawn_blocking(move || -> Result<TranscriptionResult, String> {
        let cache = TRANSCRIBER_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        // Per-call load is fine for now; caching across invocations would need
        // to key by model path and avoid leaking the context across runs.
        drop(cache);
        let transcriber = Transcriber::load(&model_path).map_err(|e| e.to_string())?;
        transcriber
            .transcribe(&resampled, language.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(result)
}

fn active_model_marker<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    Some(dir.join("whisper_active_model"))
}

// Placeholder for a future cross-call cache. Currently unused but reserved so
// that a per-model singleton context can be added without changing the public API.
static TRANSCRIBER_CACHE: Mutex<Option<()>> = Mutex::new(None);

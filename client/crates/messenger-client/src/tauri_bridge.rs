//! Bridge to invoke Tauri native commands from the WASM frontend.
//!
//! On native Tauri builds (desktop, Android), the frontend runs as WASM inside
//! a WebView. AGE encryption/decryption (`age` crate) is not available in WASM,
//! so these operations are delegated to the Tauri native backend via `invoke`.
//!
//! On plain browser WASM (no Tauri), these functions return errors.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Local copy of `BootstrapPayload` to avoid depending on `messenger_core::bootstrap`
/// which is gated behind `#[cfg(feature = "native")]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapPayload {
    pub user_id: Uuid,
    pub username: String,
    pub identity_signing_seed: [u8; 32],
    pub device_signing_seed: [u8; 32],
    pub device_hpke_seed: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub key_package_bundle: Vec<u8>,
    /// Account role ("admin" | "user"), so a provisioned device inherits the
    /// account's role instead of always landing as a plain user. Defaulted for
    /// backward compatibility with blobs from older approving devices.
    #[serde(default)]
    pub role: String,
    /// The user's own avatar (data-URL), so a provisioned device shows it in
    /// settings — avatars are client-only (the server is blind to them), so the
    /// new device has no other way to learn its own.
    #[serde(default)]
    pub avatar: Option<String>,
}

/// Check if the frontend is running inside a Tauri WebView.
///
/// Detects the presence of the `window.__TAURI_INTERNALS__` global object.
#[must_use]
pub fn is_tauri_context() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from("__TAURI_INTERNALS__"))
            .ok()
            .map_or(false, |v| !v.is_undefined())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Store a value in the native Android keystore (via Tauri plugin).
///
/// No-op on non-Tauri builds. On desktop Tauri, the plugin currently returns
/// `ok=true` without actually persisting — the OS keyring path lives elsewhere.
///
/// # Errors
///
/// Returns an error string if the invoke call fails.
pub async fn keystore_set(key: &str, value: &str) -> Result<(), String> {
    if !is_tauri_context() {
        return Err("not in Tauri context".into());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("key"), &JsValue::from(key))
        .map_err(|_| "set key arg".to_string())?;
    js_sys::Reflect::set(&args, &JsValue::from("value"), &JsValue::from(value))
        .map_err(|_| "set value arg".to_string())?;
    tauri_plugin_invoke("android-keystore", "set", &args).await?;
    Ok(())
}

/// Retrieve a value from the native Android keystore.
///
/// Returns `Ok(Some(...))` when the key exists, `Ok(None)` when absent,
/// or `Err(...)` on transport failure. Decodes the base64 payload returned
/// by the Kotlin side and re-encodes as UTF-8 (the caller stores text).
///
/// # Errors
///
/// Returns an error string if the invoke call fails.
pub async fn keystore_get(key: &str) -> Result<Option<String>, String> {
    if !is_tauri_context() {
        return Ok(None);
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("key"), &JsValue::from(key))
        .map_err(|_| "set key arg".to_string())?;
    let result = tauri_plugin_invoke("android-keystore", "get", &args).await?;
    let value = js_sys::Reflect::get(&result, &JsValue::from("value"))
        .map_err(|_| "no value field".to_string())?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let b64 = value.as_string().ok_or("value not string")?;
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("base64: {e}"))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Delete a key from the native Android keystore.
///
/// # Errors
///
/// Returns an error string if the invoke call fails.
pub async fn keystore_delete(key: &str) -> Result<(), String> {
    if !is_tauri_context() {
        return Ok(());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("key"), &JsValue::from(key))
        .map_err(|_| "set key arg".to_string())?;
    tauri_plugin_invoke("android-keystore", "delete", &args).await?;
    Ok(())
}

/// Invoke a Tauri plugin command via `window.__TAURI_INTERNALS__.invoke`.
///
/// Builds the canonical `plugin:<plugin>|<command>` name used by Tauri 2.
async fn tauri_plugin_invoke(plugin: &str, command: &str, args: &js_sys::Object) -> Result<JsValue, String> {
    let cmd = format!("plugin:{plugin}|{command}");
    tauri_invoke(&cmd, args).await
}

/// Encrypt a `BootstrapPayload` for a new device via Tauri's native AGE backend.
///
/// Calls the `age_encrypt_bootstrap` Tauri command.
///
/// # Errors
///
/// Returns an error string if not in Tauri context, or if encryption fails.
pub async fn age_encrypt(
    payload: &BootstrapPayload,
    recipient_pubkey: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let args = js_sys::Object::new();
    js_sys::Reflect::set(
        &args,
        &JsValue::from("payload"),
        &serde_wasm_bindgen::to_value(payload).map_err(|e| e.to_string())?,
    )
    .map_err(|_| "failed to set payload argument".to_string())?;
    js_sys::Reflect::set(
        &args,
        &JsValue::from("recipientPubkey"),
        &js_sys::Uint8Array::from(recipient_pubkey.as_slice()),
    )
    .map_err(|_| "failed to set recipientPubkey argument".to_string())?;

    let result = tauri_invoke("age_encrypt_bootstrap", &args).await?;
    let arr = js_sys::Uint8Array::from(result);
    Ok(arr.to_vec())
}

/// Decrypt a bootstrap blob using a raw X25519 secret seed via Tauri's native AGE backend.
///
/// Calls the `age_decrypt_bootstrap` Tauri command.
///
/// # Errors
///
/// Returns an error string if not in Tauri context, or if decryption fails.
pub async fn age_decrypt(
    blob: &[u8],
    secret_seed: &[u8; 32],
) -> Result<BootstrapPayload, String> {
    let args = js_sys::Object::new();
    js_sys::Reflect::set(
        &args,
        &JsValue::from("blob"),
        &js_sys::Uint8Array::from(blob),
    )
    .map_err(|_| "failed to set blob argument".to_string())?;
    js_sys::Reflect::set(
        &args,
        &JsValue::from("secretSeed"),
        &js_sys::Uint8Array::from(secret_seed.as_slice()),
    )
    .map_err(|_| "failed to set secretSeed argument".to_string())?;

    let result = tauri_invoke("age_decrypt_bootstrap", &args).await?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// Register a media key + session with the native streaming proxy so the
/// `stream://` protocol can range-decrypt this attachment for a `<video>`.
///
/// # Errors
///
/// Returns an error string if not in Tauri context or the command fails.
pub async fn stream_prepare(
    server_url: &str,
    device_id: &str,
    secret_b64: &str,
    attachment_id: &str,
    key_b64: &str,
    mime: &str,
) -> Result<(), String> {
    if !is_tauri_context() {
        return Err("not tauri".to_string());
    }
    let args = js_sys::Object::new();
    let set = |k: &str, v: &str| -> Result<(), String> {
        js_sys::Reflect::set(&args, &JsValue::from(k), &JsValue::from(v))
            .map(|_| ())
            .map_err(|_| format!("set {k}"))
    };
    set("serverUrl", server_url)?;
    set("deviceId", device_id)?;
    set("secretB64", secret_b64)?;
    set("attachmentId", attachment_id)?;
    set("keyB64", key_b64)?;
    set("mime", mime)?;
    tauri_invoke("stream_prepare", &args).await?;
    Ok(())
}

/// Save attachment bytes to the platform's Downloads folder via the file-saver plugin.
///
/// Returns the saved path / URI string.
///
/// # Errors
///
/// Returns an error string if not in Tauri context or if save fails.
pub async fn file_save(
    bytes: &[u8],
    file_name: &str,
    attachment_id: &str,
    mime: &str,
) -> Result<String, String> {
    if !is_tauri_context() {
        // Browser fallback: the web client has no native file-saver plugin,
        // so trigger an ordinary download via a temporary object URL.
        return browser_download(bytes, file_name, mime);
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(
        &args,
        &JsValue::from("bytes"),
        &js_sys::Uint8Array::from(bytes),
    )
    .map_err(|_| "set bytes".to_string())?;
    js_sys::Reflect::set(&args, &JsValue::from("fileName"), &JsValue::from(file_name))
        .map_err(|_| "set fileName".to_string())?;
    js_sys::Reflect::set(
        &args,
        &JsValue::from("attachmentId"),
        &JsValue::from(attachment_id),
    )
    .map_err(|_| "set attachmentId".to_string())?;
    js_sys::Reflect::set(&args, &JsValue::from("mime"), &JsValue::from(mime))
        .map_err(|_| "set mime".to_string())?;
    let result = tauri_invoke("plugin:file-saver|fs_save", &args).await?;
    let path = js_sys::Reflect::get(&result, &JsValue::from("path"))
        .map_err(|_| "no path field".to_string())?
        .as_string()
        .ok_or("path not a string")?;
    Ok(path)
}

/// Save bytes in a plain browser: Blob → object URL → click on a hidden
/// `<a download>` — lands in the user's Downloads folder.
fn browser_download(bytes: &[u8], file_name: &str, mime: &str) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    let u8a = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&u8a.into());
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
        .map_err(|_| "blob".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "url".to_string())?;
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = doc
        .create_element("a")
        .map_err(|_| "anchor".to_string())?
        .unchecked_into();
    a.set_href(&url);
    a.set_download(file_name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(file_name.to_string())
}

/// Check whether an attachment is already saved on disk.
///
/// Returns `Ok(Some(path))` if it exists, `Ok(None)` otherwise.
///
/// # Errors
///
/// Returns an error string if not in Tauri context or if the lookup fails.
pub async fn file_is_saved(attachment_id: &str) -> Result<Option<String>, String> {
    if !is_tauri_context() {
        return Ok(None);
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(
        &args,
        &JsValue::from("attachmentId"),
        &JsValue::from(attachment_id),
    )
    .map_err(|_| "set attachmentId".to_string())?;
    let result = tauri_invoke("plugin:file-saver|fs_is_saved", &args).await?;
    let v = js_sys::Reflect::get(&result, &JsValue::from("path"))
        .map_err(|_| "no path field".to_string())?;
    if v.is_null() || v.is_undefined() {
        Ok(None)
    } else {
        Ok(v.as_string())
    }
}

/// Open a saved file with the system default handler.
///
/// # Errors
///
/// Returns an error string if not in Tauri context or if the open call fails.
pub async fn file_open(path: &str, mime: &str) -> Result<(), String> {
    if !is_tauri_context() {
        return Err("not in Tauri context".into());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("path"), &JsValue::from(path))
        .map_err(|_| "set path".to_string())?;
    js_sys::Reflect::set(&args, &JsValue::from("mime"), &JsValue::from(mime))
        .map_err(|_| "set mime".to_string())?;
    tauri_invoke("plugin:file-saver|fs_open", &args).await?;
    Ok(())
}

// ---------- Voice transcription ----------

#[derive(Debug, Clone, Deserialize)]
pub struct WhisperModelInfo {
    pub id: String,
    pub display_name: String,
    pub size_mb: u32,
    pub ram_mb: u32,
    pub multilingual: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionResult {
    pub text: String,
}

/// List the static catalogue of Whisper models.
///
/// # Errors
///
/// Returns an error if not in Tauri context.
pub async fn transcription_list_models() -> Result<Vec<WhisperModelInfo>, String> {
    if !is_tauri_context() {
        return Ok(Vec::new());
    }
    let args = js_sys::Object::new();
    let result = tauri_invoke("transcription_list_models", &args).await?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// List ids of models already downloaded to disk.
pub async fn transcription_list_downloaded() -> Result<Vec<String>, String> {
    if !is_tauri_context() {
        return Ok(Vec::new());
    }
    let args = js_sys::Object::new();
    let result = tauri_invoke("transcription_list_downloaded", &args).await?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// Download progress of the in-flight model: `(downloaded_bytes, total_bytes)`.
/// `None` when no download is running or not in a Tauri context.
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// Poll the current model-download progress (the settings UI calls this on a
/// short interval while a download is running).
pub async fn transcription_download_progress() -> Option<DownloadProgress> {
    if !is_tauri_context() {
        return None;
    }
    let args = js_sys::Object::new();
    let result = tauri_invoke("transcription_download_progress", &args).await.ok()?;
    Some(serde_wasm_bindgen::from_value(result).ok()?)
}

/// Trigger a model download. Progress is emitted as `transcription://progress` events.
pub async fn transcription_download_model(model_id: &str) -> Result<String, String> {
    if !is_tauri_context() {
        return Err("not in Tauri context".into());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("modelId"), &JsValue::from(model_id))
        .map_err(|_| "set modelId".to_string())?;
    let result = tauri_invoke("transcription_download_model", &args).await?;
    result.as_string().ok_or("no path returned".into())
}

pub async fn transcription_delete_model(model_id: &str) -> Result<(), String> {
    if !is_tauri_context() {
        return Ok(());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("modelId"), &JsValue::from(model_id))
        .map_err(|_| "set modelId".to_string())?;
    tauri_invoke("transcription_delete_model", &args).await?;
    Ok(())
}

pub async fn transcription_get_active() -> Result<Option<String>, String> {
    if !is_tauri_context() {
        return Ok(None);
    }
    let args = js_sys::Object::new();
    let result = tauri_invoke("transcription_get_active", &args).await?;
    if result.is_null() || result.is_undefined() {
        Ok(None)
    } else {
        Ok(result.as_string())
    }
}

pub async fn transcription_set_active(model_id: &str) -> Result<(), String> {
    if !is_tauri_context() {
        return Ok(());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(&args, &JsValue::from("modelId"), &JsValue::from(model_id))
        .map_err(|_| "set modelId".to_string())?;
    tauri_invoke("transcription_set_active", &args).await?;
    Ok(())
}

/// Transcribe PCM samples on the Tauri side via whisper.
///
/// `samples` is `f32` channel data already decoded by the WebView's
/// `AudioContext`; it gets reinterpreted as little-endian `f32` bytes for the
/// invoke payload (a `Uint8Array`) so we don't blow up the JSON encoder with
/// millions of numbers.
pub async fn transcription_transcribe(
    samples: &[f32],
    sample_rate: u32,
    language: Option<&str>,
) -> Result<String, String> {
    if !is_tauri_context() {
        return Err("not in Tauri context".into());
    }
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    let args = js_sys::Object::new();
    js_sys::Reflect::set(
        &args,
        &JsValue::from("samplesBytes"),
        &js_sys::Uint8Array::from(bytes.as_slice()),
    )
    .map_err(|_| "set samplesBytes".to_string())?;
    js_sys::Reflect::set(
        &args,
        &JsValue::from("sampleRate"),
        &JsValue::from(sample_rate),
    )
    .map_err(|_| "set sampleRate".to_string())?;
    if let Some(lang) = language {
        js_sys::Reflect::set(&args, &JsValue::from("language"), &JsValue::from(lang))
            .map_err(|_| "set language".to_string())?;
    }
    let result = tauri_invoke("transcription_transcribe", &args).await?;
    let parsed: TranscriptionResult =
        serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())?;
    Ok(parsed.text)
}

/// Low-level Tauri invoke call via `window.__TAURI_INTERNALS__.invoke()`.
async fn tauri_invoke(cmd: &str, args: &js_sys::Object) -> Result<JsValue, String> {
    let global = js_sys::global();
    let tauri_internals = js_sys::Reflect::get(&global, &JsValue::from("__TAURI_INTERNALS__"))
        .map_err(|_| "not in Tauri context".to_string())?;

    let invoke_fn = js_sys::Reflect::get(&tauri_internals, &JsValue::from("invoke"))
        .map_err(|_| "__TAURI_INTERNALS__.invoke not found".to_string())?;

    let invoke_fn: js_sys::Function = invoke_fn
        .dyn_into()
        .map_err(|_| "__TAURI_INTERNALS__.invoke is not a function".to_string())?;

    let promise = invoke_fn
        .call2(&tauri_internals, &JsValue::from(cmd), args)
        .map_err(|e| format!("invoke call failed: {:?}", e))?;

    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| "invoke did not return a Promise".to_string())?;

    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("invoke rejected: {:?}", e))
}

/// One attachment shared into the app via the Android "Share" sheet.
#[derive(Debug, Clone, Deserialize)]
pub struct SharedAttachment {
    pub name: String,
    pub mime: String,
    /// File bytes, base64 (standard) encoded.
    pub b64: String,
}

/// Drain the Android "share" inbox: files the user shared into the app from
/// the gallery or another app. Each call returns the pending items and clears
/// them natively. Empty on non-Tauri / non-Android builds.
pub async fn take_shared_attachments() -> Vec<SharedAttachment> {
    if !is_tauri_context() {
        return Vec::new();
    }
    let args = js_sys::Object::new();
    match tauri_invoke("take_shared_attachments", &args).await {
        Ok(result) => serde_wasm_bindgen::from_value(result).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

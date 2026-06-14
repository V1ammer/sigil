//! Tauri commands for AGE bootstrap encrypt/decrypt.
//!
//! These commands are invoked from the WASM frontend via `__TAURI_INTERNALS__.invoke()`
//! on platforms where `age` is available natively (desktop, Android) but not in WASM.

use messenger_core::age_wrap::recipient_from_raw_public;
use messenger_core::bootstrap::{build_bootstrap, open_bootstrap_raw_secret, BootstrapPayload};

/// Encrypt a bootstrap payload for a new device.
///
/// # Errors
///
/// Returns a string error description on encryption failure.
#[tauri::command]
pub fn age_encrypt_bootstrap(
    payload: BootstrapPayload,
    recipient_pubkey: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let pk: [u8; 32] = recipient_pubkey
        .try_into()
        .map_err(|_| "recipient_pubkey must be 32 bytes".to_string())?;
    let recipient = recipient_from_raw_public(&pk);
    build_bootstrap(&payload, &recipient).map_err(|e| e.to_string())
}

/// Decrypt a bootstrap blob using a raw X25519 secret seed.
///
/// # Errors
///
/// Returns a string error description on decryption failure.
#[tauri::command]
pub fn age_decrypt_bootstrap(
    blob: Vec<u8>,
    secret_seed: Vec<u8>,
) -> Result<BootstrapPayload, String> {
    let seed: [u8; 32] = secret_seed
        .try_into()
        .map_err(|_| "secret_seed must be 32 bytes".to_string())?;
    open_bootstrap_raw_secret(&blob, &seed).map_err(|e| e.to_string())
}

/// One attachment shared into the app via the Android "Share" sheet.
#[derive(serde::Serialize)]
pub struct SharedAttachment {
    pub name: String,
    pub mime: String,
    /// File bytes, base64 (standard) encoded.
    pub b64: String,
}

/// Drain the share inbox written by `MainActivity.handleShareIntent`.
///
/// Reads every `<id>.json` + `<id>.data` pair from the app's private
/// `files/share_inbox`, returns them, and deletes them so each share is
/// delivered to the frontend exactly once. Empty on non-Android or when nothing
/// was shared.
#[tauri::command]
pub fn take_shared_attachments() -> Vec<SharedAttachment> {
    #[cfg(target_os = "android")]
    {
        use base64::Engine as _;
        // MainActivity writes to `filesDir/share_inbox`. The app's data dir is
        // `/data/user/0/<id>/files` on modern Android, historically symlinked as
        // `/data/data/<id>/files`; try both so we read regardless.
        let candidates = [
            "/data/user/0/com.example.messenger/files/share_inbox",
            "/data/data/com.example.messenger/files/share_inbox",
        ];
        let mut out = Vec::new();
        let Some(entries) = candidates
            .iter()
            .find_map(|p| std::fs::read_dir(p).ok())
        else {
            return out;
        };
        for entry in entries.flatten() {
            let json_path = entry.path();
            if json_path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let data_path = json_path.with_extension("data");
            let meta = std::fs::read_to_string(&json_path).ok();
            let bytes = std::fs::read(&data_path).ok();
            if let (Some(meta), Some(bytes)) = (meta, bytes) {
                let v: serde_json::Value = serde_json::from_str(&meta).unwrap_or_default();
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("shared").to_string();
                let mime = v
                    .get("mime")
                    .and_then(|x| x.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                out.push(SharedAttachment { name, mime, b64 });
            }
            let _ = std::fs::remove_file(&json_path);
            let _ = std::fs::remove_file(&data_path);
        }
        out
    }
    #[cfg(not(target_os = "android"))]
    {
        Vec::new()
    }
}

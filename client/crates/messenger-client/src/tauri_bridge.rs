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

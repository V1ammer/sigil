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

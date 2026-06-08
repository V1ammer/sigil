//! Mobile keystore bridge — Rust → Kotlin via Tauri's PluginHandle.

use serde_json::json;
use tauri::plugin::PluginHandle;
use tauri::Runtime;

/// Mobile keystore that bridges to the Kotlin plugin via Tauri's invoke system.
pub struct KeystoreMobile<R: Runtime> {
    handle: PluginHandle<R>,
}

impl<R: Runtime> KeystoreMobile<R> {
    /// Create a new mobile keystore with the registered plugin handle.
    pub fn new(handle: PluginHandle<R>) -> Self {
        Self { handle }
    }

    /// Store a value in the Android EncryptedSharedPreferences.
    pub fn set(&self, key: &str, value: &[u8]) -> Result<super::KeystoreResponse, String> {
        let v_b64 = base64::engine::general_purpose::STANDARD.encode(value);
        self.handle
            .run_mobile_plugin::<serde_json::Value>(
                "set",
                json!({ "key": key, "value": v_b64 }),
            )
            .map_err(|e| e.to_string())?;
        tracing::debug!("[keystore::mobile] set({key}) = {} bytes", value.len());
        Ok(super::KeystoreResponse { ok: true })
    }

    /// Retrieve a value from the Android EncryptedSharedPreferences.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let result = self
            .handle
            .run_mobile_plugin::<serde_json::Value>("get", json!({ "key": key }))
            .map_err(|e| e.to_string())?;

        let value = result
            .get("value")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(s)
                    .ok()
            });

        tracing::debug!("[keystore::mobile] get({key})");
        Ok(value)
    }

    /// Delete a value from the Android EncryptedSharedPreferences.
    pub fn delete(&self, key: &str) -> Result<super::KeystoreResponse, String> {
        self.handle
            .run_mobile_plugin::<serde_json::Value>("delete", json!({ "key": key }))
            .map_err(|e| e.to_string())?;
        tracing::debug!("[keystore::mobile] delete({key})");
        Ok(super::KeystoreResponse { ok: true })
    }
}

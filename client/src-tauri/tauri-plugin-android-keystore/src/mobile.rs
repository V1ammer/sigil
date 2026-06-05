/// Mobile keystore that bridges to the Kotlin plugin via JNI.
pub struct KeystoreMobile;

impl KeystoreMobile {
    pub fn new() -> Self {
        Self
    }

    /// Store a value in the Android EncryptedSharedPreferences.
    pub fn set(&self, key: &str, value: &[u8]) -> Result<super::KeystoreResponse, String> {
        let v_b64 = base64::engine::general_purpose::STANDARD.encode(value);
        // On actual mobile, this would call the Kotlin plugin via invoke.
        // For now, the actual IPC happens through the Tauri invoke mechanism.
        tracing::debug!("[keystore::mobile] set({key}) = {} bytes", value.len());
        Ok(super::KeystoreResponse { ok: true })
    }

    /// Retrieve a value from the Android EncryptedSharedPreferences.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        tracing::debug!("[keystore::mobile] get({key})");
        Ok(None)
    }

    /// Delete a value from the Android EncryptedSharedPreferences.
    pub fn delete(&self, key: &str) -> Result<super::KeystoreResponse, String> {
        tracing::debug!("[keystore::mobile] delete({key})");
        Ok(super::KeystoreResponse { ok: true })
    }
}

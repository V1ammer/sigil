//! File-based secret store for Android (temporary MVP replacement for Keystore).

use crate::{error::StorageError, traits::SecretStore};
use async_trait::async_trait;
use base64::Engine;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const SECRETS_FILE: &str = "secrets.json";

/// Secret store using a local JSON file (MVP fallback for Android).
///
/// In production, use the Tauri Keystore plugin for hardware-backed storage.
pub struct FileSecretStore {
    file_path: PathBuf,
    /// In-memory cache + persistence
    cache: Mutex<HashMap<String, String>>,
}

impl FileSecretStore {
    /// Create a new file secret store in the given directory.
    pub fn new(dir: &PathBuf) -> Self {
        let file_path = dir.join(SECRETS_FILE);
        let cache = if file_path.exists() {
            std::fs::read_to_string(&file_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self {
            file_path,
            cache: Mutex::new(cache),
        }
    }

    fn persist(&self) -> Result<(), StorageError> {
        let data = serde_json::to_string(&*self.cache.lock().unwrap())
            .map_err(|e| StorageError::Io(e.to_string()))?;
        std::fs::write(&self.file_path, data)
            .map_err(|e| StorageError::Io(e.to_string()))
    }
}

#[async_trait(?Send)]
impl SecretStore for FileSecretStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let cache = self.cache.lock().unwrap();
        match cache.get(key) {
            Some(val) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(val)
                    .map_err(|e| StorageError::Crypto(e.to_string()))?;
                Ok(Some(bytes))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(value);
        self.cache.lock().unwrap().insert(key.to_string(), encoded);
        self.persist()
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.cache.lock().unwrap().remove(key);
        self.persist()
    }
}

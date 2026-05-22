//! Desktop secret store backed by the OS keyring.

use crate::{error::StorageError, traits::SecretStore};
use async_trait::async_trait;
use base64::Engine;

const KEYRING_SERVICE: &str = "com.example.messenger";

/// Secret store implementation using the OS keyring (keyring crate).
pub struct KeyringSecretStore {
    user: String,
}

impl KeyringSecretStore {
    /// Create a new keyring store for the given user/profile.
    pub fn new(user: &str) -> Self {
        Self {
            user: user.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl SecretStore for KeyringSecretStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let id = format!("{}:{}", self.user, key);
        let entry = keyring::Entry::new(KEYRING_SERVICE, &id)
            .map_err(|e| StorageError::Platform(e.to_string()))?;
        match entry.get_password() {
            Ok(s) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&s)
                    .map_err(|e| StorageError::Crypto(e.to_string()))?;
                Ok(Some(bytes))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(StorageError::Platform(e.to_string())),
        }
    }

    async fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let id = format!("{}:{}", self.user, key);
        let entry = keyring::Entry::new(KEYRING_SERVICE, &id)
            .map_err(|e| StorageError::Platform(e.to_string()))?;
        let s = base64::engine::general_purpose::STANDARD.encode(value);
        entry
            .set_password(&s)
            .map_err(|e| StorageError::Platform(e.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let id = format!("{}:{}", self.user, key);
        let entry = keyring::Entry::new(KEYRING_SERVICE, &id)
            .map_err(|e| StorageError::Platform(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(StorageError::Platform(e.to_string())),
        }
    }
}

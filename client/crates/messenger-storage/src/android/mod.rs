//! Android storage implementation using stock SQLite + file-based secrets.
//!
//! Database uses plain SQLite (no encryption at the DB layer for MVP —
//! SQLCipher requires OpenSSL cross-compilation for Android, which is deferred).
//! Secrets are stored in a JSON file in the app's private data directory.
//! In production, the Keystore Tauri plugin should be used for
//! hardware-backed secret storage.

use crate::{
    error::StorageError,
    traits::{LocalDatabase, MessengerLocalStore},
};
use std::path::PathBuf;

mod secret_store;
mod database;
mod messenger_store;

pub use secret_store::FileSecretStore;
pub use database::AndroidDatabase;
pub use messenger_store::AndroidMessengerStore;

/// Resolve the app's data directory.
fn data_dir() -> Result<PathBuf, StorageError> {
    // On Android, try env vars first, then fall back
    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".messenger"));
    }
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        return Ok(PathBuf::from(tmpdir).join("messenger"));
    }
    // Fallback for Android
    Ok(PathBuf::from("/data/data/com.example.messenger/files"))
}

/// Initialise Android storage for a profile.
pub async fn init(profile_name: &str) -> Result<Box<dyn MessengerLocalStore>, StorageError> {
    let base_dir = data_dir()?;
    std::fs::create_dir_all(&base_dir)
        .map_err(|e| StorageError::Io(e.to_string()))?;

    let db_path = base_dir.join(format!("{}.db", profile_name));

    let db = AndroidDatabase::open(db_path)?;

    // Apply migrations
    let migrations = crate::migrations::schema_v1();
    for sql in migrations {
        db.execute(sql, &[]).await?;
    }

    let store = AndroidMessengerStore::new(db);
    Ok(Box::new(store))
}

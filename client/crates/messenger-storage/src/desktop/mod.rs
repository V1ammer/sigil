//! Desktop implementation: OS keyring + SQLCipher.

use crate::{
    error::StorageError,
    traits::{LocalDatabase, MessengerLocalStore, SecretStore},
};

mod secret_store;
mod database;
mod messenger_store;

pub use secret_store::KeyringSecretStore;
pub use database::SqlcipherDatabase;
pub use messenger_store::SqlcipherMessengerStore;

#[cfg(test)]
mod tests;

/// Initialise desktop storage for a profile.
pub async fn init(profile_name: &str) -> Result<Box<dyn MessengerLocalStore>, StorageError> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| StorageError::Io("unable to determine data directory".into()))?
        .join("com.example.messenger");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| StorageError::Io(e.to_string()))?;

    let db_path = data_dir.join(format!("{}.db", profile_name));

    // Derive or load the database encryption key from the OS keyring.
    let keyring = KeyringSecretStore::new(profile_name);
    let db_key_name = "db_key";
    let db_key: [u8; 32] = match keyring.get(db_key_name).await? {
        Some(bytes) => {
            bytes.try_into().map_err(|_| {
                StorageError::Crypto("stored db_key has wrong length".into())
            })?
        }
        None => {
            let mut key = [0u8; 32];
            getrandom::getrandom(&mut key)
                .map_err(|e| StorageError::Crypto(e.to_string()))?;
            keyring.set(db_key_name, &key).await?;
            key
        }
    };

    let db = SqlcipherDatabase::open(db_path, &db_key)?;

    // Apply migrations.
    let migrations = crate::migrations::schema_v1();
    for sql in migrations {
        db.execute(sql, &[]).await?;
    }

    let store = SqlcipherMessengerStore::new(db);
    Ok(Box::new(store))
}

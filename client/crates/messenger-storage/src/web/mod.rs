//! Web implementation: WebCrypto + IndexedDB.

use crate::{
    error::StorageError,
    traits::MessengerLocalStore,
};
use idb::{Database, DatabaseEvent, Factory, ObjectStoreParams, TransactionMode};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

mod secret_store;
mod messenger_store;

pub use secret_store::WebCryptoSecretStore;
pub use messenger_store::IndexedDbMessengerStore;

/// Open or create the IndexedDB used by the app.
async fn open_db(name: &str) -> Result<Database, StorageError> {
    let factory = Factory::new().map_err(|e| StorageError::Platform(format!("{e:?}")))?;

    let mut open_req = factory.open(name, Some(1)).map_err(js_err)?;
    open_req.on_upgrade_needed(|event| {
        let db = match event.database() {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("IDB upgrade failed: {e:?}");
                return;
            }
        };
        let _ = db.create_object_store("secrets", ObjectStoreParams::new());
        let _ = db.create_object_store("messages", ObjectStoreParams::new());
        let _ = db.create_object_store("chats", ObjectStoreParams::new());
        let _ = db.create_object_store("mls_groups", ObjectStoreParams::new());
        let _ = db.create_object_store("identity", ObjectStoreParams::new());
        let _ = db.create_object_store("keypackages", ObjectStoreParams::new());
        let _ = db.create_object_store("settings", ObjectStoreParams::new());
        let _ = db.create_object_store("attachments", ObjectStoreParams::new());
        let _ = db.create_object_store("keys", ObjectStoreParams::new());
    });
    open_req.await.map_err(|e| StorageError::Platform(format!("{e:?}")))
}

/// Initialise web storage for a profile.
pub async fn init(profile_name: &str) -> Result<Box<dyn MessengerLocalStore>, StorageError> {
    let db = open_db(profile_name).await?;
    let secret_store = WebCryptoSecretStore::open(profile_name).await?;
    let store = IndexedDbMessengerStore::new(db, secret_store);
    Ok(Box::new(store))
}

/// Convert a JsValue error to `StorageError`.
fn js_err<E: core::fmt::Debug>(e: E) -> StorageError {
    StorageError::Platform(format!("{e:?}"))
}

/// Get the SubtleCrypto instance.
fn subtle() -> Result<web_sys::SubtleCrypto, StorageError> {
    web_sys::window()
        .ok_or_else(|| StorageError::Platform("no window".into()))?
        .crypto()
        .map(|c| c.subtle())
        .map_err(|e| StorageError::Platform(format!("{e:?}")))
}

/// Generate a non-extractable AES-256-GCM master key.
async fn generate_master_key() -> Result<web_sys::CryptoKey, StorageError> {
    let subtle = subtle()?;
    let alg = js_sys::Object::new();
    js_sys::Reflect::set(&alg, &"name".into(), &"AES-GCM".into()).unwrap();
    js_sys::Reflect::set(&alg, &"length".into(), &256.into()).unwrap();

    let usages = js_sys::Array::of2(&"encrypt".into(), &"decrypt".into());
    let key_promise = subtle
        .generate_key_with_object(&alg, false, &usages)
        .map_err(js_err)?;
    let key = JsFuture::from(key_promise).await.map_err(js_err)?;
    Ok(key.dyn_into().map_err(|_| StorageError::Platform("invalid key type".into()))?)
}

/// Retrieve or generate the master key from IndexedDB.
async fn get_or_generate_master_key(db: &Database) -> Result<web_sys::CryptoKey, StorageError> {
    let tx = db
        .transaction(&["keys"], TransactionMode::ReadWrite)
        .map_err(js_err)?;
    let store = tx.object_store("keys").map_err(js_err)?;

    // Try to get existing key
    let existing = store
        .get(JsValue::from_str("master"))
        .map_err(js_err)?
        .await
        .map_err(js_err)?;
    if let Some(val) = existing {
        return val
            .dyn_into()
            .map_err(|_| StorageError::Platform("master key type mismatch".into()));
    }

    // Generate new key
    let key = generate_master_key().await?;
    store
        .put(&key, Some(&JsValue::from_str("master")))
        .map_err(js_err)?
        .await
        .map_err(js_err)?;
    tx.await.map_err(js_err)?;
    Ok(key)
}

use wasm_bindgen::JsValue;

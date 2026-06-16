//! WebCrypto-backed secret store using IndexedDB.

use crate::error::StorageError;
use crate::traits::SecretStore;
use async_trait::async_trait;
use idb::{Database, Query, TransactionMode};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::{get_or_generate_master_key, js_err, subtle};

/// Secret store implementation using WebCrypto + IndexedDB.
pub struct WebCryptoSecretStore {
    db: Database,
    master_key: web_sys::CryptoKey,
}

impl WebCryptoSecretStore {
    /// Open (or create) the secret store.
    pub async fn open(db_name: &str) -> Result<Self, StorageError> {
        let db = super::open_db(db_name).await?;
        let master_key = get_or_generate_master_key(&db).await?;
        Ok(Self { db, master_key })
    }

    /// Encrypt plaintext with AES-GCM using the master key.
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, StorageError> {
        let subtle = subtle()?;
        let window = web_sys::window().ok_or_else(|| StorageError::Platform("no window".into()))?;
        let crypto = window.crypto().map_err(|e| StorageError::Platform(format!("{e:?}")))?;

        let mut iv = [0u8; 12];
        crypto
            .get_random_values_with_u8_array(&mut iv)
            .map_err(|e| StorageError::Crypto(format!("{e:?}")))?;

        let alg = js_sys::Object::new();
        js_sys::Reflect::set(&alg, &"name".into(), &"AES-GCM".into()).unwrap();
        js_sys::Reflect::set(
            &alg,
            &"iv".into(),
            &js_sys::Uint8Array::from(&iv[..]).into(),
        )
        .unwrap();

        let ct = JsFuture::from(
            subtle
                .encrypt_with_object_and_u8_array(&alg, &self.master_key, plaintext)
                .map_err(js_err)?,
        )
        .await
        .map_err(js_err)?;

        let ct_arr = js_sys::Uint8Array::new(&ct);
        let mut result = iv.to_vec();
        result.extend_from_slice(&ct_arr.to_vec());
        Ok(result)
    }

    /// Decrypt ciphertext (iv || ct) with AES-GCM using the master key.
    async fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>, StorageError> {
        if blob.len() < 12 {
            return Err(StorageError::Crypto("ciphertext too short".into()));
        }
        let (iv, ct) = blob.split_at(12);

        let subtle = subtle()?;
        let alg = js_sys::Object::new();
        js_sys::Reflect::set(&alg, &"name".into(), &"AES-GCM".into()).unwrap();
        js_sys::Reflect::set(
            &alg,
            &"iv".into(),
            &js_sys::Uint8Array::from(iv).into(),
        )
        .unwrap();

        let pt = JsFuture::from(
            subtle
                .decrypt_with_object_and_u8_array(&alg, &self.master_key, ct)
                .map_err(js_err)?,
        )
        .await
        .map_err(js_err)?;

        let pt_arr = js_sys::Uint8Array::new(&pt);
        Ok(pt_arr.to_vec())
    }
}

#[async_trait(?Send)]
impl SecretStore for WebCryptoSecretStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let tx = self
            .db
            .transaction(&["secrets"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("secrets").map_err(js_err)?;
        let val = store
            .get(JsValue::from_str(key))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        // Stored as a Uint8Array; IndexedDB returns it as one. Handle both shapes
        // (a plain ArrayBuffer cast silently dropped every read).
        let encrypted: Option<Vec<u8>> = val.and_then(|v| {
            if v.is_undefined() || v.is_null() {
                None
            } else if let Ok(u8a) = v.clone().dyn_into::<js_sys::Uint8Array>() {
                Some(u8a.to_vec())
            } else if let Ok(buf) = v.dyn_into::<js_sys::ArrayBuffer>() {
                Some(js_sys::Uint8Array::new(&buf).to_vec())
            } else {
                None
            }
        });
        match encrypted {
            Some(blob) => self.decrypt(&blob).await.map(Some),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let encrypted = self.encrypt(value).await?;
        let tx = self
            .db
            .transaction(&["secrets"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("secrets").map_err(js_err)?;
        let arr = js_sys::Uint8Array::from(&encrypted[..]);
        store
            .put(&arr.into(), Some(&JsValue::from_str(key)))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["secrets"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("secrets").map_err(js_err)?;
        store
            .delete(Query::Key(JsValue::from_str(key)))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }
}

use wasm_bindgen::JsValue;

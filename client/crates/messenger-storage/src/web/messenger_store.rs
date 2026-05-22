//! High-level messenger store backed by IndexedDB (web).

use crate::{
    error::StorageError,
    traits::{MessengerLocalStore, SecretStore},
    types::*,
};
use async_trait::async_trait;
use idb::{Database, Query, TransactionMode};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

use super::{js_err, WebCryptoSecretStore};

/// Web messenger store using IndexedDB + WebCrypto.
pub struct IndexedDbMessengerStore {
    db: Database,
    secrets: WebCryptoSecretStore,
}

impl IndexedDbMessengerStore {
    /// Create a new store.
    pub fn new(db: Database, secrets: WebCryptoSecretStore) -> Self {
        Self { db, secrets }
    }
}

#[async_trait(?Send)]
impl MessengerLocalStore for IndexedDbMessengerStore {
    // ------------------------------------------------------------------
    // Identity
    // ------------------------------------------------------------------
    async fn save_identity(
        &self,
        user_id: Uuid,
        identity: &EncryptedIdentity,
    ) -> Result<(), StorageError> {
        let data = serde_json::to_vec(identity).map_err(|e| StorageError::Crypto(e.to_string()))?;
        self.secrets
            .set(&format!("identity:{user_id}"), &data)
            .await
    }

    async fn load_identity(
        &self,
        user_id: Uuid,
    ) -> Result<Option<EncryptedIdentity>, StorageError> {
        let data = self
            .secrets
            .get(&format!("identity:{user_id}"))
            .await?;
        match data {
            Some(bytes) => {
                let id: EncryptedIdentity =
                    serde_json::from_slice(&bytes).map_err(|e| StorageError::Crypto(e.to_string()))?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    // ------------------------------------------------------------------
    // MLS groups
    // ------------------------------------------------------------------
    async fn save_mls_group_state(
        &self,
        group_id: Uuid,
        state: &[u8],
    ) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["mls_groups"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("mls_groups").map_err(js_err)?;
        let arr = js_sys::Uint8Array::from(state);
        store
            .put(&arr.into(), Some(&JsValue::from_str(&group_id.to_string())))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    async fn load_mls_group_state(
        &self,
        group_id: Uuid,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let tx = self
            .db
            .transaction(&["mls_groups"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("mls_groups").map_err(js_err)?;
        let val = store
            .get(JsValue::from_str(&group_id.to_string()))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        Ok(val.and_then(|v| v.dyn_into::<js_sys::ArrayBuffer>().ok()).map(|b| {
            js_sys::Uint8Array::new(&b).to_vec()
        }))
    }

    async fn list_mls_group_ids(&self) -> Result<Vec<Uuid>, StorageError> {
        let tx = self
            .db
            .transaction(&["mls_groups"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("mls_groups").map_err(js_err)?;
        let keys = store
            .get_all_keys(None, None)
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        let mut result = Vec::new();
        for key in keys {
            if let Some(s) = key.as_string() {
                if let Ok(u) = Uuid::parse_str(&s) {
                    result.push(u);
                }
            }
        }
        Ok(result)
    }

    // ------------------------------------------------------------------
    // Chats
    // ------------------------------------------------------------------
    async fn save_chat_meta(&self, chat: &ChatMeta) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["chats"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("chats").map_err(js_err)?;
        let data = serde_json::to_string(chat).map_err(|e| StorageError::Crypto(e.to_string()))?;
        store
            .put(&data.into(), Some(&JsValue::from_str(&chat.group_id.to_string())))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    async fn list_chats(&self) -> Result<Vec<ChatMeta>, StorageError> {
        let tx = self
            .db
            .transaction(&["chats"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("chats").map_err(js_err)?;
        let vals = store.get_all(None, None).map_err(js_err)?.await.map_err(js_err)?;
        let mut result = Vec::new();
        for val in vals {
            if let Some(s) = val.as_string() {
                let chat: ChatMeta =
                    serde_json::from_str(&s).map_err(|e| StorageError::Crypto(e.to_string()))?;
                result.push(chat);
            }
        }
        // Sort by last_message_at desc
        result.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));
        Ok(result)
    }

    // ------------------------------------------------------------------
    // Messages
    // ------------------------------------------------------------------
    async fn save_message(&self, msg: &CachedMessage) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["messages"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("messages").map_err(js_err)?;
        let data = serde_json::to_string(msg).map_err(|e| StorageError::Crypto(e.to_string()))?;
        store
            .put(&data.into(), Some(&JsValue::from_str(&msg.id.to_string())))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    async fn list_messages(
        &self,
        group_id: Uuid,
        limit: usize,
        _before_id: Option<Uuid>,
    ) -> Result<Vec<CachedMessage>, StorageError> {
        let tx = self
            .db
            .transaction(&["messages"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("messages").map_err(js_err)?;
        let vals = store.get_all(None, None).map_err(js_err)?.await.map_err(js_err)?;
        let mut result = Vec::new();
        for val in vals {
            if let Some(s) = val.as_string() {
                let msg: CachedMessage =
                    serde_json::from_str(&s).map_err(|e| StorageError::Crypto(e.to_string()))?;
                if msg.group_id == group_id {
                    result.push(msg);
                }
            }
        }
        // Sort by created_at desc, then take limit
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result.truncate(limit);
        Ok(result)
    }

    async fn mark_message_state(
        &self,
        message_id: Uuid,
        edited_at: Option<i64>,
        deleted_at: Option<i64>,
    ) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["messages"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("messages").map_err(js_err)?;
        let val = store
            .get(JsValue::from_str(&message_id.to_string()))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        if let Some(v) = val {
            if let Some(s) = v.as_string() {
                let mut msg: CachedMessage = serde_json::from_str(&s)
                    .map_err(|e| StorageError::Crypto(e.to_string()))?;
                msg.edited_at = edited_at;
                msg.deleted_at = deleted_at;
                let data = serde_json::to_string(&msg)
                    .map_err(|e| StorageError::Crypto(e.to_string()))?;
                store
                    .put(&data.into(), Some(&JsValue::from_str(&message_id.to_string())))
                    .map_err(js_err)?
                    .await
                    .map_err(js_err)?;
            }
        }
        tx.await.map_err(js_err)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // KeyPackages
    // ------------------------------------------------------------------
    async fn save_keypackage_local(&self, kp: &LocalKeyPackage) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["keypackages"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("keypackages").map_err(js_err)?;
        let data = serde_json::to_string(kp).map_err(|e| StorageError::Crypto(e.to_string()))?;
        store
            .put(&data.into(), Some(&JsValue::from_str(&kp.id.to_string())))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    async fn list_local_keypackages(&self) -> Result<Vec<LocalKeyPackage>, StorageError> {
        let tx = self
            .db
            .transaction(&["keypackages"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("keypackages").map_err(js_err)?;
        let vals = store.get_all(None, None).map_err(js_err)?.await.map_err(js_err)?;
        let mut result = Vec::new();
        for val in vals {
            if let Some(s) = val.as_string() {
                let kp: LocalKeyPackage =
                    serde_json::from_str(&s).map_err(|e| StorageError::Crypto(e.to_string()))?;
                result.push(kp);
            }
        }
        Ok(result)
    }

    async fn delete_local_keypackage(&self, id: Uuid) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["keypackages"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("keypackages").map_err(js_err)?;
        store
            .delete(Query::Key(JsValue::from_str(&id.to_string())))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Settings
    // ------------------------------------------------------------------
    async fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        let tx = self
            .db
            .transaction(&["settings"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("settings").map_err(js_err)?;
        let val = store
            .get(JsValue::from_str(key))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        Ok(val.and_then(|v| v.as_string()))
    }

    async fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["settings"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("settings").map_err(js_err)?;
        store
            .put(&value.into(), Some(&JsValue::from_str(key)))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Attachments
    // ------------------------------------------------------------------
    async fn save_attachment_meta(&self, att: &AttachmentMeta) -> Result<(), StorageError> {
        let tx = self
            .db
            .transaction(&["attachments"], TransactionMode::ReadWrite)
            .map_err(js_err)?;
        let store = tx.object_store("attachments").map_err(js_err)?;
        let data = serde_json::to_string(att).map_err(|e| StorageError::Crypto(e.to_string()))?;
        store
            .put(&data.into(), Some(&JsValue::from_str(&att.attachment_id.to_string())))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        tx.await.map_err(js_err)?;
        Ok(())
    }

    async fn load_attachment_meta(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<AttachmentMeta>, StorageError> {
        let tx = self
            .db
            .transaction(&["attachments"], TransactionMode::ReadOnly)
            .map_err(js_err)?;
        let store = tx.object_store("attachments").map_err(js_err)?;
        let val = store
            .get(JsValue::from_str(&attachment_id.to_string()))
            .map_err(js_err)?
            .await
            .map_err(js_err)?;
        match val.and_then(|v| v.as_string()) {
            Some(s) => {
                let att: AttachmentMeta =
                    serde_json::from_str(&s).map_err(|e| StorageError::Crypto(e.to_string()))?;
                Ok(Some(att))
            }
            None => Ok(None),
        }
    }
}

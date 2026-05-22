//! Android-specific implementation skeleton.
//!
//! Uses Android Keystore (via JNI) for key wrapping.
//!
//! Strategy:
//! 1. At first run, generate an AES-256-GCM key in Android Keystore (hardware-backed if available).
//! 2. The local DB encryption key (32 bytes random) is stored in a file, encrypted by
//!    that Keystore-resident AES key via JNI cipher.doFinal.
//! 3. App also uses Keystore directly for small secrets (identity_seed) wrapped + stored
//!    as encrypted blobs in SharedPreferences.
//!
//! TODO: Full implementation in C12 when testing on a real Android device.

use crate::{
    error::StorageError,
    traits::{MessengerLocalStore, SecretStore},
    types::*,
};
use async_trait::async_trait;
use uuid::Uuid;

/// Initialise Android storage for a profile.
pub async fn init(_profile_name: &str) -> Result<Box<dyn MessengerLocalStore>, StorageError> {
    // Placeholder: will be wired to Tauri Mobile plugin / JNI in C12.
    Err(StorageError::Platform(
        "Android storage not yet implemented — see C12".into(),
    ))
}

/// Android Keystore secret store (skeleton).
pub struct AndroidKeystoreSecretStore;

impl AndroidKeystoreSecretStore {
    /// Create a new store.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait(?Send)]
impl SecretStore for AndroidKeystoreSecretStore {
    async fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        todo!("Android Keystore get: implement via Tauri plugin or JNI in C12")
    }

    async fn set(&self, _key: &str, _value: &[u8]) -> Result<(), StorageError> {
        todo!("Android Keystore set: implement via Tauri plugin or JNI in C12")
    }

    async fn delete(&self, _key: &str) -> Result<(), StorageError> {
        todo!("Android Keystore delete: implement via Tauri plugin or JNI in C12")
    }
}

/// Android messenger store (skeleton).
pub struct AndroidMessengerStore;

impl AndroidMessengerStore {
    /// Create a new store.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait(?Send)]
impl MessengerLocalStore for AndroidMessengerStore {
    async fn save_identity(
        &self,
        _user_id: Uuid,
        _identity: &EncryptedIdentity,
    ) -> Result<(), StorageError> {
        todo!()
    }

    async fn load_identity(
        &self,
        _user_id: Uuid,
    ) -> Result<Option<EncryptedIdentity>, StorageError> {
        todo!()
    }

    async fn save_mls_group_state(
        &self,
        _group_id: Uuid,
        _state: &[u8],
    ) -> Result<(), StorageError> {
        todo!()
    }

    async fn load_mls_group_state(
        &self,
        _group_id: Uuid,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        todo!()
    }

    async fn list_mls_group_ids(&self) -> Result<Vec<Uuid>, StorageError> {
        todo!()
    }

    async fn save_chat_meta(&self, _chat: &ChatMeta) -> Result<(), StorageError> {
        todo!()
    }

    async fn list_chats(&self) -> Result<Vec<ChatMeta>, StorageError> {
        todo!()
    }

    async fn save_message(&self, _msg: &CachedMessage) -> Result<(), StorageError> {
        todo!()
    }

    async fn list_messages(
        &self,
        _group_id: Uuid,
        _limit: usize,
        _before_id: Option<Uuid>,
    ) -> Result<Vec<CachedMessage>, StorageError> {
        todo!()
    }

    async fn mark_message_state(
        &self,
        _message_id: Uuid,
        _edited_at: Option<i64>,
        _deleted_at: Option<i64>,
    ) -> Result<(), StorageError> {
        todo!()
    }

    async fn save_keypackage_local(&self, _kp: &LocalKeyPackage) -> Result<(), StorageError> {
        todo!()
    }

    async fn list_local_keypackages(&self) -> Result<Vec<LocalKeyPackage>, StorageError> {
        todo!()
    }

    async fn delete_local_keypackage(&self, _id: Uuid) -> Result<(), StorageError> {
        todo!()
    }

    async fn get_setting(&self, _key: &str) -> Result<Option<String>, StorageError> {
        todo!()
    }

    async fn set_setting(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        todo!()
    }

    async fn save_attachment_meta(&self, _att: &AttachmentMeta) -> Result<(), StorageError> {
        todo!()
    }

    async fn load_attachment_meta(
        &self,
        _attachment_id: Uuid,
    ) -> Result<Option<AttachmentMeta>, StorageError> {
        todo!()
    }
}

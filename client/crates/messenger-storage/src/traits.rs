//! Storage traits for secrets and local database.

use crate::{error::StorageError, types::*};
use async_trait::async_trait;
use uuid::Uuid;

/// Small secrets (16-256 bytes) protected by the OS.
#[async_trait(?Send)]
pub trait SecretStore {
    /// Retrieve a secret by key.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    /// Store a secret by key.
    async fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError>;
    /// Delete a secret by key.
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
}

/// Encrypted local database for chats, messages and state.
#[async_trait(?Send)]
pub trait LocalDatabase {
    /// Execute SQL with bind values (native only).
    async fn execute(&self, sql: &str, params: &[StorageValue]) -> Result<u64, StorageError>;
    /// Query SQL with bind values (native only).
    async fn query(&self, sql: &str, params: &[StorageValue]) -> Result<Vec<Row>, StorageError>;
    /// Close the database connection.
    async fn close(&self);
}

/// High-level messenger local store.
#[async_trait(?Send)]
pub trait MessengerLocalStore {
    // Identity
    /// Save encrypted identity for a user.
    async fn save_identity(
        &self,
        user_id: Uuid,
        identity: &EncryptedIdentity,
    ) -> Result<(), StorageError>;
    /// Load encrypted identity for a user.
    async fn load_identity(
        &self,
        user_id: Uuid,
    ) -> Result<Option<EncryptedIdentity>, StorageError>;

    // MLS group state
    /// Save MLS group state blob for a device.
    async fn save_mls_group_state(
        &self,
        device_id: Uuid,
        group_id: Uuid,
        state: &[u8],
    ) -> Result<(), StorageError>;
    /// Load MLS group state blob for a device.
    async fn load_mls_group_state(
        &self,
        device_id: Uuid,
        group_id: Uuid,
    ) -> Result<Option<Vec<u8>>, StorageError>;
    /// List all MLS group IDs for a device.
    async fn list_mls_group_ids(&self, device_id: Uuid) -> Result<Vec<Uuid>, StorageError>;

    // Chats / messages cache
    /// Save or update chat metadata.
    async fn save_chat_meta(&self, chat: &ChatMeta) -> Result<(), StorageError>;
    /// List all chats ordered by last message time.
    async fn list_chats(&self) -> Result<Vec<ChatMeta>, StorageError>;

    /// Save a message.
    async fn save_message(&self, msg: &CachedMessage) -> Result<(), StorageError>;
    /// List messages for a group.
    async fn list_messages(
        &self,
        group_id: Uuid,
        limit: usize,
        before_id: Option<Uuid>,
    ) -> Result<Vec<CachedMessage>, StorageError>;
    /// Mark message as edited or deleted.
    async fn mark_message_state(
        &self,
        message_id: Uuid,
        edited_at: Option<i64>,
        deleted_at: Option<i64>,
    ) -> Result<(), StorageError>;

    // KeyPackages pool tracking
    /// Save a local key package.
    async fn save_keypackage_local(&self, kp: &LocalKeyPackage) -> Result<(), StorageError>;
    /// List all local key packages.
    async fn list_local_keypackages(&self) -> Result<Vec<LocalKeyPackage>, StorageError>;
    /// Delete a local key package.
    async fn delete_local_keypackage(&self, id: Uuid) -> Result<(), StorageError>;

    // Settings (key-value, low-sensitivity)
    /// Get a setting value.
    async fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError>;
    /// Set a setting value.
    async fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError>;

    // Attachments
    /// Save attachment metadata.
    async fn save_attachment_meta(&self, att: &AttachmentMeta) -> Result<(), StorageError>;
    /// Load attachment metadata.
    async fn load_attachment_meta(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<AttachmentMeta>, StorageError>;
}

/// A single value for binding to SQL parameters.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageValue {
    /// SQL NULL.
    Null,
    /// Integer.
    Int(i64),
    /// Floating point.
    Real(f64),
    /// Text.
    Text(String),
    /// Binary blob.
    Blob(Vec<u8>),
}

/// A single row returned from a query.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Column name → value pairs.
    pub columns: Vec<(String, StorageValue)>,
}

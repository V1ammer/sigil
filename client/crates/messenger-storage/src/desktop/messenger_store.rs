//! High-level messenger store backed by SQLCipher.

use crate::{
    error::StorageError,
    traits::{LocalDatabase, MessengerLocalStore, Row, StorageValue},
    types::*,
};
use async_trait::async_trait;
use uuid::Uuid;

use super::database::SqlcipherDatabase;

/// Messenger store implementation using SQLCipher.
pub struct SqlcipherMessengerStore {
    db: SqlcipherDatabase,
}

impl SqlcipherMessengerStore {
    /// Wrap an existing SQLCipher database.
    pub fn new(db: SqlcipherDatabase) -> Self {
        Self { db }
    }

    /// Access the underlying database (for tests).
    pub fn db(&self) -> &SqlcipherDatabase {
        &self.db
    }
}

#[async_trait(?Send)]
impl MessengerLocalStore for SqlcipherMessengerStore {
    // ------------------------------------------------------------------
    // Identity
    // ------------------------------------------------------------------
    async fn save_identity(
        &self,
        user_id: Uuid,
        identity: &EncryptedIdentity,
    ) -> Result<(), StorageError> {
        self.db
            .execute(
                "INSERT INTO identity (
                    user_id, identity_secret_key_wrapped, identity_public_key,
                    device_signing_secret_key_wrapped, device_signing_public_key,
                    device_hpke_secret_key_wrapped, device_hpke_public_key
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(user_id) DO UPDATE SET
                    identity_secret_key_wrapped=excluded.identity_secret_key_wrapped,
                    identity_public_key=excluded.identity_public_key,
                    device_signing_secret_key_wrapped=excluded.device_signing_secret_key_wrapped,
                    device_signing_public_key=excluded.device_signing_public_key,
                    device_hpke_secret_key_wrapped=excluded.device_hpke_secret_key_wrapped,
                    device_hpke_public_key=excluded.device_hpke_public_key",
                &[
                    uuid_blob(user_id),
                    StorageValue::Blob(identity.identity_secret_key_wrapped.clone()),
                    StorageValue::Blob(identity.identity_public_key.clone()),
                    StorageValue::Blob(identity.device_signing_secret_key_wrapped.clone()),
                    StorageValue::Blob(identity.device_signing_public_key.clone()),
                    StorageValue::Blob(identity.device_hpke_secret_key_wrapped.clone()),
                    StorageValue::Blob(identity.device_hpke_public_key.clone()),
                ],
            )
            .await?;
        Ok(())
    }

    async fn load_identity(
        &self,
        user_id: Uuid,
    ) -> Result<Option<EncryptedIdentity>, StorageError> {
        let rows = self
            .db
            .query(
                "SELECT * FROM identity WHERE user_id = ?",
                &[uuid_blob(user_id)],
            )
            .await?;
        Ok(rows.into_iter().next().map(parse_identity_row).transpose()?)
    }

    // ------------------------------------------------------------------
    // MLS groups
    // ------------------------------------------------------------------
    async fn save_mls_group_state(
        &self,
        group_id: Uuid,
        state: &[u8],
    ) -> Result<(), StorageError> {
        let now = now_ms();
        self.db
            .execute(
                "INSERT INTO mls_groups (group_id, state_blob, updated_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT(group_id) DO UPDATE SET
                     state_blob=excluded.state_blob,
                     updated_at=excluded.updated_at",
                &[
                    uuid_blob(group_id),
                    StorageValue::Blob(state.to_vec()),
                    StorageValue::Int(now),
                ],
            )
            .await?;
        Ok(())
    }

    async fn load_mls_group_state(
        &self,
        group_id: Uuid,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let rows = self
            .db
            .query(
                "SELECT state_blob FROM mls_groups WHERE group_id = ?",
                &[uuid_blob(group_id)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|r| r.columns.into_iter().next())
            .and_then(|(_, v)| match v {
                StorageValue::Blob(b) => Some(b),
                _ => None,
            }))
    }

    async fn list_mls_group_ids(&self) -> Result<Vec<Uuid>, StorageError> {
        let rows = self
            .db
            .query("SELECT group_id FROM mls_groups", &[])
            .await?;
        rows.into_iter()
            .map(|r| {
                let (_, v) = r
                    .columns
                    .into_iter()
                    .next()
                    .ok_or_else(|| StorageError::Database("empty row".into()))?;
                match v {
                    StorageValue::Blob(b) => {
                        Uuid::from_slice(&b).map_err(|e| StorageError::Database(e.to_string()))
                    }
                    _ => Err(StorageError::Database("invalid group_id type".into())),
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Chats
    // ------------------------------------------------------------------
    async fn save_chat_meta(&self, chat: &ChatMeta) -> Result<(), StorageError> {
        self.db
            .execute(
                "INSERT INTO chats (
                    group_id, chat_type, display_name, avatar_blob,
                    last_message_at, unread_count, archived, pinned,
                    mute_until, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(group_id) DO UPDATE SET
                    chat_type=excluded.chat_type,
                    display_name=excluded.display_name,
                    avatar_blob=excluded.avatar_blob,
                    last_message_at=excluded.last_message_at,
                    unread_count=excluded.unread_count,
                    archived=excluded.archived,
                    pinned=excluded.pinned,
                    mute_until=excluded.mute_until,
                    updated_at=excluded.updated_at",
                &[
                    uuid_blob(chat.group_id),
                    StorageValue::Text(chat.chat_type.clone()),
                    chat.display_name.clone().map(StorageValue::Text).unwrap_or(StorageValue::Null),
                    chat.avatar_blob.clone().map(StorageValue::Blob).unwrap_or(StorageValue::Null),
                    chat.last_message_at.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    StorageValue::Int(chat.unread_count),
                    StorageValue::Int(i64::from(chat.archived)),
                    StorageValue::Int(i64::from(chat.pinned)),
                    chat.mute_until.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    StorageValue::Int(chat.updated_at),
                ],
            )
            .await?;
        Ok(())
    }

    async fn list_chats(&self) -> Result<Vec<ChatMeta>, StorageError> {
        let rows = self
            .db
            .query(
                "SELECT * FROM chats ORDER BY last_message_at DESC",
                &[],
            )
            .await?;
        rows.into_iter().map(parse_chat_row).collect()
    }

    // ------------------------------------------------------------------
    // Messages
    // ------------------------------------------------------------------
    async fn save_message(&self, msg: &CachedMessage) -> Result<(), StorageError> {
        self.db
            .execute(
                "INSERT INTO messages (
                    id, group_id, sender_user_id, sender_device_id,
                    wire_format, ciphertext, plaintext, content_type,
                    reply_to_message_id, thread_root_id,
                    edited_at, deleted_at, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    ciphertext=excluded.ciphertext,
                    plaintext=excluded.plaintext,
                    content_type=excluded.content_type,
                    edited_at=excluded.edited_at,
                    deleted_at=excluded.deleted_at",
                &[
                    uuid_blob(msg.id),
                    uuid_blob(msg.group_id),
                    uuid_blob(msg.sender_user_id),
                    uuid_blob(msg.sender_device_id),
                    StorageValue::Text(msg.wire_format.clone()),
                    StorageValue::Blob(msg.ciphertext.clone()),
                    msg.plaintext.clone().map(StorageValue::Blob).unwrap_or(StorageValue::Null),
                    msg.content_type.clone().map(StorageValue::Text).unwrap_or(StorageValue::Null),
                    msg.reply_to_message_id.map(uuid_blob).unwrap_or(StorageValue::Null),
                    msg.thread_root_id.map(uuid_blob).unwrap_or(StorageValue::Null),
                    msg.edited_at.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    msg.deleted_at.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    StorageValue::Int(msg.created_at),
                ],
            )
            .await?;
        Ok(())
    }

    async fn list_messages(
        &self,
        group_id: Uuid,
        limit: usize,
        before_id: Option<Uuid>,
    ) -> Result<Vec<CachedMessage>, StorageError> {
        let sql = if before_id.is_some() {
            "SELECT * FROM messages
             WHERE group_id = ? AND id < ?
             ORDER BY created_at DESC, id DESC
             LIMIT ?"
        } else {
            "SELECT * FROM messages
             WHERE group_id = ?
             ORDER BY created_at DESC, id DESC
             LIMIT ?"
        };
        let mut params: Vec<StorageValue> = vec![uuid_blob(group_id)];
        if let Some(bid) = before_id {
            params.push(uuid_blob(bid));
        }
        params.push(StorageValue::Int(limit as i64));

        let rows = self.db.query(sql, &params).await?;
        rows.into_iter().map(parse_message_row).collect()
    }

    async fn mark_message_state(
        &self,
        message_id: Uuid,
        edited_at: Option<i64>,
        deleted_at: Option<i64>,
    ) -> Result<(), StorageError> {
        self.db
            .execute(
                "UPDATE messages SET edited_at = ?, deleted_at = ? WHERE id = ?",
                &[
                    edited_at.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    deleted_at.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    uuid_blob(message_id),
                ],
            )
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // KeyPackages
    // ------------------------------------------------------------------
    async fn save_keypackage_local(&self, kp: &LocalKeyPackage) -> Result<(), StorageError> {
        self.db
            .execute(
                "INSERT INTO keypackages_local (
                    id, init_key_hash, secret_keys_wrapped,
                    expires_at, is_last_resort, published, consumed, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    init_key_hash=excluded.init_key_hash,
                    secret_keys_wrapped=excluded.secret_keys_wrapped,
                    expires_at=excluded.expires_at,
                    is_last_resort=excluded.is_last_resort,
                    published=excluded.published,
                    consumed=excluded.consumed",
                &[
                    uuid_blob(kp.id),
                    StorageValue::Blob(kp.init_key_hash.clone()),
                    StorageValue::Blob(kp.secret_keys_wrapped.clone()),
                    StorageValue::Int(kp.expires_at),
                    StorageValue::Int(i64::from(kp.is_last_resort)),
                    StorageValue::Int(i64::from(kp.published)),
                    StorageValue::Int(i64::from(kp.consumed)),
                    StorageValue::Int(kp.created_at),
                ],
            )
            .await?;
        Ok(())
    }

    async fn list_local_keypackages(&self) -> Result<Vec<LocalKeyPackage>, StorageError> {
        let rows = self
            .db
            .query("SELECT * FROM keypackages_local", &[])
            .await?;
        rows.into_iter().map(parse_keypackage_row).collect()
    }

    async fn delete_local_keypackage(&self, id: Uuid) -> Result<(), StorageError> {
        self.db
            .execute(
                "DELETE FROM keypackages_local WHERE id = ?",
                &[uuid_blob(id)],
            )
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Settings
    // ------------------------------------------------------------------
    async fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        let rows = self
            .db
            .query("SELECT value FROM settings WHERE key = ?", &[StorageValue::Text(key.to_string())])
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|r| r.columns.into_iter().next())
            .and_then(|(_, v)| match v {
                StorageValue::Text(s) => Some(s),
                _ => None,
            }))
    }

    async fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.db
            .execute(
                "INSERT INTO settings (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                &[
                    StorageValue::Text(key.to_string()),
                    StorageValue::Text(value.to_string()),
                ],
            )
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Attachments
    // ------------------------------------------------------------------
    async fn save_attachment_meta(&self, att: &AttachmentMeta) -> Result<(), StorageError> {
        self.db
            .execute(
                "INSERT INTO attachments_meta (
                    attachment_id, message_id, decryption_key_wrapped,
                    mime, display_filename, padded_size, real_size, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(attachment_id) DO UPDATE SET
                    message_id=excluded.message_id,
                    decryption_key_wrapped=excluded.decryption_key_wrapped,
                    mime=excluded.mime,
                    display_filename=excluded.display_filename,
                    padded_size=excluded.padded_size,
                    real_size=excluded.real_size",
                &[
                    uuid_blob(att.attachment_id),
                    att.message_id.map(uuid_blob).unwrap_or(StorageValue::Null),
                    StorageValue::Blob(att.decryption_key_wrapped.clone()),
                    att.mime.clone().map(StorageValue::Text).unwrap_or(StorageValue::Null),
                    att.display_filename.clone().map(StorageValue::Text).unwrap_or(StorageValue::Null),
                    att.padded_size.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    att.real_size.map(StorageValue::Int).unwrap_or(StorageValue::Null),
                    StorageValue::Int(att.created_at),
                ],
            )
            .await?;
        Ok(())
    }

    async fn load_attachment_meta(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<AttachmentMeta>, StorageError> {
        let rows = self
            .db
            .query(
                "SELECT * FROM attachments_meta WHERE attachment_id = ?",
                &[uuid_blob(attachment_id)],
            )
            .await?;
        Ok(rows.into_iter().next().map(parse_attachment_row).transpose()?)
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn uuid_blob(u: Uuid) -> StorageValue {
    StorageValue::Blob(u.as_bytes().to_vec())
}

fn parse_identity_row(row: Row) -> Result<EncryptedIdentity, StorageError> {
    let mut cols: std::collections::HashMap<String, StorageValue> =
        row.columns.into_iter().collect();
    Ok(EncryptedIdentity {
        identity_secret_key_wrapped: take_blob(&mut cols, "identity_secret_key_wrapped")?,
        identity_public_key: take_blob(&mut cols, "identity_public_key")?,
        device_signing_secret_key_wrapped: take_blob(&mut cols, "device_signing_secret_key_wrapped")?,
        device_signing_public_key: take_blob(&mut cols, "device_signing_public_key")?,
        device_hpke_secret_key_wrapped: take_blob(&mut cols, "device_hpke_secret_key_wrapped")?,
        device_hpke_public_key: take_blob(&mut cols, "device_hpke_public_key")?,
    })
}

fn parse_chat_row(row: Row) -> Result<ChatMeta, StorageError> {
    let mut cols: std::collections::HashMap<String, StorageValue> =
        row.columns.into_iter().collect();
    Ok(ChatMeta {
        group_id: take_uuid(&mut cols, "group_id")?,
        chat_type: take_text(&mut cols, "chat_type")?,
        display_name: take_opt_text(&mut cols, "display_name"),
        avatar_blob: take_opt_blob(&mut cols, "avatar_blob"),
        last_message_at: take_opt_int(&mut cols, "last_message_at"),
        unread_count: take_int(&mut cols, "unread_count")?,
        archived: take_int(&mut cols, "archived")? != 0,
        pinned: take_int(&mut cols, "pinned")? != 0,
        mute_until: take_opt_int(&mut cols, "mute_until"),
        updated_at: take_int(&mut cols, "updated_at")?,
    })
}

fn parse_message_row(row: Row) -> Result<CachedMessage, StorageError> {
    let mut cols: std::collections::HashMap<String, StorageValue> =
        row.columns.into_iter().collect();
    Ok(CachedMessage {
        id: take_uuid(&mut cols, "id")?,
        group_id: take_uuid(&mut cols, "group_id")?,
        sender_user_id: take_uuid(&mut cols, "sender_user_id")?,
        sender_device_id: take_uuid(&mut cols, "sender_device_id")?,
        wire_format: take_text(&mut cols, "wire_format")?,
        ciphertext: take_blob(&mut cols, "ciphertext")?,
        plaintext: take_opt_blob(&mut cols, "plaintext"),
        content_type: take_opt_text(&mut cols, "content_type"),
        reply_to_message_id: take_opt_uuid(&mut cols, "reply_to_message_id"),
        thread_root_id: take_opt_uuid(&mut cols, "thread_root_id"),
        edited_at: take_opt_int(&mut cols, "edited_at"),
        deleted_at: take_opt_int(&mut cols, "deleted_at"),
        created_at: take_int(&mut cols, "created_at")?,
    })
}

fn parse_keypackage_row(row: Row) -> Result<LocalKeyPackage, StorageError> {
    let mut cols: std::collections::HashMap<String, StorageValue> =
        row.columns.into_iter().collect();
    Ok(LocalKeyPackage {
        id: take_uuid(&mut cols, "id")?,
        init_key_hash: take_blob(&mut cols, "init_key_hash")?,
        secret_keys_wrapped: take_blob(&mut cols, "secret_keys_wrapped")?,
        expires_at: take_int(&mut cols, "expires_at")?,
        is_last_resort: take_int(&mut cols, "is_last_resort")? != 0,
        published: take_int(&mut cols, "published")? != 0,
        consumed: take_int(&mut cols, "consumed")? != 0,
        created_at: take_int(&mut cols, "created_at")?,
    })
}

fn parse_attachment_row(row: Row) -> Result<AttachmentMeta, StorageError> {
    let mut cols: std::collections::HashMap<String, StorageValue> =
        row.columns.into_iter().collect();
    Ok(AttachmentMeta {
        attachment_id: take_uuid(&mut cols, "attachment_id")?,
        message_id: take_opt_uuid(&mut cols, "message_id"),
        decryption_key_wrapped: take_blob(&mut cols, "decryption_key_wrapped")?,
        mime: take_opt_text(&mut cols, "mime"),
        display_filename: take_opt_text(&mut cols, "display_filename"),
        padded_size: take_opt_int(&mut cols, "padded_size"),
        real_size: take_opt_int(&mut cols, "real_size"),
        created_at: take_int(&mut cols, "created_at")?,
    })
}

// -- column extraction helpers -------------------------------------

fn take_blob(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Result<Vec<u8>, StorageError> {
    match cols.remove(key) {
        Some(StorageValue::Blob(b)) => Ok(b),
        _ => Err(StorageError::Database(format!("missing blob col {key}"))),
    }
}

fn take_text(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Result<String, StorageError> {
    match cols.remove(key) {
        Some(StorageValue::Text(t)) => Ok(t),
        _ => Err(StorageError::Database(format!("missing text col {key}"))),
    }
}

fn take_int(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Result<i64, StorageError> {
    match cols.remove(key) {
        Some(StorageValue::Int(i)) => Ok(i),
        _ => Err(StorageError::Database(format!("missing int col {key}"))),
    }
}

fn take_uuid(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Result<Uuid, StorageError> {
    match cols.remove(key) {
        Some(StorageValue::Blob(b)) => {
            Uuid::from_slice(&b).map_err(|e| StorageError::Database(e.to_string()))
        }
        _ => Err(StorageError::Database(format!("missing uuid col {key}"))),
    }
}

fn take_opt_blob(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Option<Vec<u8>> {
    match cols.remove(key) {
        Some(StorageValue::Blob(b)) => Some(b),
        _ => None,
    }
}

fn take_opt_text(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Option<String> {
    match cols.remove(key) {
        Some(StorageValue::Text(t)) => Some(t),
        _ => None,
    }
}

fn take_opt_int(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Option<i64> {
    match cols.remove(key) {
        Some(StorageValue::Int(i)) => Some(i),
        _ => None,
    }
}

fn take_opt_uuid(
    cols: &mut std::collections::HashMap<String, StorageValue>,
    key: &str,
) -> Option<Uuid> {
    match cols.remove(key) {
        Some(StorageValue::Blob(b)) => Uuid::from_slice(&b).ok(),
        _ => None,
    }
}

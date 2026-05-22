//! Data types used by the high-level storage API.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Encrypted identity key material stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedIdentity {
    /// Wrapped identity secret key.
    pub identity_secret_key_wrapped: Vec<u8>,
    /// Identity public key.
    pub identity_public_key: Vec<u8>,
    /// Wrapped device signing secret key.
    pub device_signing_secret_key_wrapped: Vec<u8>,
    /// Device signing public key.
    pub device_signing_public_key: Vec<u8>,
    /// Wrapped device HPKE secret key.
    pub device_hpke_secret_key_wrapped: Vec<u8>,
    /// Device HPKE public key.
    pub device_hpke_public_key: Vec<u8>,
}

/// Chat metadata cached locally for UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMeta {
    /// MLS group ID.
    pub group_id: Uuid,
    /// Chat type: "direct" or "group".
    pub chat_type: String,
    /// Display name (cached, may be encrypted on server).
    pub display_name: Option<String>,
    /// Avatar blob (cached).
    pub avatar_blob: Option<Vec<u8>>,
    /// Timestamp of last message (ms since epoch).
    pub last_message_at: Option<i64>,
    /// Number of unread messages.
    pub unread_count: i64,
    /// Whether the chat is archived.
    pub archived: bool,
    /// Whether the chat is pinned.
    pub pinned: bool,
    /// Mute until timestamp (ms since epoch).
    pub mute_until: Option<i64>,
    /// Last update timestamp (ms since epoch).
    pub updated_at: i64,
}

/// A cached message stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMessage {
    /// Message ID.
    pub id: Uuid,
    /// MLS group ID.
    pub group_id: Uuid,
    /// Sender user ID.
    pub sender_user_id: Uuid,
    /// Sender device ID.
    pub sender_device_id: Uuid,
    /// Wire format type.
    pub wire_format: String,
    /// Ciphertext payload.
    pub ciphertext: Vec<u8>,
    /// Decrypted plaintext (if cached).
    pub plaintext: Option<Vec<u8>>,
    /// Content type hint.
    pub content_type: Option<String>,
    /// ID of message this replies to.
    pub reply_to_message_id: Option<Uuid>,
    /// Thread root message ID.
    pub thread_root_id: Option<Uuid>,
    /// Edit timestamp (ms since epoch).
    pub edited_at: Option<i64>,
    /// Delete timestamp (ms since epoch).
    pub deleted_at: Option<i64>,
    /// Creation timestamp (ms since epoch).
    pub created_at: i64,
}

/// Local key package tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalKeyPackage {
    /// Key package ID.
    pub id: Uuid,
    /// Hash of the init key.
    pub init_key_hash: Vec<u8>,
    /// Wrapped secret keys.
    pub secret_keys_wrapped: Vec<u8>,
    /// Expiration timestamp (ms since epoch).
    pub expires_at: i64,
    /// Whether this is a last-resort key package.
    pub is_last_resort: bool,
    /// Whether published to server.
    pub published: bool,
    /// Whether consumed by a group join.
    pub consumed: bool,
    /// Creation timestamp (ms since epoch).
    pub created_at: i64,
}

/// Attachment metadata stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// Attachment ID.
    pub attachment_id: Uuid,
    /// Parent message ID.
    pub message_id: Option<Uuid>,
    /// Wrapped decryption key.
    pub decryption_key_wrapped: Vec<u8>,
    /// MIME type.
    pub mime: Option<String>,
    /// Display filename.
    pub display_filename: Option<String>,
    /// Padded size on server.
    pub padded_size: Option<i64>,
    /// Real size after decryption.
    pub real_size: Option<i64>,
    /// Creation timestamp (ms since epoch).
    pub created_at: i64,
}

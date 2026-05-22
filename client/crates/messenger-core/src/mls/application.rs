//! Application message envelope — plaintext inside MLS application messages.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Application message structure — encrypted via MLS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationEnvelope {
    /// Client-generated message ID.
    pub client_message_id: Uuid,
    /// Message kind.
    pub kind: AppMessageKind,
    /// Message body.
    pub body: AppMessageBody,
    /// Reply-to message ID.
    pub reply_to_message_id: Option<Uuid>,
    /// Thread root message ID.
    pub thread_root_id: Option<Uuid>,
    /// Creation timestamp (seconds since epoch).
    pub created_at: i64,
    /// Optional display name override.
    pub sender_display_name_override: Option<String>,
}

/// Kind of application message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppMessageKind {
    /// Plain text message.
    Text,
    /// Voice message.
    Voice,
    /// File attachment.
    File,
    /// Image attachment.
    Image,
    /// System event (join/leave/etc).
    SystemNote,
    /// Read receipt.
    ReadReceipt,
    /// Edit notice.
    EditNotice,
    /// Delete notice.
    DeleteNotice,
    /// Avatar update.
    AvatarUpdate,
    /// Username update.
    UsernameUpdate,
    /// Reaction.
    Reaction,
}

/// Body of an application message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AppMessageBody {
    /// Text message.
    Text {
        /// Plain text content.
        text: String,
        /// Optional HTML formatting.
        formatted_html: Option<String>,
    },
    /// Voice message.
    Voice {
        /// Attachment ID.
        attachment_id: Uuid,
        /// Decryption key.
        decryption_key: Vec<u8>,
        /// Duration in milliseconds.
        duration_ms: u32,
        /// Waveform data.
        waveform: Vec<u8>,
    },
    /// File attachment.
    File {
        /// Attachment ID.
        attachment_id: Uuid,
        /// Decryption key.
        decryption_key: Vec<u8>,
        /// MIME type.
        mime: String,
        /// Filename.
        filename: String,
        /// File size.
        size: u64,
    },
    /// Image attachment.
    Image {
        /// Attachment ID.
        attachment_id: Uuid,
        /// Decryption key.
        decryption_key: Vec<u8>,
        /// MIME type.
        mime: String,
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Thumbnail data.
        thumb: Option<Vec<u8>>,
    },
    /// System event.
    SystemNote {
        /// Event code.
        code: String,
        /// Event parameters.
        params: HashMap<String, String>,
    },
    /// Read receipt.
    ReadReceipt {
        /// Up-to message ID.
        up_to_message_id: Uuid,
        /// Timestamp.
        at: i64,
    },
    /// Edit notice.
    EditNotice {
        /// Original message ID.
        original_message_id: Uuid,
        /// New text.
        new_text: String,
    },
    /// Delete notice.
    DeleteNotice {
        /// Target message ID.
        target_message_id: Uuid,
    },
    /// Avatar update.
    AvatarUpdate {
        /// Avatar blob ID.
        avatar_blob_id: Option<Uuid>,
    },
    /// Username update.
    UsernameUpdate {
        /// New username.
        new_username: String,
    },
    /// Reaction.
    Reaction {
        /// Target message ID.
        target_message_id: Uuid,
        /// Emoji.
        emoji: String,
        /// Add or remove.
        action: ReactionAction,
    },
}

/// Action for a reaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReactionAction {
    /// Add reaction.
    Add,
    /// Remove reaction.
    Remove,
}

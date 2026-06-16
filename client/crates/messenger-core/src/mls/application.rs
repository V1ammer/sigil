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
    /// Group metadata update (name / avatar).
    GroupUpdate,
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
        /// Optional caption typed alongside the attachment.
        #[serde(default)]
        caption: Option<String>,
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
        /// Optional poster thumbnail (JPEG) — used for video attachments so the
        /// bubble shows a frame instead of a placeholder. `default` keeps wire
        /// compatibility with messages sent before this field existed.
        #[serde(default)]
        thumb: Option<Vec<u8>>,
        /// Optional caption typed alongside the attachment.
        #[serde(default)]
        caption: Option<String>,
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
        /// Optional caption typed alongside the attachment.
        #[serde(default)]
        caption: Option<String>,
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
    /// Avatar update — mirrors `Image`: the picture lives in the encrypted
    /// attachment store, only the reference + key travel inside MLS.
    AvatarUpdate {
        /// Encrypted blob in /v1/attachments; `None` = avatar removed.
        avatar_blob_id: Option<Uuid>,
        /// Symmetric key for the blob (empty when `avatar_blob_id` is None).
        decryption_key: Vec<u8>,
        /// MIME type of the decrypted image.
        mime: String,
    },
    /// Group metadata update — name and/or avatar, end-to-end like
    /// `AvatarUpdate`. Sent by the owner; the picture lives in the encrypted
    /// attachment store, only the reference + key travel inside MLS. A `None`
    /// field means "unchanged" except `avatar_blob_id: None` with a non-empty
    /// update is an explicit avatar removal.
    GroupUpdate {
        /// New group name, `None` if this update only touches the avatar.
        name: Option<String>,
        /// Encrypted avatar blob id; `None` = no avatar / removed.
        avatar_blob_id: Option<Uuid>,
        /// Symmetric key for the blob (empty when `avatar_blob_id` is None).
        decryption_key: Vec<u8>,
        /// MIME type of the decrypted image.
        mime: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_update_roundtrip() {
        let blob_id = Uuid::now_v7();
        let envelope = ApplicationEnvelope {
            client_message_id: Uuid::now_v7(),
            kind: AppMessageKind::AvatarUpdate,
            body: AppMessageBody::AvatarUpdate {
                avatar_blob_id: Some(blob_id),
                decryption_key: vec![7u8; 32],
                mime: "image/jpeg".to_string(),
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: 1_750_000_000,
            sender_display_name_override: Some("alice".to_string()),
        };
        let bytes = rmp_serde::to_vec(&envelope).expect("serialize");
        let parsed: ApplicationEnvelope = rmp_serde::from_slice(&bytes).expect("deserialize");
        match parsed.body {
            AppMessageBody::AvatarUpdate { avatar_blob_id, decryption_key, mime } => {
                assert_eq!(avatar_blob_id, Some(blob_id));
                assert_eq!(decryption_key, vec![7u8; 32]);
                assert_eq!(mime, "image/jpeg");
            }
            other => panic!("wrong body variant: {other:?}"),
        }
    }

    #[test]
    fn avatar_removed_roundtrip() {
        let envelope = ApplicationEnvelope {
            client_message_id: Uuid::now_v7(),
            kind: AppMessageKind::AvatarUpdate,
            body: AppMessageBody::AvatarUpdate {
                avatar_blob_id: None,
                decryption_key: Vec::new(),
                mime: String::new(),
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: 0,
            sender_display_name_override: None,
        };
        let bytes = rmp_serde::to_vec(&envelope).expect("serialize");
        let parsed: ApplicationEnvelope = rmp_serde::from_slice(&bytes).expect("deserialize");
        assert!(matches!(
            parsed.body,
            AppMessageBody::AvatarUpdate { avatar_blob_id: None, .. }
        ));
    }
}

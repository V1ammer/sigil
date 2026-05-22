//! Per-group message buffers.

use std::collections::HashMap;
use leptos::prelude::*;
use uuid::Uuid;

/// Delivery status of an outgoing message.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeliveryStatus {
    Sending,
    SentToServer,
    DeliveredToAll,
    Failed,
}

/// High-level message kind used for UI rendering.
#[derive(Clone, Debug)]
pub enum MessageKind {
    Text,
    Voice,
    Image,
    Video,
    File,
    System,
}

/// Decrypted message body.
#[derive(Clone, Debug)]
pub enum MessageBody {
    Text(String),
    Voice {
        duration_secs: u32,
        waveform: Vec<u8>,
        transcription: Option<String>,
    },
    Image {
        thumbnail: Vec<u8>,
        full: Vec<u8>,
    },
    Video {
        thumbnail: Vec<u8>,
    },
    File {
        name: String,
        size: u64,
    },
    System {
        action: String,
    },
}

/// A single reaction on a message.
#[derive(Clone, Debug)]
pub struct DisplayReaction {
    pub emoji: String,
    pub count: u32,
    pub has_own: bool,
}

/// A message ready for the UI layer — fully decrypted and hydrated.
#[derive(Clone, Debug)]
pub struct DisplayMessage {
    pub id: Uuid,
    pub client_message_id: Uuid,
    pub group_id: Uuid,
    pub sender_user_id: Uuid,
    pub sender_device_id: Uuid,
    pub kind: MessageKind,
    pub body: MessageBody,
    pub reply_to_message_id: Option<Uuid>,
    pub thread_root_id: Option<Uuid>,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub delivery_status: DeliveryStatus,
    pub reactions: Vec<DisplayReaction>,
}

/// Per-group message buffers.
///
/// Messages are stored keyed by `group_id`. Each group's messages are held in
/// insertion order (oldest first).
#[derive(Clone)]
pub struct MessagesState {
    pub by_group: RwSignal<HashMap<Uuid, Vec<DisplayMessage>>>,
}

impl MessagesState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_group: RwSignal::new(HashMap::new()),
        }
    }

    /// Returns a derived signal that yields the messages for `group_id`.
    pub fn for_group(&self, group_id: Uuid) -> impl Fn() -> Vec<DisplayMessage> + 'static {
        let by = self.by_group;
        move || by.get().get(&group_id).cloned().unwrap_or_default()
    }
}

impl Default for MessagesState {
    fn default() -> Self {
        Self::new()
    }
}

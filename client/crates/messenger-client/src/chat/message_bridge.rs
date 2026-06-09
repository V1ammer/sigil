//! Bridge between `DisplayMessage` (store) and `mock::Message` (UI).
//!
//! The existing `MessageList`, `MessageItem`, and `InputBar` components all
//! use `mock::Message`.  This module converts our real message data into
//! the mock format so we can reuse those components until they are refactored.

use crate::mock;
use crate::state::messages::{DeliveryStatus, DisplayMessage, MessageBody, MessageKind};

/// Convert a `DisplayMessage` into a `mock::Message` for UI rendering.
pub fn display_to_mock(msg: &DisplayMessage) -> mock::Message {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;

    let mut media_attachment_id: Option<String> = None;
    let mut media_decryption_key: Option<String> = None;
    let mut media_mime: Option<String> = None;

    let (msg_type, content, duration, waveform, transcription, file_name, file_size) =
        match &msg.body {
            MessageBody::Text(t) => ("text".to_string(), t.clone(), None, vec![], None, None, None),
            MessageBody::Voice {
                attachment_id,
                decryption_key,
                duration_ms,
                waveform: wf,
                transcription: tr,
            } => {
                media_attachment_id = Some(attachment_id.to_string());
                media_decryption_key = Some(b64.encode(decryption_key));
                media_mime = Some("audio/webm;codecs=opus".to_string());
                (
                    "voice".to_string(),
                    String::new(),
                    Some(*duration_ms / 1000),
                    wf.iter().map(|&b| f64::from(b) / 255.0).collect(),
                    tr.clone(),
                    None,
                    None,
                )
            }
            MessageBody::Image {
                attachment_id,
                decryption_key,
                mime,
                ..
            } => {
                media_attachment_id = Some(attachment_id.to_string());
                media_decryption_key = Some(b64.encode(decryption_key));
                media_mime = Some(mime.clone());
                ("image".to_string(), String::new(), None, vec![], None, None, None)
            }
            MessageBody::File {
                attachment_id,
                decryption_key,
                mime,
                name,
                size,
            } => {
                media_attachment_id = Some(attachment_id.to_string());
                media_decryption_key = Some(b64.encode(decryption_key));
                media_mime = Some(mime.clone());
                (
                    "file".to_string(),
                    String::new(),
                    None,
                    vec![],
                    None,
                    Some(name.clone()),
                    Some(*size),
                )
            }
            MessageBody::System { action } => {
                ("system".to_string(), action.clone(), None, vec![], None, None, None)
            }
        };

    let status = match msg.delivery_status {
        DeliveryStatus::Sending => "sending",
        DeliveryStatus::SentToServer => "sent",
        DeliveryStatus::DeliveredToAll => "delivered",
        DeliveryStatus::Failed => "sent", // fallback
    };

    mock::Message {
        id: msg.id.to_string(),
        chat_id: msg.group_id.to_string(),
        sender_id: msg.sender_user_id.to_string(),
        sender_name: msg.sender_display_name.clone().unwrap_or_default(),
        sender_avatar: None,
        msg_type,
        content,
        timestamp: msg.created_at as f64,
        status: status.to_string(),
        is_own: true, // will be adjusted by the caller
        is_edited: msg.edited_at.is_some(),
        is_deleted: msg.deleted_at.is_some(),
        reply_to: None,
        reactions: msg
            .reactions
            .iter()
            .map(|r| mock::Reaction {
                emoji: r.emoji.clone(),
                count: r.count,
                users: Vec::new(),
                has_own: r.has_own,
            })
            .collect(),
        thread_count: None,
        duration,
        waveform,
        transcription,
        media_url: None,
        thumbnail_url: None,
        file_name,
        file_size,
        mime_type: media_mime,
        attachment_id: media_attachment_id,
        decryption_key: media_decryption_key,
        system_action: None,
    }
}

/// Convert a slice of `DisplayMessage`s into `mock::Message`s.
pub fn display_vec_to_mock(msgs: &[DisplayMessage], own_user_id: &str) -> Vec<mock::Message> {
    use leptos::prelude::use_context;
    let users = use_context::<crate::state::users::UsersState>();
    msgs.iter()
        .map(|m| {
            let mut mock = display_to_mock(m);
            // Mark as own if the sender is the current user.
            let is_own = m.sender_user_id.to_string() == own_user_id
                || m.sender_user_id == uuid::Uuid::nil();
            mock.is_own = is_own;
            mock.sender_id = m.sender_user_id.to_string();
            // Fill in a display name when the envelope didn't carry one:
            // fall back to the users cache, then to a short id.
            if mock.sender_name.is_empty() {
                if let Some(ref users) = users {
                    mock.sender_name = users.label_for(m.sender_user_id);
                } else if !is_own {
                    mock.sender_name = m
                        .sender_user_id
                        .to_string()
                        .chars()
                        .take(8)
                        .collect::<String>()
                        + "…";
                }
            }
            mock
        })
        .collect()
}

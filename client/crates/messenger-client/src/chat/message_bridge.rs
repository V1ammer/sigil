//! Bridge between `DisplayMessage` (store) and `mock::Message` (UI).
//!
//! The existing `MessageList`, `MessageItem`, and `InputBar` components all
//! use `mock::Message`.  This module converts our real message data into
//! the mock format so we can reuse those components until they are refactored.

use crate::mock;
use crate::state::messages::{DeliveryStatus, DisplayMessage, MessageBody};

/// Convert a `DisplayMessage` into a `mock::Message` for UI rendering.
pub fn display_to_mock(msg: &DisplayMessage) -> mock::Message {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;

    let mut media_attachment_id: Option<String> = None;
    let mut media_decryption_key: Option<String> = None;
    let mut media_mime: Option<String> = None;
    let mut media_thumb_url: Option<String> = None;

    let (msg_type, content, duration, waveform, transcription, file_name, file_size) =
        match &msg.body {
            MessageBody::Text(t) => ("text".to_string(), t.clone(), None, vec![], None, None, None),
            MessageBody::Voice {
                attachment_id,
                decryption_key,
                duration_ms,
                waveform: wf,
                transcription: tr,
                caption,
            } => {
                media_attachment_id = Some(attachment_id.to_string());
                media_decryption_key = Some(b64.encode(decryption_key));
                media_mime = Some("audio/webm;codecs=opus".to_string());
                (
                    "voice".to_string(),
                    caption.clone().unwrap_or_default(),
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
                caption,
                ..
            } => {
                media_attachment_id = Some(attachment_id.to_string());
                media_decryption_key = Some(b64.encode(decryption_key));
                media_mime = Some(mime.clone());
                // The caption rides along as the bubble's text so the media and
                // its caption render as one message.
                ("image".to_string(), caption.clone().unwrap_or_default(), None, vec![], None, None, None)
            }
            MessageBody::File {
                attachment_id,
                decryption_key,
                mime,
                name,
                size,
                thumb,
                caption,
            } => {
                media_attachment_id = Some(attachment_id.to_string());
                media_decryption_key = Some(b64.encode(decryption_key));
                media_mime = Some(mime.clone());
                // Video poster (if any) → a data URL the bubble shows immediately.
                if let Some(t) = thumb {
                    media_thumb_url = Some(format!("data:image/jpeg;base64,{}", b64.encode(t)));
                }
                // Video/audio files get an inline player instead of a download row.
                let kind = if mime.starts_with("video/") {
                    "video"
                } else if mime.starts_with("audio/") {
                    "audio"
                } else {
                    "file"
                };
                (
                    kind.to_string(),
                    caption.clone().unwrap_or_default(),
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
        DeliveryStatus::Read => "read",
        DeliveryStatus::Failed => "failed",
    };

    mock::Message {
        id: msg.id.to_string(),
        chat_id: msg.group_id.to_string(),
        sender_id: msg.sender_user_id.to_string(),
        sender_name: msg.sender_display_name.clone().unwrap_or_default(),
        sender_avatar: None,
        msg_type,
        content,
        // created_at is in seconds; the UI formatters (format_time/format_date,
        // grouping windows) all expect milliseconds.
        timestamp: msg.created_at as f64 * 1000.0,
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
        thumbnail_url: media_thumb_url,
        file_name,
        file_size,
        mime_type: media_mime,
        attachment_id: media_attachment_id,
        decryption_key: media_decryption_key,
        system_action: None,
    }
}

/// Short quotable snippet of a message body for reply previews.
fn quote_snippet(body: &MessageBody) -> String {
    match body {
        MessageBody::Text(t) => {
            let trimmed = t.trim();
            if trimmed.chars().count() > 60 {
                trimmed.chars().take(60).collect::<String>() + "…"
            } else {
                trimmed.to_string()
            }
        }
        MessageBody::Voice { .. } => "🎤".to_string(),
        MessageBody::Image { .. } => "📷".to_string(),
        MessageBody::File { name, .. } => format!("📎 {name}"),
        MessageBody::System { action } => action.clone(),
    }
}

/// Convert a slice of `DisplayMessage`s into `mock::Message`s for the main
/// timeline: thread replies are folded into a counter badge on their root
/// Convert a single `DisplayMessage` with sender identity resolved — for the
/// thread panel, which renders messages standalone. The raw [`display_to_mock`]
/// hardcodes `is_own = true` (expecting the caller to fix it); without this the
/// thread showed every reply right-aligned with no name/avatar, so it was
/// impossible to tell who wrote what.
#[must_use]
pub fn display_to_mock_with_owner(m: &DisplayMessage, own_user_id: &str) -> mock::Message {
    use leptos::prelude::use_context;
    let users = use_context::<crate::state::users::UsersState>();
    let mut mock = display_to_mock(m);
    let is_own =
        m.sender_user_id.to_string() == own_user_id || m.sender_user_id == uuid::Uuid::nil();
    mock.is_own = is_own;
    mock.sender_id = m.sender_user_id.to_string();
    if mock.sender_name.is_empty() {
        if let Some(ref users) = users {
            mock.sender_name = users.label_for(m.sender_user_id);
        } else if !is_own {
            mock.sender_name =
                m.sender_user_id.to_string().chars().take(8).collect::<String>() + "…";
        }
    }
    mock
}

/// (Slack model) instead of appearing inline as ordinary messages.
pub fn display_vec_to_mock(msgs: &[DisplayMessage], own_user_id: &str) -> Vec<mock::Message> {
    use leptos::prelude::use_context;
    let users = use_context::<crate::state::users::UsersState>();
    let mut thread_counts: std::collections::HashMap<uuid::Uuid, u32> =
        std::collections::HashMap::new();
    for m in msgs {
        if let Some(root) = m.thread_root_id {
            *thread_counts.entry(root).or_default() += 1;
        }
    }
    msgs.iter()
        .filter(|m| m.thread_root_id.is_none())
        .map(|m| {
            let mut mock = display_to_mock(m);
            mock.thread_count = thread_counts.get(&m.id).copied().filter(|&c| c > 0);
            // Resolve the reply quote from the same batch — the bridge used
            // to drop reply_to entirely, so replies looked like plain texts.
            if let Some(orig_id) = m.reply_to_message_id {
                if let Some(orig) = msgs.iter().find(|o| o.id == orig_id) {
                    let sender_name = orig
                        .sender_display_name
                        .clone()
                        .or_else(|| {
                            users.as_ref().and_then(|u| u.get(orig.sender_user_id))
                        })
                        .unwrap_or_default();
                    mock.reply_to = Some(Box::new(mock::ReplyTo {
                        id: orig.id.to_string(),
                        sender_name,
                        content: quote_snippet(&orig.body),
                    }));
                }
            }
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

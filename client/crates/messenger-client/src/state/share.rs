//! Incoming-share state.
//!
//! Files the user shared into the app via the Android "Share" sheet (gallery →
//! Share → this app). They land here as ready-to-stage payloads; the chat
//! screen stages the first one into the composer once the user picks a chat.

use leptos::prelude::*;

use crate::chat::input_bar::{AttachmentKind, AttachmentPayload};

#[derive(Clone, Copy)]
pub struct ShareState {
    /// Shared files waiting to be staged into a chat.
    pub pending: RwSignal<Vec<AttachmentPayload>>,
}

impl ShareState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: RwSignal::new(Vec::new()),
        }
    }

    /// Take and clear the next pending shared payload (FIFO).
    pub fn take_one(&self) -> Option<AttachmentPayload> {
        let mut out = None;
        self.pending.update(|v| {
            if !v.is_empty() {
                out = Some(v.remove(0));
            }
        });
        out
    }

    pub fn has_pending(&self) -> bool {
        self.pending.with(|v| !v.is_empty())
    }
}

impl Default for ShareState {
    fn default() -> Self {
        Self::new()
    }
}

/// Poll the native Android share inbox and append anything found to `pending`.
/// Returns how many new items were added. No-op outside Tauri/Android.
pub async fn poll_shared(share: ShareState) -> usize {
    use base64::Engine as _;
    let items = crate::tauri_bridge::take_shared_attachments().await;
    if items.is_empty() {
        return 0;
    }
    let mut added = 0;
    for it in items {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&it.b64) else {
            continue;
        };
        let size = bytes.len() as u64;
        let is_image = it.mime.starts_with("image/");
        // Photos/videos shared via the OS sheet go through the compressed,
        // streamable Media path; anything else stays an untouched File.
        let kind = if is_image || it.mime.starts_with("video/") {
            AttachmentKind::Media
        } else {
            AttachmentKind::File
        };
        share.pending.update(|v| {
            v.push(AttachmentPayload {
                bytes,
                mime: it.mime.clone(),
                name: it.name.clone(),
                size,
                is_image,
                kind,
                caption: None,
            });
        });
        added += 1;
    }
    added
}

//! Message service — fetch, send, and display messages.
//!
//! Bridges the server API with the reactive UI state.
//! Uses real MLS encrypt/decrypt when the group crypto state is available.

use std::cell::RefCell;
use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_core::api::client::ApiClient;
use messenger_core::api::endpoints::mls::*;
use messenger_core::mls::application::{AppMessageBody, AppMessageKind, ApplicationEnvelope};
use messenger_core::mls::group::MlsRuntime;
use messenger_proto::mls::{PostMessageRequest, UpdateMessageStateRequest};
use uuid::Uuid;

use super::messages::{
    DeliveryStatus, DisplayMessage, DisplayReaction, MessageBody, MessageKind, MessagesState,
};
use super::session::build_api_client;

// MLS runtime is cached in a thread-local because `MessengerLocalStore` is ?Send
// (WASM-compatible async-trait), which means MlsRuntime is not Send+Sync and
// cannot be stored directly in MessageService (which goes into Leptos context).
thread_local! {
    static MLS_CACHE: RefCell<Option<MlsRuntime>> = const { RefCell::new(None) };
}

/// Reactive message operations handle.
#[derive(Clone)]
pub struct MessageService {
    pub messages: MessagesState,
}

impl MessageService {
    /// Create a new service (MLS not yet initialized).
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: MessagesState::new(),
        }
    }

    /// Initialize MLS runtime from the local store and device identity.
    ///
    /// Must be called once after session restore. Safe to call multiple times
    /// (subsequent calls are no-ops once MLS is set).
    pub async fn init_mls(&self, device_id: Uuid) {
        let already_initialized = MLS_CACHE.with(|c| c.borrow().is_some());
        if already_initialized {
            tracing::debug!("mls already initialized");
            return;
        }

        match messenger_storage::init_storage("default").await {
            Ok(local) => {
                let local: Arc<dyn messenger_storage::traits::MessengerLocalStore> = local.into();
                let runtime = MlsRuntime::new(local, device_id);
                MLS_CACHE.with(|c| *c.borrow_mut() = Some(runtime));
                tracing::debug!("mls runtime initialized");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to init local storage for MLS");
            }
        }
    }

    /// Fetch messages for a group from the server and update the reactive store.
    pub async fn load_messages(&self, group_id: Uuid) {
        let api = match build_api_client() {
            Some(c) => c,
            None => return,
        };

        match api.list_messages(group_id, None, None).await {
            Ok(resp) => {
                let display_messages = self.convert_messages(&resp.messages, group_id).await;
                self.messages.by_group.update(|map| {
                    map.insert(group_id, display_messages);
                });
                tracing::debug!(%group_id, count = resp.messages.len(), "messages loaded");
            }
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "failed to load messages");
            }
        }
    }

    /// Send a text message to a group.
    ///
    /// Returns the server-assigned message ID, or `None` on failure.
    pub async fn send_text(
        &self,
        group_id: Uuid,
        text: &str,
        reply_to: Option<Uuid>,
    ) -> Option<Uuid> {
        self.send_text_in_thread(group_id, text, reply_to, None).await
    }

    /// Send a text message with an optional thread root reference.
    pub async fn send_text_in_thread(
        &self,
        group_id: Uuid,
        text: &str,
        reply_to: Option<Uuid>,
        thread_root: Option<Uuid>,
    ) -> Option<Uuid> {
        let api = match build_api_client() {
            Some(c) => c,
            None => return None,
        };

        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000; // seconds

        // Build the MLS application envelope
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::Text,
            body: AppMessageBody::Text {
                text: text.to_string(),
                formatted_html: None,
            },
            reply_to_message_id: reply_to,
            thread_root_id: thread_root,
            created_at: now,
            sender_display_name_override: None,
        };

        let ciphertext = self
            .encrypt_envelope(group_id, &envelope)
            .await
            .unwrap_or_else(|| {
                // Fallback: send plaintext if MLS isn't ready or encryption fails
                tracing::warn!("MLS not ready, sending plaintext");
                text.as_bytes().to_vec()
            });

        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: ciphertext,
            parent_message_id: None,
            reply_to_message_id: reply_to,
            thread_root_id: thread_root,
            client_message_id,
        };

        match api.post_message(group_id, &req).await {
            Ok(resp) => {
                // Optimistic update — preserve reply/thread context so the local
                // bubble shows the reply quote without waiting for the server echo.
                self.messages.by_group.update(|map| {
                    map.entry(group_id).or_default().push(DisplayMessage {
                        id: resp.message_id,
                        client_message_id,
                        group_id,
                        sender_user_id: Uuid::nil(),
                        sender_device_id: Uuid::nil(),
                        kind: MessageKind::Text,
                        body: MessageBody::Text(text.to_string()),
                        reply_to_message_id: reply_to,
                        thread_root_id: thread_root,
                        created_at: now,
                        edited_at: None,
                        deleted_at: None,
                        delivery_status: DeliveryStatus::SentToServer,
                        reactions: Vec::new(),
                    });
                });
                Some(resp.message_id)
            }
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "failed to send message");
                None
            }
        }
    }

    /// Record + send a voice message.
    ///
    /// Pipeline: pick a fresh 32-byte content key → AES-GCM encrypt the Opus blob →
    /// upload the ciphertext → build & MLS-encrypt the envelope with `attachment_id` and
    /// the key → post the message → finalize the attachment to bind it to the message.
    pub async fn send_voice(
        &self,
        group_id: Uuid,
        payload: crate::chat::input_bar::VoicePayload,
    ) -> Option<Uuid> {
        use messenger_core::attachment_crypto::encrypt_attachment;
        use rand::RngCore;

        let api = build_api_client()?;

        // 1. Fresh per-attachment AES-256-GCM key. Lives only inside the MLS envelope.
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        let ciphertext = match encrypt_attachment(&key, &payload.bytes) {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("attachment encrypt failed: {e:?}");
                return None;
            }
        };
        let padded_size = ciphertext.len() as u64;
        let size_bucket = size_bucket_for(padded_size);

        // 2. Upload the ciphertext blob.
        let upload = match api.upload_attachment(ciphertext, padded_size, size_bucket).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("attachment upload failed: {e}");
                return None;
            }
        };

        // 3. Build & MLS-encrypt the envelope that references the attachment.
        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000;
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::Voice,
            body: AppMessageBody::Voice {
                attachment_id: upload.attachment_id,
                decryption_key: key.to_vec(),
                duration_ms: payload.duration_ms,
                waveform: payload.waveform.clone(),
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: now,
            sender_display_name_override: None,
        };
        let envelope_ct = self.encrypt_envelope(group_id, &envelope).await?;

        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: envelope_ct,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };
        let resp = match api.post_message(group_id, &req).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "voice post_message failed");
                return None;
            }
        };

        // 4. Finalize binds attachment to the message — otherwise GC will reap it.
        let finalize_req = messenger_proto::attachments::FinalizeAttachmentRequest {
            message_id: resp.message_id,
        };
        if let Err(e) = api
            .finalize_attachment(upload.attachment_id, &finalize_req)
            .await
        {
            tracing::warn!(error = %e, "finalize_attachment failed");
        }

        // 5. Optimistic local insert.
        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(DisplayMessage {
                id: resp.message_id,
                client_message_id,
                group_id,
                sender_user_id: Uuid::nil(),
                sender_device_id: Uuid::nil(),
                kind: MessageKind::Voice,
                body: MessageBody::Voice {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    duration_ms: payload.duration_ms,
                    waveform: payload.waveform,
                    transcription: None,
                },
                reply_to_message_id: None,
                thread_root_id: None,
                created_at: now,
                edited_at: None,
                deleted_at: None,
                delivery_status: DeliveryStatus::SentToServer,
                reactions: Vec::new(),
            });
        });

        Some(resp.message_id)
    }

    /// Send a generic attachment (file or image) — same pipeline as `send_voice` but
    /// with a `File` or `Image` envelope body.
    pub async fn send_attachment(
        &self,
        group_id: Uuid,
        payload: crate::chat::input_bar::AttachmentPayload,
    ) -> Option<Uuid> {
        use messenger_core::attachment_crypto::encrypt_attachment;
        use rand::RngCore;

        let api = build_api_client()?;

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        let ciphertext = match encrypt_attachment(&key, &payload.bytes) {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("attachment encrypt failed: {e:?}");
                return None;
            }
        };
        let padded_size = ciphertext.len() as u64;
        let size_bucket = size_bucket_for(padded_size);

        let upload = match api.upload_attachment(ciphertext, padded_size, size_bucket).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("attachment upload failed: {e}");
                return None;
            }
        };

        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000;
        let (kind, body, local_body) = if payload.is_image {
            (
                AppMessageKind::Image,
                AppMessageBody::Image {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: payload.mime.clone(),
                    // Width/height detection not wired — server doesn't see them
                    // and the client falls back to natural sizing.
                    width: 0,
                    height: 0,
                    thumb: None,
                },
                MessageBody::Image {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: payload.mime.clone(),
                    width: 0,
                    height: 0,
                    thumb: None,
                },
            )
        } else {
            (
                AppMessageKind::File,
                AppMessageBody::File {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: payload.mime.clone(),
                    filename: payload.name.clone(),
                    size: payload.size,
                },
                MessageBody::File {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: payload.mime.clone(),
                    name: payload.name.clone(),
                    size: payload.size,
                },
            )
        };

        let envelope = ApplicationEnvelope {
            client_message_id,
            kind,
            body,
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: now,
            sender_display_name_override: None,
        };
        let envelope_ct = self.encrypt_envelope(group_id, &envelope).await?;

        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: envelope_ct,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };
        let resp = match api.post_message(group_id, &req).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "attachment post_message failed");
                return None;
            }
        };
        let finalize_req = messenger_proto::attachments::FinalizeAttachmentRequest {
            message_id: resp.message_id,
        };
        if let Err(e) = api
            .finalize_attachment(upload.attachment_id, &finalize_req)
            .await
        {
            tracing::warn!(error = %e, "finalize_attachment failed");
        }

        let kind_for_display = if payload.is_image { MessageKind::Image } else { MessageKind::File };
        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(DisplayMessage {
                id: resp.message_id,
                client_message_id,
                group_id,
                sender_user_id: Uuid::nil(),
                sender_device_id: Uuid::nil(),
                kind: kind_for_display,
                body: local_body,
                reply_to_message_id: None,
                thread_root_id: None,
                created_at: now,
                edited_at: None,
                deleted_at: None,
                delivery_status: DeliveryStatus::SentToServer,
                reactions: Vec::new(),
            });
        });

        Some(resp.message_id)
    }

    /// Edit a message — send a replacement and mark the original as edited.
    pub async fn edit_message(
        &self,
        group_id: Uuid,
        original_id: Uuid,
        new_text: &str,
    ) -> Option<Uuid> {
        let api = match build_api_client() {
            Some(c) => c,
            None => return None,
        };

        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000;

        // Build EditNotice envelope
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::EditNotice,
            body: AppMessageBody::EditNotice {
                original_message_id: original_id,
                new_text: new_text.to_string(),
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: now,
            sender_display_name_override: None,
        };

        let ciphertext = self
            .encrypt_envelope(group_id, &envelope)
            .await
            .unwrap_or_else(|| new_text.as_bytes().to_vec());

        // 1. Post the replacement message
        let post_req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: ciphertext,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };

        let replacement_id = match api.post_message(group_id, &post_req).await {
            Ok(resp) => resp.message_id,
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "failed to post edit replacement");
                return None;
            }
        };

        // 2. Mark original as edited
        let state_req = UpdateMessageStateRequest {
            kind: "edit".to_string(),
            replacement_message_id: Some(replacement_id),
        };

        if let Err(e) = api.update_message_state(original_id, &state_req).await {
            tracing::warn!(%original_id, error = %e, "failed to mark message as edited");
        }

        Some(replacement_id)
    }

    /// Delete a message — mark it as deleted on the server.
    pub async fn delete_message(&self, group_id: Uuid, message_id: Uuid) -> bool {
        let api = match build_api_client() {
            Some(c) => c,
            None => return false,
        };

        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000;

        // Build DeleteNotice envelope
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::DeleteNotice,
            body: AppMessageBody::DeleteNotice {
                target_message_id: message_id,
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: now,
            sender_display_name_override: None,
        };

        let ciphertext = self
            .encrypt_envelope(group_id, &envelope)
            .await
            .unwrap_or_default();

        // 1. Post the delete notice as an MLS message
        let post_req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: ciphertext,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };

        if let Err(e) = api.post_message(group_id, &post_req).await {
            tracing::warn!(%group_id, error = %e, "failed to post delete notice");
            // Continue anyway to mark the original as deleted
        }

        // 2. Mark original as deleted on server
        let state_req = UpdateMessageStateRequest {
            kind: "delete".to_string(),
            replacement_message_id: None,
        };

        match api.update_message_state(message_id, &state_req).await {
            Ok(_) => {
                // Update local state
                self.messages.by_group.update(|map| {
                    if let Some(msgs) = map.get_mut(&group_id) {
                        if let Some(msg) = msgs.iter_mut().find(|m| m.id == message_id) {
                            msg.deleted_at = Some(js_sys::Date::now() as i64 / 1000);
                        }
                    }
                });
                true
            }
            Err(e) => {
                tracing::warn!(%message_id, error = %e, "failed to delete message");
                false
            }
        }
    }

    /// Toggle a reaction on a message.
    pub async fn toggle_reaction(
        &self,
        _group_id: Uuid,
        message_id: Uuid,
        emoji: &str,
    ) -> bool {
        let api = match build_api_client() {
            Some(c) => c,
            None => return false,
        };

        // Check if we already have this reaction
        let has_own = self
            .messages
            .by_group
            .get_untracked()
            .values()
            .flatten()
            .find(|m| m.id == message_id)
            .map_or(false, |m| m.reactions.iter().any(|r| r.emoji == emoji && r.has_own));

        // Compute blind index — use BLAKE3 of (message_id, emoji)
        let blind_index = blake3::hash(format!("{}:{}", message_id, emoji).as_bytes())
            .as_bytes()
            .to_vec();

        // Optimistic update — toggle locally immediately
        let emoji_owned = emoji.to_string();
        self.messages.by_group.update(|map| {
            for msgs in map.values_mut() {
                if let Some(msg) = msgs.iter_mut().find(|m| m.id == message_id) {
                    if has_own {
                        msg.reactions.retain(|r| r.emoji != emoji_owned);
                    } else {
                        if let Some(existing) = msg.reactions.iter_mut().find(|r| r.emoji == emoji_owned) {
                            existing.count += 1;
                            existing.has_own = true;
                        } else {
                            msg.reactions.push(DisplayReaction {
                                emoji: emoji_owned.clone(),
                                count: 1,
                                has_own: true,
                            });
                        }
                    }
                }
            }
        });

        // Call server
        if has_own {
            use messenger_proto::reactions::RemoveReactionRequest;
            let req = RemoveReactionRequest {
                message_id,
                reaction_blind_index: blind_index,
            };
            match api.remove_reaction(message_id, &req).await {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!(%message_id, error = %e, "failed to remove reaction");
                    false
                }
            }
        } else {
            use messenger_proto::reactions::AddReactionRequest;
            let req = AddReactionRequest {
                message_id,
                reaction_blind_index: blind_index,
            };
            match api.add_reaction(message_id, &req).await {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!(%message_id, error = %e, "failed to add reaction");
                    false
                }
            }
        }
    }

    /// Add an own message to the local buffer (optimistic update).
    fn add_own_message(&self, group_id: Uuid, text: &str, client_msg_id: Uuid, created_at: i64) {
        let msg = DisplayMessage {
            id: client_msg_id,
            client_message_id: client_msg_id,
            group_id,
            sender_user_id: Uuid::nil(),
            sender_device_id: Uuid::nil(),
            kind: MessageKind::Text,
            body: MessageBody::Text(text.to_string()),
            reply_to_message_id: None,
            thread_root_id: None,
            created_at,
            edited_at: None,
            deleted_at: None,
            delivery_status: DeliveryStatus::SentToServer,
            reactions: Vec::new(),
        };

        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(msg);
        });
    }

    /// Encrypt an `ApplicationEnvelope` via MLS, falling back to plaintext.
    async fn encrypt_envelope(
        &self,
        group_id: Uuid,
        envelope: &ApplicationEnvelope,
    ) -> Option<Vec<u8>> {
        let plaintext = rmp_serde::to_vec_named(envelope).ok()?;

        // Get ClientIdentity from session
        let session = use_context::<super::session::Session>()?;
        let identity = match session.state.get_untracked() {
            super::session::SessionState::Authenticated { identity, .. } => identity,
            _ => return None,
        };

        // Run encryption inside the thread-local accessor.
        // We hand the runtime to the async block by taking it out of the cache
        // temporarily and putting it back after.
        let (ct, runtime) = {
            let rt = MLS_CACHE.with(|c| c.borrow_mut().take())?;
            let result = rt
                .encrypt_application_message(group_id, &identity, &plaintext)
                .await;
            (result, rt)
        };
        MLS_CACHE.with(|c| *c.borrow_mut() = Some(runtime));

        match ct {
            Ok(ct) => Some(ct),
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "MLS encrypt failed");
                None
            }
        }
    }

    /// Decrypt an MLS ciphertext, returning the plaintext bytes.
    async fn decrypt_ciphertext(
        &self,
        group_id: Uuid,
        ciphertext: &[u8],
    ) -> Option<Vec<u8>> {
        let (result, runtime) = {
            let rt = MLS_CACHE.with(|c| c.borrow_mut().take())?;
            let res = rt
                .decrypt_application_message(group_id, ciphertext)
                .await;
            (res, rt)
        };
        MLS_CACHE.with(|c| *c.borrow_mut() = Some(runtime));

        match result {
            Ok(dec) => Some(dec.plaintext),
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "MLS decrypt failed");
                None
            }
        }
    }

    /// Convert stored messages to DisplayMessages with MLS decryption.
    async fn convert_messages(
        &self,
        stored: &[messenger_proto::mls::StoredMessage],
        group_id: Uuid,
    ) -> Vec<DisplayMessage> {
        let mut result = Vec::with_capacity(stored.len());
        for msg in stored {
            result.push(self.convert_one(msg, group_id).await);
        }
        result
    }

    /// Convert a single stored message, attempting MLS decryption.
    async fn convert_one(
        &self,
        msg: &messenger_proto::mls::StoredMessage,
        group_id: Uuid,
    ) -> DisplayMessage {
        // Try MLS decryption first
        let decrypted = if !msg.mls_ciphertext.is_empty() {
            self.decrypt_ciphertext(group_id, &msg.mls_ciphertext).await
        } else {
            None
        };

        // Parse the application envelope from decrypted bytes
        let (kind, body, reply_to, thread_root, created) =
            if let Some(ref plaintext) = decrypted {
                match rmp_serde::from_slice::<ApplicationEnvelope>(plaintext) {
                    Ok(envelope) => {
                        let (k, b) = Self::envelope_to_display(&envelope);
                        (
                            k,
                            b,
                            envelope.reply_to_message_id,
                            envelope.thread_root_id,
                            envelope.created_at,
                        )
                    }
                    Err(_) => {
                        // Fallback: treat as plaintext
                        let text = String::from_utf8_lossy(plaintext).to_string();
                        (
                            MessageKind::Text,
                            MessageBody::Text(text),
                            None,
                            None,
                            msg.created_at,
                        )
                    }
                }
            } else {
                // No MLS yet — try lossy UTF-8
                let text = if msg.mls_ciphertext.is_empty() {
                    String::new()
                } else {
                    String::from_utf8_lossy(&msg.mls_ciphertext).to_string()
                };
                (
                    MessageKind::Text,
                    MessageBody::Text(text),
                    None,
                    None,
                    msg.created_at,
                )
            };

        // Parse state (edit/delete)
        let (edited_at, deleted_at) = match &msg.state {
            Some(s) => (s.edited_at, s.deleted_at),
            None => (None, None),
        };

        DisplayMessage {
            id: msg.id,
            client_message_id: msg.client_message_id,
            group_id,
            sender_user_id: msg.sender_user_id,
            sender_device_id: msg.sender_device_id,
            kind,
            body,
            reply_to_message_id: reply_to.or(msg.reply_to_message_id),
            thread_root_id: thread_root.or(msg.thread_root_id),
            created_at: created,
            edited_at,
            deleted_at,
            delivery_status: DeliveryStatus::SentToServer,
            reactions: Vec::new(),
        }
    }

    /// Convert an `ApplicationEnvelope` to display-level `(MessageKind, MessageBody)`.
    fn envelope_to_display(envelope: &ApplicationEnvelope) -> (MessageKind, MessageBody) {
        match envelope.body {
            AppMessageBody::Text { ref text, .. } => {
                (MessageKind::Text, MessageBody::Text(text.clone()))
            }
            AppMessageBody::Voice {
                attachment_id,
                ref decryption_key,
                duration_ms,
                ref waveform,
            } => (
                MessageKind::Voice,
                MessageBody::Voice {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    duration_ms,
                    waveform: waveform.clone(),
                    transcription: None,
                },
            ),
            AppMessageBody::File {
                attachment_id,
                ref decryption_key,
                ref mime,
                ref filename,
                size,
            } => (
                MessageKind::File,
                MessageBody::File {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    mime: mime.clone(),
                    name: filename.clone(),
                    size,
                },
            ),
            AppMessageBody::Image {
                attachment_id,
                ref decryption_key,
                ref mime,
                width,
                height,
                ref thumb,
            } => (
                MessageKind::Image,
                MessageBody::Image {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    mime: mime.clone(),
                    width,
                    height,
                    thumb: thumb.clone(),
                },
            ),
            AppMessageBody::SystemNote { ref code, .. } => {
                (MessageKind::System, MessageBody::System {
                    action: code.clone(),
                })
            }
            AppMessageBody::EditNotice { ref new_text, .. } => {
                (MessageKind::Text, MessageBody::Text(new_text.clone()))
            }
            AppMessageBody::DeleteNotice { .. } => (MessageKind::System, MessageBody::System {
                action: "deleted".to_string(),
            }),
            AppMessageBody::ReadReceipt { .. }
            | AppMessageBody::Reaction { .. }
            | AppMessageBody::AvatarUpdate { .. }
            | AppMessageBody::UsernameUpdate { .. } => {
                (MessageKind::System, MessageBody::System {
                    action: "event".to_string(),
                })
            }
        }
    }
}

impl Default for MessageService {
    fn default() -> Self {
        Self::new()
    }
}

/// Bucketed size — coarse classification used by the server for metadata minimization.
/// Matches the server's bucket spec: powers-of-two from 1 KiB up to 32 MiB.
fn size_bucket_for(size: u64) -> u32 {
    let kib = (size / 1024).max(1);
    let bits = 64u32 - kib.leading_zeros();
    bits.min(20)
}

// --- Module-level helpers for SyncService ---

/// Check whether MLS has been initialized (MLS_CACHE contains a runtime).
///
/// Used by `SyncService` to decide whether to attempt welcome processing.
#[must_use]
pub fn is_mls_initialized() -> bool {
    MLS_CACHE.with(|c| c.borrow().is_some())
}

/// Join a group via a welcome message using the cached MLS runtime.
///
/// Returns `true` if the join succeeded.
pub async fn join_welcome(welcome_id: Uuid, welcome_ciphertext: &[u8]) -> bool {
    let (result, runtime) = {
        let rt = match MLS_CACHE.with(|c| c.borrow_mut().take()) {
            Some(r) => r,
            None => {
                tracing::warn!("MLS not initialized, cannot join welcome");
                return false;
            }
        };

        let identity = {
            use super::session::Session;
            let session = use_context::<Session>();
            match session.and_then(|s| match s.state.get_untracked() {
                super::session::SessionState::Authenticated { identity, .. } => Some(identity),
                _ => None,
            }) {
                Some(id) => id,
                None => {
                    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
                    return false;
                }
            }
        };

        let res = rt.join_via_welcome(&identity, welcome_ciphertext).await;
        (res, rt)
    };
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(runtime));

    match result {
        Ok(_) => {
            tracing::debug!(welcome_id = ?welcome_id, "joined group via welcome");
            true
        }
        Err(e) => {
            tracing::warn!(welcome_id = ?welcome_id, error = %e, "failed to join via welcome");
            false
        }
    }
}

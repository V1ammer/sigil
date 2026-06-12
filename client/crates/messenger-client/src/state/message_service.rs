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
use super::session::{build_api_client, Session, SessionState};
use super::users::UsersState;

/// Current user's plaintext username, taken from the session state stored on
/// `MessageService`. Used to fill `sender_display_name_override` on outgoing
/// messages so peers can identify the sender without a server-side lookup.
///
/// Reads through a `RwSignal` rather than `use_context::<Session>()` so it
/// works from inside nested `spawn_local` tasks where the leptos owner is
/// gone (voice/attachment pipelines call back into us across two awaits).
fn current_username() -> Option<String> {
    SESSION_STATE.with(|c| c.borrow().as_ref().and_then(|s| {
        match s.get_untracked() {
            SessionState::Authenticated { identity, .. } => Some(identity.username.clone()),
            _ => None,
        }
    }))
}

/// Current user's id — same access pattern as [`current_username`].
fn current_user_id() -> Option<Uuid> {
    SESSION_STATE.with(|c| c.borrow().as_ref().and_then(|s| {
        match s.get_untracked() {
            SessionState::Authenticated { identity, .. } => Some(identity.user_id),
            _ => None,
        }
    }))
}

thread_local! {
    static SESSION_STATE: RefCell<Option<RwSignal<SessionState>>> = const { RefCell::new(None) };
    static USERS_STATE: RefCell<Option<UsersState>> = const { RefCell::new(None) };
    static CHATS_STATE: RefCell<Option<crate::state::chats::ChatsState>> = const { RefCell::new(None) };
}

/// Bump a chat's `last_message_at` if `ts_ms` is newer than what's stored.
/// Safe to call from detached async tasks — uses the thread-local copy of
/// `ChatsState` mirrored at startup.
fn touch_chat_last_message(group_id: Uuid, ts_ms: i64) {
    CHATS_STATE.with(|c| {
        if let Some(chats) = c.borrow().as_ref() {
            chats.touch_last_message(group_id, ts_ms);
        }
    });
}

/// Same as [`touch_chat_last_message`] but also publishes a preview snippet
/// and the message kind so the sidebar can render a Telegram-style preview.
fn set_chat_last_message(
    group_id: Uuid,
    ts_ms: i64,
    preview: Option<String>,
    kind: Option<MessageKind>,
) {
    CHATS_STATE.with(|c| {
        if let Some(chats) = c.borrow().as_ref() {
            chats.set_last_message(group_id, ts_ms, preview, kind);
        }
    });
}

/// Extract a short snippet for the chat list from a decoded message body.
fn preview_from_body(body: &MessageBody) -> String {
    match body {
        MessageBody::Text(t) => {
            let trimmed = t.trim();
            if trimmed.chars().count() > 80 {
                trimmed.chars().take(80).collect::<String>() + "…"
            } else {
                trimmed.to_string()
            }
        }
        MessageBody::Voice { .. }
        | MessageBody::Image { .. } => String::new(),
        MessageBody::File { name, .. } => name.clone(),
        MessageBody::System { action } => action.clone(),
    }
}

/// Take the `MlsRuntime` out of `MLS_CACHE`, polling until it's available.
///
/// MLS is held by a single `thread_local!` slot. Several async paths
/// (encrypt/decrypt/join_welcome) need exclusive access across awaits, so
/// they `take()` it for the duration of the call and put it back. If another
/// task is already holding the runtime, polling here yields to the runtime
/// every ~5 ms instead of dropping the operation.
///
/// Returns `None` only if the runtime was never initialized at all.
async fn take_mls_runtime() -> Option<MlsRuntime> {
    if MLS_CACHE.with(|c| c.borrow().is_none()) {
        return None;
    }
    let mut attempts = 0u32;
    loop {
        if let Some(rt) = MLS_CACHE.with(|c| c.borrow_mut().take()) {
            return Some(rt);
        }
        attempts += 1;
        // Give up after ~5 s — something is wrong if we waited this long.
        if attempts > 1_000 {
            web_sys::console::error_1(&"[take_mls_runtime] gave up after 5s of contention".into());
            return None;
        }
        gloo_timers::future::TimeoutFuture::new(5).await;
    }
}

/// Wire up session/users state for use from detached async tasks.
///
/// Must be called once at app startup, after `provide_session()` and
/// `provide_context(UsersState::new())`. Required because `MessageService`
/// is reached from nested `spawn_local` tasks (voice/attachment pipelines)
/// where the leptos owner — and therefore `use_context` — is gone.
pub fn init_message_service_context(session: &Session, users: UsersState, chats: crate::state::chats::ChatsState) {
    SESSION_STATE.with(|c| *c.borrow_mut() = Some(session.state));
    USERS_STATE.with(|c| *c.borrow_mut() = Some(users));
    CHATS_STATE.with(|c| *c.borrow_mut() = Some(chats));
}

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
            web_sys::console::log_1(&"[init_mls] already initialized".into());
            return;
        }

        web_sys::console::log_1(&"[init_mls] starting...".into());
        match messenger_storage::init_storage("default").await {
            Ok(local) => {
                let local: Arc<dyn messenger_storage::traits::MessengerLocalStore> = local.into();
                let runtime = MlsRuntime::new(local, device_id);
                MLS_CACHE.with(|c| *c.borrow_mut() = Some(runtime));
                web_sys::console::log_1(&"[init_mls] runtime installed in MLS_CACHE".into());
            }
            Err(e) => {
                web_sys::console::error_1(&format!("[init_mls] storage init failed: {e}").into());
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
                let latest = display_messages.iter().max_by_key(|m| m.created_at).cloned();
                self.messages.by_group.update(|map| {
                    map.insert(group_id, display_messages);
                });
                if let Some(m) = latest {
                    set_chat_last_message(
                        group_id,
                        m.created_at * 1000,
                        Some(preview_from_body(&m.body)),
                        Some(m.kind),
                    );
                }
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
        let me = current_username();

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
            sender_display_name_override: me.clone(),
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
                        sender_display_name: me.clone(),
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
                set_chat_last_message(
                    group_id,
                    now * 1000,
                    Some(preview_from_body(&MessageBody::Text(text.to_string()))),
                    Some(MessageKind::Text),
                );
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
        let me = current_username();
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
            sender_display_name_override: me.clone(),
        };
        // Encrypt or fall back to a plaintext envelope so the message still
        // delivers when MLS group state isn't set up locally (parity with the
        // text path's `MLS not ready, sending plaintext`).
        let envelope_ct = match self.encrypt_envelope(group_id, &envelope).await {
            Some(ct) => ct,
            None => match rmp_serde::to_vec_named(&envelope) {
                Ok(plain) => {
                    web_sys::console::warn_1(&"[send_voice] MLS not ready, sending plaintext envelope".into());
                    plain
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("[send_voice] envelope serialize: {e}").into());
                    return None;
                }
            },
        };

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
                web_sys::console::error_1(&format!("[send_voice] post_message failed: {e}").into());
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
                sender_display_name: me.clone(),
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
        set_chat_last_message(group_id, now * 1000, Some(String::new()), Some(MessageKind::Voice));

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
        let me = current_username();
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
            sender_display_name_override: me.clone(),
        };
        // Encrypt or fall back to a plaintext envelope so the message still
        // delivers when MLS group state isn't set up locally (parity with the
        // text path's `MLS not ready, sending plaintext`).
        let envelope_ct = match self.encrypt_envelope(group_id, &envelope).await {
            Some(ct) => ct,
            None => match rmp_serde::to_vec_named(&envelope) {
                Ok(plain) => {
                    web_sys::console::warn_1(&"[send_attachment] MLS not ready, sending plaintext envelope".into());
                    plain
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("[send_attachment] envelope serialize: {e}").into());
                    return None;
                }
            },
        };

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
                web_sys::console::error_1(&format!("[send_attachment] post_message failed: {e}").into());
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
        let preview = Some(preview_from_body(&local_body));
        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(DisplayMessage {
                id: resp.message_id,
                client_message_id,
                group_id,
                sender_user_id: Uuid::nil(),
                sender_device_id: Uuid::nil(),
                sender_display_name: me.clone(),
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
        set_chat_last_message(group_id, now * 1000, preview, Some(kind_for_display));

        Some(resp.message_id)
    }

    /// Broadcast the current (or removed) own avatar to one group as an MLS
    /// `AvatarUpdate`. Unlike `send_attachment` this is a profile side-channel:
    /// nothing is added to the local timeline or the chat-list preview.
    ///
    /// Returns `false` when there was nothing to send or the send failed.
    pub async fn broadcast_avatar(&self, group_id: Uuid) -> bool {
        use messenger_core::attachment_crypto::encrypt_attachment;
        use rand::RngCore;

        let Some(me_id) = current_user_id() else { return false };
        let Some(api) = build_api_client() else { return false };

        let own = crate::state::avatar_store::load_own_avatar(me_id);
        let (body, upload_id) = match own.as_deref().and_then(crate::state::avatar_store::data_url_to_bytes) {
            Some((mime, bytes)) => {
                let mut key = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                let ciphertext = match encrypt_attachment(&key, &bytes) {
                    Ok(ct) => ct,
                    Err(e) => {
                        tracing::warn!("avatar encrypt failed: {e:?}");
                        return false;
                    }
                };
                let padded_size = ciphertext.len() as u64;
                let size_bucket = size_bucket_for(padded_size);
                let upload = match api.upload_attachment(ciphertext, padded_size, size_bucket).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(error = %e, "avatar upload failed");
                        return false;
                    }
                };
                (
                    AppMessageBody::AvatarUpdate {
                        avatar_blob_id: Some(upload.attachment_id),
                        decryption_key: key.to_vec(),
                        mime,
                    },
                    Some(upload.attachment_id),
                )
            }
            // No stored avatar — explicit removal notice.
            None => (
                AppMessageBody::AvatarUpdate {
                    avatar_blob_id: None,
                    decryption_key: Vec::new(),
                    mime: String::new(),
                },
                None,
            ),
        };

        let client_message_id = Uuid::now_v7();
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::AvatarUpdate,
            body,
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: js_sys::Date::now() as i64 / 1000,
            sender_display_name_override: current_username(),
        };
        let envelope_ct = match self.encrypt_envelope(group_id, &envelope).await {
            Some(ct) => ct,
            None => match rmp_serde::to_vec_named(&envelope) {
                Ok(plain) => plain,
                Err(e) => {
                    tracing::warn!("avatar envelope serialize failed: {e}");
                    return false;
                }
            },
        };
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
                tracing::warn!(%group_id, error = %e, "avatar broadcast failed");
                return false;
            }
        };
        if let Some(attachment_id) = upload_id {
            let finalize_req = messenger_proto::attachments::FinalizeAttachmentRequest {
                message_id: resp.message_id,
            };
            if let Err(e) = api.finalize_attachment(attachment_id, &finalize_req).await {
                tracing::warn!(error = %e, "avatar finalize failed");
            }
        }
        true
    }

    /// Broadcast the own avatar to every chat in the list (used after a
    /// change in settings).
    pub async fn broadcast_avatar_all(&self) {
        let mut groups: Vec<Uuid> = CHATS_STATE
            .with(|c| c.borrow().clone())
            .map(|cs| cs.chats.get_untracked().iter().map(|ch| ch.group_id).collect())
            .unwrap_or_default();
        // The UI chat list is only populated once the chats screen has been
        // visited; when settings is opened directly the state is empty, so
        // fall back to asking the server.
        if groups.is_empty() {
            if let Some(api) = build_api_client() {
                if let Ok(resp) = api.list_groups(None).await {
                    groups = resp.groups.iter().map(|g| g.id).collect();
                }
            }
        }
        web_sys::console::log_1(
            &format!("[avatar] broadcast_all: {} group(s)", groups.len()).into(),
        );
        for group_id in groups {
            let ok = self.broadcast_avatar(group_id).await;
            web_sys::console::log_1(&format!("[avatar] broadcast {group_id}: {ok}").into());
        }
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
            sender_display_name_override: current_username(),
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
            sender_display_name_override: current_username(),
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
            sender_display_name: current_username(),
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

        let preview = Some(preview_from_body(&msg.body));
        let kind = msg.kind;
        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(msg);
        });
        set_chat_last_message(group_id, created_at * 1000, preview, Some(kind));
    }

    /// Encrypt an `ApplicationEnvelope` via MLS, falling back to plaintext.
    async fn encrypt_envelope(
        &self,
        group_id: Uuid,
        envelope: &ApplicationEnvelope,
    ) -> Option<Vec<u8>> {
        let plaintext = match rmp_serde::to_vec_named(envelope) {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::error_1(&format!("[encrypt_envelope] rmp_serde encode failed: {e}").into());
                return None;
            }
        };

        // Get ClientIdentity from session (stored at app init so it works from
        // nested spawn_local tasks where leptos context is gone).
        let Some(state) = SESSION_STATE.with(|c| c.borrow().as_ref().copied()) else {
            web_sys::console::error_1(&"[encrypt_envelope] SESSION_STATE not initialized".into());
            return None;
        };
        let identity = match state.get_untracked() {
            super::session::SessionState::Authenticated { identity, .. } => identity,
            _ => {
                web_sys::console::error_1(&"[encrypt_envelope] session not authenticated".into());
                return None;
            }
        };

        // Wait for the MLS runtime to be available — another task (welcome
        // join, message decrypt) may currently hold it. The runtime is moved
        // out of the cache for the duration of the await because the inner
        // future borrows it.
        let Some(rt) = take_mls_runtime().await else {
            web_sys::console::error_1(&"[encrypt_envelope] MLS_CACHE empty (runtime not initialized)".into());
            return None;
        };
        let result = rt
            .encrypt_application_message(group_id, &identity, &plaintext)
            .await;
        MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));

        match result {
            Ok(ct) => Some(ct),
            Err(e) => {
                web_sys::console::error_1(&format!("[encrypt_envelope] MLS encrypt failed for {group_id}: {e}").into());
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
            let rt = take_mls_runtime().await?;
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
    ///
    /// MLS control-plane frames (proposals, commits, welcomes) are dropped here
    /// — only `application` frames carry user content.
    async fn convert_messages(
        &self,
        stored: &[messenger_proto::mls::StoredMessage],
        group_id: Uuid,
    ) -> Vec<DisplayMessage> {
        let mut result = Vec::with_capacity(stored.len());
        for msg in stored {
            if msg.wire_format != "application" {
                continue;
            }
            if let Some(dm) = self.convert_one(msg, group_id).await {
                result.push(dm);
            }
        }
        result
    }

    /// Convert a single stored message, attempting MLS decryption.
    ///
    /// Returns `None` for profile side-channel messages (`AvatarUpdate`) —
    /// they are consumed here and never appear in the timeline.
    async fn convert_one(
        &self,
        msg: &messenger_proto::mls::StoredMessage,
        group_id: Uuid,
    ) -> Option<DisplayMessage> {
        // Try MLS decryption first. If that fails (MLS state missing, etc.)
        // fall back to treating `mls_ciphertext` as a plaintext envelope —
        // the send path uses the same fallback when MLS isn't ready.
        let decrypted = if !msg.mls_ciphertext.is_empty() {
            self.decrypt_ciphertext(group_id, &msg.mls_ciphertext).await
        } else {
            None
        };
        let payload: &[u8] = decrypted.as_deref().unwrap_or(&msg.mls_ciphertext);

        // Parse the application envelope. Works for both MLS-decrypted bytes
        // and raw plaintext envelopes sent during the MLS-not-ready fallback.
        let (kind, body, reply_to, thread_root, created, sender_display_name) =
            match rmp_serde::from_slice::<ApplicationEnvelope>(payload) {
                Ok(envelope) => {
                    // Remember the sender's username so reply previews, group
                    // headers and future messages can show a real label.
                    if let (Some(users), Some(name)) = (
                        USERS_STATE.with(|c| c.borrow().clone()),
                        envelope.sender_display_name_override.as_deref(),
                    ) {
                        users.remember(msg.sender_user_id, name);
                    }
                    // Track the direct-chat peer so the chat list can resolve
                    // the other side's avatar without server lookups.
                    Self::remember_direct_peer(group_id, msg.sender_user_id);
                    // Profile side-channel: consume and drop from the timeline.
                    if let AppMessageBody::AvatarUpdate {
                        avatar_blob_id,
                        ref decryption_key,
                        ref mime,
                    } = envelope.body
                    {
                        Self::apply_avatar_update(
                            msg.sender_user_id,
                            avatar_blob_id,
                            decryption_key.clone(),
                            mime.clone(),
                        );
                        return None;
                    }
                    let (k, b) = Self::envelope_to_display(&envelope);
                    (
                        k,
                        b,
                        envelope.reply_to_message_id,
                        envelope.thread_root_id,
                        envelope.created_at,
                        envelope.sender_display_name_override.clone(),
                    )
                }
                Err(_) => {
                    // Last resort: treat as plain UTF-8 text (legacy text fallback).
                    let text = if payload.is_empty() {
                        String::new()
                    } else {
                        String::from_utf8_lossy(payload).to_string()
                    };
                    (
                        MessageKind::Text,
                        MessageBody::Text(text),
                        None,
                        None,
                        msg.created_at,
                        None,
                    )
                }
            };

        // Parse state (edit/delete)
        let (edited_at, deleted_at) = match &msg.state {
            Some(s) => (s.edited_at, s.deleted_at),
            None => (None, None),
        };

        Some(DisplayMessage {
            id: msg.id,
            client_message_id: msg.client_message_id,
            group_id,
            sender_user_id: msg.sender_user_id,
            sender_device_id: msg.sender_device_id,
            sender_display_name,
            kind,
            body,
            reply_to_message_id: reply_to.or(msg.reply_to_message_id),
            thread_root_id: thread_root.or(msg.thread_root_id),
            created_at: created,
            edited_at,
            deleted_at,
            delivery_status: DeliveryStatus::SentToServer,
            reactions: Vec::new(),
        })
    }

    /// Record the sender as the direct-chat peer of `group_id` (no-op for
    /// own messages and non-direct groups).
    fn remember_direct_peer(group_id: Uuid, sender: Uuid) {
        if sender.is_nil() || Some(sender) == current_user_id() {
            return;
        }
        let Some(users) = USERS_STATE.with(|c| c.borrow().clone()) else {
            return;
        };
        let is_direct = CHATS_STATE.with(|c| c.borrow().clone()).is_some_and(|cs| {
            cs.chats
                .get_untracked()
                .iter()
                .any(|ch| {
                    ch.group_id == group_id
                        && ch.chat_type == crate::state::chats::ChatType::Direct
                })
        });
        if is_direct {
            users.remember_peer(group_id, sender);
            // The welcome recipient never typed the peer's username, so the
            // chat may still be labeled with a bare UUID. Backfill it from
            // the name cache (populated by envelope display-name overrides).
            if let (Some(cs), Some(name)) = (
                CHATS_STATE.with(|c| c.borrow().clone()),
                users.get(sender),
            ) {
                let needs_name = cs
                    .display_name_cache
                    .get_untracked()
                    .get(&group_id)
                    .is_none_or(|n| n.parse::<Uuid>().is_ok());
                if needs_name {
                    cs.set_display_name(group_id, &name);
                }
            }
        }
    }

    /// Fetch + decrypt a peer's avatar blob and cache it as a data URL.
    /// Own updates are ignored — the local store is the source of truth.
    fn apply_avatar_update(sender: Uuid, blob_id: Option<Uuid>, key: Vec<u8>, mime: String) {
        if sender.is_nil() || Some(sender) == current_user_id() {
            return;
        }
        let Some(users) = USERS_STATE.with(|c| c.borrow().clone()) else {
            return;
        };
        match blob_id {
            None => users.forget_avatar(sender),
            Some(id) => {
                spawn_local(async move {
                    let Some(api) = build_api_client() else { return };
                    let ciphertext = match api.download_attachment(id, None).await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!(%id, error = %e, "avatar blob download failed");
                            return;
                        }
                    };
                    let Ok(key_arr) = <[u8; 32]>::try_from(key.as_slice()) else {
                        tracing::warn!("avatar update with malformed key");
                        return;
                    };
                    let Ok(plain) =
                        messenger_core::attachment_crypto::decrypt_attachment(&key_arr, &ciphertext)
                    else {
                        tracing::warn!(%id, "avatar blob decrypt failed");
                        return;
                    };
                    let mime = if mime.is_empty() { "image/jpeg".to_string() } else { mime };
                    let data_url = crate::state::avatar_store::bytes_to_data_url(&mime, &plain);
                    users.remember_avatar(sender, &data_url);
                });
            }
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
        let rt = match take_mls_runtime().await {
            Some(r) => r,
            None => {
                tracing::warn!("MLS not initialized, cannot join welcome");
                return false;
            }
        };

        let identity = {
            let state = SESSION_STATE.with(|c| c.borrow().as_ref().copied());
            match state.and_then(|s| match s.get_untracked() {
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

//! Message service — fetch, send, and display messages.
//!
//! Bridges the server API with the reactive UI state.
//! Uses real MLS encrypt/decrypt when the group crypto state is available.

use std::cell::RefCell;
use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_core::api::client::ApiClient;
use messenger_core::mls::application::{AppMessageBody, AppMessageKind, ApplicationEnvelope};
use messenger_core::mls::group::MlsRuntime;
use messenger_proto::mls::{PostMessageRequest, UpdateMessageStateRequest};
use uuid::Uuid;

use super::messages::{
    DeliveryStatus, DisplayMessage, DisplayReaction, MessageBody, MessageKind, MessagesState,
};
use super::session::{build_api_client, Session, SessionState};
use super::users::UsersState;

/// Bind an attachment to its message with bounded retries.
///
/// Finalize must succeed: until it lands, `attachment.message_id` is NULL and
/// the server denies download to everyone but the uploader (recipients see a
/// 403). A single dropped finalize would make the image permanently broken for
/// peers, so we retry a few times with backoff before giving up.
async fn finalize_attachment_retrying(
    api: &ApiClient,
    attachment_id: Uuid,
    message_id: Uuid,
) {
    let req = messenger_proto::attachments::FinalizeAttachmentRequest { message_id };
    for (attempt, delay) in [0u32, 300, 800, 2000].into_iter().enumerate() {
        if delay > 0 {
            gloo_timers::future::TimeoutFuture::new(delay).await;
        }
        match api.finalize_attachment(attachment_id, &req).await {
            Ok(()) => return,
            Err(e) => tracing::warn!(
                error = %e,
                attempt = attempt + 1,
                %attachment_id,
                "finalize_attachment failed"
            ),
        }
    }
}

/// Result of converting one stored message for display. Control messages are
/// consumed (edits/deletes applied to their target, side-channels dropped).
enum Converted {
    /// A normal message to show in the timeline.
    Show(DisplayMessage),
    /// An edit of an earlier message: replace its text.
    Edit {
        original: Uuid,
        new_text: String,
        at: i64,
    },
    /// A delete of an earlier message: mark it deleted.
    Delete { target: Uuid },
    /// A consumed side-channel (avatar/read-receipt) — nothing to show.
    Drop,
}

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

/// What peers should see as our name: the locally stored display name when
/// set, otherwise the username. Fills `sender_display_name_override`.
fn current_display_name() -> Option<String> {
    current_user_id()
        .and_then(crate::state::profile_store::load_display_name)
        .or_else(current_username)
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
    static MSG_SERVICE: RefCell<Option<MessageService>> = const { RefCell::new(None) };
    static TYPING_STATE: RefCell<Option<crate::state::typing::TypingState>> = const { RefCell::new(None) };
    static NOTIFICATIONS: RefCell<Option<crate::state::notifications::NotificationsState>> = const { RefCell::new(None) };
}

/// Toast notifications, for code paths outside the leptos owner.
pub fn notifications_handle() -> Option<crate::state::notifications::NotificationsState> {
    NOTIFICATIONS.with(|c| c.borrow().clone())
}

/// Surface a user-meaningful toast for a failed send. Currently the only case
/// worth telling the user about explicitly is messaging a suspended peer (the
/// server rejects it for direct chats).
fn notify_send_error(err: &messenger_core::api::ApiError) {
    if err.error_code() == Some("ERR_RECIPIENT_SUSPENDED") {
        if let Some(nf) = notifications_handle() {
            nf.push(
                crate::state::notifications::ToastKind::Error,
                crate::i18n::tr("chat.recipientSuspended"),
            );
        }
    }
}

/// The globally registered `MessageService`, for code paths that run outside
/// the leptos owner (background sync loop) where `use_context` returns None.
#[must_use]
pub fn service_handle() -> Option<MessageService> {
    MSG_SERVICE.with(|c| c.borrow().clone())
}

/// The globally registered `ChatsState` — same rationale as [`service_handle`].
#[must_use]
pub fn chats_handle() -> Option<crate::state::chats::ChatsState> {
    CHATS_STATE.with(|c| c.borrow().clone())
}

/// Typing-indicator state, for code paths outside the leptos owner (the WS loop).
pub fn typing_handle() -> Option<crate::state::typing::TypingState> {
    TYPING_STATE.with(|c| *c.borrow())
}

/// Publishes a chat's `last_message_at` along with a preview snippet and the
/// message kind so the sidebar can render a Telegram-style preview.
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

// --- Delivery / read markers (localStorage, per group) ---
//
// Receipts are tiny monotone watermarks: for OUR messages we track the last
// id the peer has read (from their MLS ReadReceipt) and the last id the
// server confirmed delivered; for THEIR messages — the last id we already
// acknowledged with our own ReadReceipt. UUIDv7 ordering matches time, so a
// plain `Uuid` comparison gives "everything up to X".

fn marker_get(prefix: &str, group_id: Uuid) -> Option<Uuid> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()?
        .get_item(&format!("{prefix}{group_id}"))
        .ok()
        .flatten()?
        .parse()
        .ok()
}

fn marker_max(prefix: &str, group_id: Uuid, id: Uuid) {
    let current = marker_get(prefix, group_id);
    if current.is_some_and(|c| c >= id) {
        return;
    }
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok()).flatten() {
        let _ = s.set_item(&format!("{prefix}{group_id}"), &id.to_string());
    }
}

const PEER_READ_PREFIX: &str = "messenger_peer_read_";
const DELIVERED_PREFIX: &str = "messenger_delivered_";
const READ_ACKED_PREFIX: &str = "messenger_read_acked_";
/// Highest foreign message id the user has actually seen (chat opened). Drives
/// the sidebar unread badge independently of read-receipt settings.
const SEEN_PREFIX: &str = "messenger_seen_";
/// Highest message id cleared by a chat deletion. The server has no delete
/// endpoint and dedups direct chats to the same group, so without this a
/// "deleted" chat restores its whole history the moment it's re-created. We
/// record `now_v7()` at delete time (greater than every existing UUIDv7 id) and
/// drop anything at or below it when materializing messages — the re-created
/// chat opens empty and only messages sent afterward appear.
const CLEARED_PREFIX: &str = "messenger_cleared_";

/// Update a chat's unread badge through the thread-local `ChatsState`.
fn set_chat_unread(group_id: Uuid, count: u32) {
    CHATS_STATE.with(|c| {
        if let Some(chats) = c.borrow().as_ref() {
            chats.set_unread(group_id, count);
        }
    });
}

// --- Own message bodies / deletes (localStorage) ---
//
// MLS can't decrypt our OWN messages, so on a refresh `convert_one` can't
// recover the content of messages we sent (edits revert to the original text,
// images/files lose their attachment id+key). We cache the rendered body of our
// own messages locally and re-apply it while converting, so our own content
// (text, edits, images, files, videos) survives reloads.
const OWN_MSGS_KEY: &str = "messenger_own_msgs";
const OWN_DELETES_KEY: &str = "messenger_own_deletes";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

type OwnBody = (crate::state::messages::MessageKind, MessageBody);

fn own_msgs_load() -> std::collections::HashMap<Uuid, OwnBody> {
    local_storage()
        .and_then(|s| s.get_item(OWN_MSGS_KEY).ok().flatten())
        .and_then(|j| serde_json::from_str::<Vec<(String, OwnBody)>>(&j).ok())
        .map(|v| {
            v.into_iter()
                .filter_map(|(k, b)| k.parse::<Uuid>().ok().map(|id| (id, b)))
                .collect()
        })
        .unwrap_or_default()
}

fn own_msgs_store(map: &std::collections::HashMap<Uuid, OwnBody>) {
    if let Some(s) = local_storage() {
        let v: Vec<(String, OwnBody)> =
            map.iter().map(|(k, b)| (k.to_string(), b.clone())).collect();
        if let Ok(j) = serde_json::to_string(&v) {
            let _ = s.set_item(OWN_MSGS_KEY, &j);
        }
    }
}

fn own_msgs_record(id: Uuid, kind: crate::state::messages::MessageKind, body: MessageBody) {
    let mut map = own_msgs_load();
    map.insert(id, (kind, body));
    own_msgs_store(&map);
}

fn own_deletes_load() -> std::collections::HashSet<Uuid> {
    local_storage()
        .and_then(|s| s.get_item(OWN_DELETES_KEY).ok().flatten())
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
        .map(|v| v.into_iter().filter_map(|k| k.parse::<Uuid>().ok()).collect())
        .unwrap_or_default()
}

fn own_deletes_store(set: &std::collections::HashSet<Uuid>) {
    if let Some(s) = local_storage() {
        let v: Vec<String> = set.iter().map(ToString::to_string).collect();
        if let Ok(j) = serde_json::to_string(&v) {
            let _ = s.set_item(OWN_DELETES_KEY, &j);
        }
    }
}

fn own_deletes_record(id: Uuid) {
    let mut set = own_deletes_load();
    if set.insert(id) {
        own_deletes_store(&set);
    }
}

/// Whether the user shares read receipts (privacy setting, default on).
fn read_receipts_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|s| s.get_item("ms_settings_read_receipts").ok().flatten())
        .map_or(true, |v| v == "true")
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
        // Prefer the caption in the sidebar preview when one was sent.
        MessageBody::Voice { caption, .. } | MessageBody::Image { caption, .. } => {
            caption.clone().unwrap_or_default()
        }
        MessageBody::File { name, caption, .. } => {
            caption.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| name.clone())
        }
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
pub fn init_message_service_context(
    session: &Session,
    users: UsersState,
    chats: crate::state::chats::ChatsState,
    svc: MessageService,
    typing: crate::state::typing::TypingState,
    notifications: crate::state::notifications::NotificationsState,
) {
    SESSION_STATE.with(|c| *c.borrow_mut() = Some(session.state));
    USERS_STATE.with(|c| *c.borrow_mut() = Some(users));
    CHATS_STATE.with(|c| *c.borrow_mut() = Some(chats));
    MSG_SERVICE.with(|c| *c.borrow_mut() = Some(svc));
    TYPING_STATE.with(|c| *c.borrow_mut() = Some(typing));
    NOTIFICATIONS.with(|c| *c.borrow_mut() = Some(notifications));
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

    /// Permanently clear a chat's conversation locally.
    ///
    /// Records a "cleared" watermark (`now_v7()`, above every existing message
    /// id) so the server-deduped group re-opens empty when the chat is created
    /// again, drops the in-memory messages, and forgets cached own-message
    /// bodies so nothing of the old conversation can be re-applied. The server
    /// keeps the encrypted blobs (no delete endpoint), but they're filtered out
    /// on every future materialization.
    pub fn clear_conversation(&self, group_id: Uuid) {
        let cleared_after = Uuid::now_v7();
        marker_max(CLEARED_PREFIX, group_id, cleared_after);
        // Forget own-message bodies for this group's messages (those whose id is
        // at/below the watermark) so the cache can't resurrect cleared content.
        let stale: Vec<Uuid> = self
            .messages
            .by_group
            .with_untracked(|m| {
                m.get(&group_id)
                    .map(|list| list.iter().map(|d| d.id).collect())
                    .unwrap_or_default()
            });
        if !stale.is_empty() {
            let mut own = own_msgs_load();
            let mut deletes = own_deletes_load();
            let before = (own.len(), deletes.len());
            own.retain(|id, _| !stale.contains(id));
            deletes.retain(|id| !stale.contains(id));
            if (own.len(), deletes.len()) != before {
                own_msgs_store(&own);
                own_deletes_store(&deletes);
            }
        }
        self.messages.by_group.update(|m| {
            m.remove(&group_id);
        });
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
                // Opening a chat marks everything up to the newest foreign
                // message as seen and clears the unread badge.
                self.mark_seen(group_id);
                self.apply_delivery_markers(group_id);
                self.refresh_delivery_status(group_id);
                self.acknowledge_read(group_id);
                tracing::debug!(%group_id, count = resp.messages.len(), "messages loaded");
            }
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "failed to load messages");
            }
        }
    }

    /// Refresh a chat's sidebar state when a message arrives while the chat is
    /// NOT open: pull the latest content, update the preview/timestamp and the
    /// unread badge — without acknowledging read (that only happens on open).
    ///
    /// This is what makes "собеседник написал" show up in the chat list
    /// (preview + unread + reordering) without opening the conversation.
    pub async fn refresh_incoming(&self, group_id: Uuid) {
        let Some(api) = build_api_client() else { return };
        // Cheap incremental probe: ask only for messages newer than the newest
        // one we already hold. The background sync runs this for every chat on
        // a timer, so on an idle chat (the common case) this returns nothing and
        // we skip re-pulling + MLS-decrypting the entire history. New activity
        // (including edit/delete control messages, which are new rows with newer
        // ids) trips the probe and falls through to the full reconcile below —
        // needed so an edit/delete targeting an older message still applies.
        let last_id = self
            .messages
            .by_group
            .with_untracked(|map| map.get(&group_id).and_then(|l| l.iter().map(|m| m.id).max()));
        if last_id.is_some() {
            match api.list_messages(group_id, last_id, Some(1)).await {
                Ok(r) if r.messages.is_empty() => return, // nothing new — done
                Ok(_) => {}                               // new activity → reconcile
                Err(e) => {
                    tracing::warn!(%group_id, error = %e, "refresh_incoming: probe failed");
                    return;
                }
            }
        }
        let resp = match api.list_messages(group_id, None, None).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "refresh_incoming: list failed");
                return;
            }
        };
        let display_messages = self.convert_messages(&resp.messages, group_id).await;
        let latest = display_messages.iter().max_by_key(|m| m.created_at).cloned();
        // `by_group` is one shared signal read by every open chat via `.get()`,
        // so even a no-op insert for a background chat would notify (and rebuild)
        // the currently open chat's list. Only write when something changed.
        let changed = self
            .messages
            .by_group
            .with_untracked(|map| map.get(&group_id) != Some(&display_messages));
        if changed {
            self.messages.by_group.update(|map| {
                map.insert(group_id, display_messages);
            });
        }
        if let Some(m) = latest {
            set_chat_last_message(
                group_id,
                m.created_at * 1000,
                Some(preview_from_body(&m.body)),
                Some(m.kind),
            );
        }
        // Delivery checkmarks only matter for the open chat (refreshed by
        // load_messages on open). Skipping them here keeps this background
        // refresh from churning by_group — and the open chat — every cycle.
        self.recompute_unread(group_id);
    }

    /// Set the SEEN watermark to the newest foreign message and clear unread.
    /// Called when the user opens a chat.
    fn mark_seen(&self, group_id: Uuid) {
        let me = current_user_id();
        let newest_foreign = self.messages.by_group.with_untracked(|map| {
            map.get(&group_id).and_then(|list| {
                list.iter()
                    .filter(|m| Some(m.sender_user_id) != me && !m.sender_user_id.is_nil())
                    .map(|m| m.id)
                    .max()
            })
        });
        if let Some(up_to) = newest_foreign {
            marker_max(SEEN_PREFIX, group_id, up_to);
        }
        set_chat_unread(group_id, 0);
    }

    /// Recompute the unread badge = foreign messages newer than the SEEN
    /// watermark. Skipped (and cleared) for the currently open chat.
    fn recompute_unread(&self, group_id: Uuid) {
        let is_open = chats_handle()
            .map(|c| c.selected.get_untracked() == Some(group_id))
            .unwrap_or(false);
        if is_open {
            self.mark_seen(group_id);
            return;
        }
        let me = current_user_id();
        let seen = marker_get(SEEN_PREFIX, group_id);
        let count = self.messages.by_group.with_untracked(|map| {
            map.get(&group_id).map_or(0, |list| {
                list.iter()
                    .filter(|m| {
                        Some(m.sender_user_id) != me
                            && !m.sender_user_id.is_nil()
                            && seen.map_or(true, |s| m.id > s)
                    })
                    .count() as u32
            })
        });
        set_chat_unread(group_id, count);
    }

    /// Re-stamp own messages with Read / DeliveredToAll based on the stored
    /// watermarks (peer's ReadReceipt and server delivery confirmations).
    fn apply_delivery_markers(&self, group_id: Uuid) {
        let Some(me) = current_user_id() else { return };
        let peer_read = marker_get(PEER_READ_PREFIX, group_id);
        let delivered = marker_get(DELIVERED_PREFIX, group_id);
        if peer_read.is_none() && delivered.is_none() {
            return;
        }
        self.messages.by_group.update(|map| {
            if let Some(list) = map.get_mut(&group_id) {
                for m in list.iter_mut() {
                    // Locally echoed own messages carry a nil sender id.
                    if !(m.sender_user_id == me || m.sender_user_id.is_nil()) {
                        continue;
                    }
                    if peer_read.is_some_and(|r| m.id <= r) {
                        m.delivery_status = DeliveryStatus::Read;
                    } else if delivered.is_some_and(|d| m.id <= d)
                        && m.delivery_status == DeliveryStatus::SentToServer
                    {
                        m.delivery_status = DeliveryStatus::DeliveredToAll;
                    }
                }
            }
        });
    }

    /// Ask the server whether our newest message reached the peer's devices
    /// and advance the delivered watermark. One request per chat open.
    fn refresh_delivery_status(&self, group_id: Uuid) {
        let Some(me) = current_user_id() else { return };
        let last_own = self.messages.by_group.with_untracked(|map| {
            map.get(&group_id).and_then(|list| {
                list.iter()
                    .filter(|m| m.sender_user_id == me && !m.id.is_nil())
                    .map(|m| m.id)
                    .max()
            })
        });
        let Some(last_own) = last_own else { return };
        if marker_get(DELIVERED_PREFIX, group_id).is_some_and(|d| d >= last_own) {
            return;
        }
        let svc = self.clone();
        spawn_local(async move {
            let Some(api) = build_api_client() else { return };
            if let Ok(status) = api.message_delivery(last_own).await {
                if status.delivered_count > 0 {
                    marker_max(DELIVERED_PREFIX, group_id, last_own);
                    svc.apply_delivery_markers(group_id);
                }
            }
        });
    }

    /// Tell the peer we've seen their messages (MLS ReadReceipt envelope) —
    /// only when the privacy setting allows it and only for ids we haven't
    /// acknowledged yet. Side-channel: never rendered, never in previews.
    fn acknowledge_read(&self, group_id: Uuid) {
        if !read_receipts_enabled() {
            return;
        }
        let Some(me) = current_user_id() else { return };
        let last_foreign = self.messages.by_group.with_untracked(|map| {
            map.get(&group_id).and_then(|list| {
                list.iter()
                    .filter(|m| m.sender_user_id != me && !m.sender_user_id.is_nil())
                    .map(|m| m.id)
                    .max()
            })
        });
        let Some(up_to) = last_foreign else { return };
        if marker_get(READ_ACKED_PREFIX, group_id).is_some_and(|a| a >= up_to) {
            return;
        }
        let svc = self.clone();
        spawn_local(async move {
            let Some(api) = build_api_client() else { return };
            let client_message_id = Uuid::now_v7();
            let envelope = ApplicationEnvelope {
                client_message_id,
                kind: AppMessageKind::ReadReceipt,
                body: AppMessageBody::ReadReceipt {
                    up_to_message_id: up_to,
                    at: js_sys::Date::now() as i64 / 1000,
                },
                reply_to_message_id: None,
                thread_root_id: None,
                created_at: js_sys::Date::now() as i64 / 1000,
                sender_display_name_override: current_display_name(),
            };
            let envelope_ct = match svc.encrypt_envelope(group_id, &envelope).await {
                Some(ct) => ct,
                None => match rmp_serde::to_vec_named(&envelope) {
                    Ok(plain) => plain,
                    Err(_) => return,
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
            if api.post_message(group_id, &req).await.is_ok() {
                marker_max(READ_ACKED_PREFIX, group_id, up_to);
            }
        });
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
        let me = current_display_name();

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
                own_msgs_record(
                    resp.message_id,
                    MessageKind::Text,
                    MessageBody::Text(text.to_string()),
                );
                Some(resp.message_id)
            }
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "failed to send message");
                notify_send_error(&e);
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
        let me = current_display_name();
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::Voice,
            body: AppMessageBody::Voice {
                attachment_id: upload.attachment_id,
                decryption_key: key.to_vec(),
                duration_ms: payload.duration_ms,
                waveform: payload.waveform.clone(),
                caption: None,
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
                notify_send_error(&e);
                return None;
            }
        };

        // 4. Finalize binds attachment to the message — otherwise GC will reap it
        //    and recipients get 403 on download until it lands.
        finalize_attachment_retrying(&api, upload.attachment_id, resp.message_id).await;

        // 5. Optimistic local insert.
        let voice_body = MessageBody::Voice {
            attachment_id: upload.attachment_id,
            decryption_key: key.to_vec(),
            duration_ms: payload.duration_ms,
            waveform: payload.waveform,
            transcription: None,
            caption: None,
        };
        // Cache our own voice body so it survives refreshes (no MLS self-decrypt).
        own_msgs_record(resp.message_id, MessageKind::Voice, voice_body.clone());
        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(DisplayMessage {
                id: resp.message_id,
                client_message_id,
                group_id,
                sender_user_id: Uuid::nil(),
                sender_device_id: Uuid::nil(),
                sender_display_name: me.clone(),
                kind: MessageKind::Voice,
                body: voice_body,
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

        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000;
        let me = current_display_name();
        let kind_for_display = if payload.is_image { MessageKind::Image } else { MessageKind::File };

        // Optimistic "sending" bubble shown immediately, before the (possibly
        // slow) upload — otherwise a large file/video would upload with zero
        // feedback and look like it failed. Reconciled to the real message on
        // success, or marked Failed on error. The placeholder body carries no
        // attachment id yet; the image renderer stays in its spinner for nil.
        let caption = payload.caption.clone().filter(|c| !c.trim().is_empty());
        let sending_body = if payload.is_image {
            MessageBody::Image {
                attachment_id: Uuid::nil(),
                decryption_key: Vec::new(),
                mime: payload.mime.clone(),
                width: 0,
                height: 0,
                thumb: None,
                caption: caption.clone(),
            }
        } else {
            MessageBody::File {
                attachment_id: Uuid::nil(),
                decryption_key: Vec::new(),
                mime: payload.mime.clone(),
                name: payload.name.clone(),
                size: payload.size,
                caption: caption.clone(),
            }
        };
        self.messages.by_group.update(|map| {
            map.entry(group_id).or_default().push(DisplayMessage {
                id: client_message_id,
                client_message_id,
                group_id,
                sender_user_id: Uuid::nil(),
                sender_device_id: Uuid::nil(),
                sender_display_name: me.clone(),
                kind: kind_for_display,
                body: sending_body.clone(),
                reply_to_message_id: None,
                thread_root_id: None,
                created_at: now,
                edited_at: None,
                deleted_at: None,
                delivery_status: DeliveryStatus::Sending,
                reactions: Vec::new(),
            });
        });
        set_chat_last_message(group_id, now * 1000, Some(preview_from_body(&sending_body)), Some(kind_for_display));

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        let ciphertext = match encrypt_attachment(&key, &payload.bytes) {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("attachment encrypt failed: {e:?}");
                self.mark_attachment_failed(group_id, client_message_id);
                return None;
            }
        };
        let padded_size = ciphertext.len() as u64;
        let size_bucket = size_bucket_for(padded_size);

        let upload = match api.upload_attachment(ciphertext, padded_size, size_bucket).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("attachment upload failed: {e}");
                self.mark_attachment_failed(group_id, client_message_id);
                return None;
            }
        };

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
                    caption: caption.clone(),
                },
                MessageBody::Image {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: payload.mime.clone(),
                    width: 0,
                    height: 0,
                    thumb: None,
                    caption: caption.clone(),
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
                    caption: caption.clone(),
                },
                MessageBody::File {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: payload.mime.clone(),
                    name: payload.name.clone(),
                    size: payload.size,
                    caption: caption.clone(),
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
                    self.mark_attachment_failed(group_id, client_message_id);
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
                notify_send_error(&e);
                self.mark_attachment_failed(group_id, client_message_id);
                return None;
            }
        };
        finalize_attachment_retrying(&api, upload.attachment_id, resp.message_id).await;

        // Reconcile the optimistic bubble in place: real id, real body (with the
        // attachment id), and a delivered/sent status replacing the spinner.
        let preview = Some(preview_from_body(&local_body));
        // Cache our own attachment body so it survives refreshes — MLS can't
        // decrypt our own message to recover the attachment id+key.
        own_msgs_record(resp.message_id, kind_for_display, local_body.clone());
        self.messages.by_group.update(|map| {
            if let Some(list) = map.get_mut(&group_id) {
                if let Some(m) = list.iter_mut().find(|m| m.client_message_id == client_message_id) {
                    m.id = resp.message_id;
                    m.body = local_body;
                    m.delivery_status = DeliveryStatus::SentToServer;
                }
            }
        });
        set_chat_last_message(group_id, now * 1000, preview, Some(kind_for_display));

        Some(resp.message_id)
    }

    /// Forward an existing message into another group.
    ///
    /// The message is re-sent as a brand-new message authored by us (no "via"
    /// chain — the simplest, privacy-preserving model). Text is re-sent as
    /// text; media is re-downloaded from the server, decrypted with the
    /// original per-attachment key, then re-encrypted with a fresh key and
    /// re-uploaded to the target group (attachments are scoped to their
    /// message, so the target group must hold its own copy).
    ///
    /// Returns the new message id, or `None` if the source can't be found or
    /// re-sending failed (e.g. the original attachment bytes were GC'd).
    pub async fn forward_to(&self, target_group: Uuid, source_group: Uuid, message_id: Uuid) -> Option<Uuid> {
        // Snapshot the source body — the message lives in the in-memory store.
        let body = {
            let map = self.messages.by_group.get_untracked();
            map.get(&source_group)
                .and_then(|list| list.iter().find(|m| m.id == message_id))
                .map(|m| m.body.clone())
        }?;

        // Pull the encrypted attachment bytes for a media body and decrypt them
        // with the original key, yielding the plaintext to re-upload.
        async fn fetch_plain(attachment_id: Uuid, key: &[u8]) -> Option<Vec<u8>> {
            let key_arr: [u8; 32] = key.try_into().ok()?;
            let api = build_api_client()?;
            let ct = api.download_attachment(attachment_id, None).await.ok()?;
            messenger_core::attachment_crypto::decrypt_attachment(&key_arr, &ct).ok()
        }

        match body {
            MessageBody::Text(text) => self.send_text(target_group, &text, None).await,
            MessageBody::System { .. } => None, // system events aren't forwardable
            MessageBody::Image { attachment_id, decryption_key, mime, caption, .. } => {
                let bytes = fetch_plain(attachment_id, &decryption_key).await?;
                let size = bytes.len() as u64;
                self.send_attachment(
                    target_group,
                    crate::chat::input_bar::AttachmentPayload {
                        bytes,
                        mime,
                        name: "image".to_string(),
                        size,
                        is_image: true,
                        caption,
                    },
                )
                .await
            }
            MessageBody::File { attachment_id, decryption_key, mime, name, caption, .. } => {
                let bytes = fetch_plain(attachment_id, &decryption_key).await?;
                let size = bytes.len() as u64;
                self.send_attachment(
                    target_group,
                    crate::chat::input_bar::AttachmentPayload { bytes, mime, name, size, is_image: false, caption },
                )
                .await
            }
            MessageBody::Voice { attachment_id, decryption_key, duration_ms, waveform, .. } => {
                let bytes = fetch_plain(attachment_id, &decryption_key).await?;
                self.send_voice(
                    target_group,
                    crate::chat::input_bar::VoicePayload {
                        bytes,
                        mime: "audio/webm".to_string(),
                        duration_ms,
                        waveform,
                    },
                )
                .await
            }
        }
    }

    /// Mark an in-flight optimistic attachment message as failed to send, so the
    /// bubble shows an error icon instead of spinning forever.
    fn mark_attachment_failed(&self, group_id: Uuid, client_message_id: Uuid) {
        self.messages.by_group.update(|map| {
            if let Some(list) = map.get_mut(&group_id) {
                if let Some(m) = list.iter_mut().find(|m| m.client_message_id == client_message_id) {
                    m.delivery_status = DeliveryStatus::Failed;
                }
            }
        });
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
            sender_display_name_override: current_display_name(),
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
            finalize_attachment_retrying(&api, attachment_id, resp.message_id).await;
        }
        crate::state::avatar_store::mark_announced(
            group_id,
            &crate::state::avatar_store::avatar_fingerprint(me_id),
        );
        true
    }

    /// Make sure every group has received the current avatar — the safety net
    /// behind the event-driven broadcasts (settings change, chat creation,
    /// welcome join), which all miss chats created after the avatar was set
    /// or joined without MLS. Cheap when nothing changed; called from the
    /// sync loop.
    pub async fn ensure_avatar_broadcasts(&self) {
        let Some(me_id) = current_user_id() else {
            web_sys::console::log_1(&"[avatar] ensure: no user".into());
            return;
        };
        let fingerprint = crate::state::avatar_store::avatar_fingerprint(me_id);
        let announced = crate::state::avatar_store::announced_map();

        // The sync loop refreshes CHATS_STATE right before calling us, so
        // the in-memory list is the primary source; ask the server only when
        // it is still empty (e.g. settings opened straight after app start).
        let mut groups: Vec<Uuid> = chats_handle()
            .map(|cs| cs.chats.get_untracked().iter().map(|ch| ch.group_id).collect())
            .unwrap_or_default();
        if groups.is_empty() {
            let Some(api) = build_api_client() else { return };
            match api.list_groups(None).await {
                Ok(resp) => groups = resp.groups.iter().map(|g| g.id).collect(),
                Err(e) => {
                    web_sys::console::log_1(
                        &format!("[avatar] ensure: list_groups failed: {e}").into(),
                    );
                    return;
                }
            }
        }
        for group_id in groups {
            let prev = announced.get(&group_id).map(String::as_str);
            if prev == Some(fingerprint.as_str()) {
                continue;
            }
            // Never-announced group + no avatar: nothing to deliver, don't
            // spam removal notices.
            if prev.is_none() && fingerprint == "none" {
                continue;
            }
            let ok = self.broadcast_avatar(group_id).await;
            web_sys::console::log_1(
                &format!("[avatar] ensure: re-announce to {group_id}: {ok}").into(),
            );
        }
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
            sender_display_name_override: current_display_name(),
        };

        // On the MLS-not-ready fallback, send the SERIALIZED envelope (not the
        // raw text) — otherwise the EditNotice structure is lost and the edit
        // arrives as a plain new message instead of replacing the original.
        let ciphertext = match self.encrypt_envelope(group_id, &envelope).await {
            Some(ct) => ct,
            None => match rmp_serde::to_vec_named(&envelope) {
                Ok(plain) => plain,
                Err(e) => {
                    tracing::warn!(error = %e, "edit envelope serialize failed");
                    return None;
                }
            },
        };

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

        // Remember our own edit so it survives refreshes — we can't decrypt our
        // own EditNotice to re-derive it from the server.
        let new_text = new_text.to_string();
        own_msgs_record(
            original_id,
            crate::state::messages::MessageKind::Text,
            MessageBody::Text(new_text.clone()),
        );

        // Optimistic: reflect the edit on the original bubble right away.
        self.messages.by_group.update(|map| {
            if let Some(list) = map.get_mut(&group_id) {
                if let Some(m) = list.iter_mut().find(|m| m.id == original_id) {
                    m.body = MessageBody::Text(new_text);
                    m.edited_at = Some(now);
                }
            }
        });

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
            sender_display_name_override: current_display_name(),
        };

        // Fallback (MLS not ready): send the serialized envelope so the
        // DeleteNotice survives instead of becoming an empty/plain message.
        let ciphertext = match self.encrypt_envelope(group_id, &envelope).await {
            Some(ct) => ct,
            None => match rmp_serde::to_vec_named(&envelope) {
                Ok(plain) => plain,
                Err(e) => {
                    tracing::warn!(error = %e, "delete envelope serialize failed");
                    return false;
                }
            },
        };

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
                // Remember our own delete so it survives refreshes (we can't
                // decrypt our own DeleteNotice).
                own_deletes_record(message_id);
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
        use std::collections::{HashMap, HashSet};
        let mut result = Vec::with_capacity(stored.len());
        // Control messages (edit/delete) are consumed, not shown. Messages
        // arrive in chronological order, so the last edit for an id wins.
        let mut edits: HashMap<Uuid, (String, i64)> = HashMap::new();
        let mut deletes: HashSet<Uuid> = HashSet::new();
        for msg in stored {
            if msg.wire_format != "application" {
                continue;
            }
            match self.convert_one(msg, group_id).await {
                Converted::Show(dm) => result.push(dm),
                Converted::Edit { original, new_text, at } => {
                    edits.insert(original, (new_text, at));
                }
                Converted::Delete { target } => {
                    deletes.insert(target);
                }
                Converted::Drop => {}
            }
        }
        // Our own messages can't be recovered from MLS on refresh (no
        // self-decrypt), so re-apply their cached body, our edits, and deletes.
        let own_msgs = own_msgs_load();
        let own_deletes = own_deletes_load();

        for m in &mut result {
            // Own message body cache wins (image/file/video attachment data,
            // own text) — but an edit on top of it still applies below.
            if let Some((kind, body)) = own_msgs.get(&m.id) {
                m.kind = *kind;
                m.body = body.clone();
            }
            // Edit: from our local record (own edit) or a peer's EditNotice.
            if let Some((text, at)) = edits.get(&m.id) {
                m.body = MessageBody::Text(text.clone());
                if m.edited_at.is_none() {
                    m.edited_at = Some(*at);
                }
            }
            if (deletes.contains(&m.id) || own_deletes.contains(&m.id)) && m.deleted_at.is_none() {
                m.deleted_at = Some(m.created_at);
            }
        }
        // Drop everything cleared by a prior chat deletion so a re-created
        // (server-deduped) chat starts empty instead of restoring its history.
        if let Some(cleared) = marker_get(CLEARED_PREFIX, group_id) {
            result.retain(|m| m.id > cleared);
        }
        result
    }

    /// Convert a single stored message, attempting MLS decryption.
    ///
    /// Side-channel/control messages (`AvatarUpdate`, `ReadReceipt`,
    /// `EditNotice`, `DeleteNotice`) are consumed and never shown directly —
    /// edits/deletes are applied to the target message by the caller.
    async fn convert_one(
        &self,
        msg: &messenger_proto::mls::StoredMessage,
        group_id: Uuid,
    ) -> Converted {
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
                        return Converted::Drop;
                    }
                    // Read-receipt side-channel: remember how far the peer
                    // has read so our own bubbles turn blue; never shown.
                    if let AppMessageBody::ReadReceipt { up_to_message_id, .. } = envelope.body {
                        if Some(msg.sender_user_id) != current_user_id() {
                            marker_max(PEER_READ_PREFIX, group_id, up_to_message_id);
                        }
                        return Converted::Drop;
                    }
                    // Edit: carries the new text for an earlier message. Consumed
                    // here and applied to the original by the caller, so it never
                    // shows as a separate message.
                    if let AppMessageBody::EditNotice {
                        original_message_id,
                        ref new_text,
                    } = envelope.body
                    {
                        return Converted::Edit {
                            original: original_message_id,
                            new_text: new_text.clone(),
                            at: envelope.created_at,
                        };
                    }
                    // Delete: marks an earlier message deleted; also consumed.
                    if let AppMessageBody::DeleteNotice { target_message_id } = envelope.body {
                        return Converted::Delete {
                            target: target_message_id,
                        };
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

        Converted::Show(DisplayMessage {
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
            // Keep the chat label in sync with the peer's latest name from
            // envelope overrides: covers both the UUID placeholder a welcome
            // recipient starts with and later display-name changes.
            if let (Some(cs), Some(name)) = (
                CHATS_STATE.with(|c| c.borrow().clone()),
                users.get(sender),
            ) {
                let outdated = cs
                    .display_name_cache
                    .get_untracked()
                    .get(&group_id)
                    .is_none_or(|n| n != &name);
                if outdated {
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
                ref caption,
            } => (
                MessageKind::Voice,
                MessageBody::Voice {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    duration_ms,
                    waveform: waveform.clone(),
                    transcription: None,
                    caption: caption.clone(),
                },
            ),
            AppMessageBody::File {
                attachment_id,
                ref decryption_key,
                ref mime,
                ref filename,
                size,
                ref caption,
            } => (
                MessageKind::File,
                MessageBody::File {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    mime: mime.clone(),
                    name: filename.clone(),
                    size,
                    caption: caption.clone(),
                },
            ),
            AppMessageBody::Image {
                attachment_id,
                ref decryption_key,
                ref mime,
                width,
                height,
                ref thumb,
                ref caption,
            } => (
                MessageKind::Image,
                MessageBody::Image {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    mime: mime.clone(),
                    width,
                    height,
                    thumb: thumb.clone(),
                    caption: caption.clone(),
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

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
    /// A reaction (add/remove an emoji) on an earlier message.
    Reaction {
        target: Uuid,
        emoji: String,
        add: bool,
        sender: Uuid,
    },
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
pub fn current_user_id() -> Option<Uuid> {
    SESSION_STATE.with(|c| c.borrow().as_ref().and_then(|s| {
        match s.get_untracked() {
            SessionState::Authenticated { identity, .. } => Some(identity.user_id),
            _ => None,
        }
    }))
}

/// The current device id, or `None` when not authenticated.
pub fn current_device_id() -> Option<Uuid> {
    SESSION_STATE.with(|c| c.borrow().as_ref().and_then(|s| {
        match s.get_untracked() {
            SessionState::Authenticated { identity, .. } => Some(identity.device_id),
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
    /// The AvatarUpdate message id whose avatar is currently shown, per sender.
    /// Guards against re-downloading on every chat load (which flickered) and
    /// against an OLDER update overwriting a newer one (UUIDv7 is time-ordered).
    static AVATAR_APPLIED: RefCell<std::collections::HashMap<Uuid, Uuid>> = RefCell::new(std::collections::HashMap::new());
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

thread_local! {
    /// Reason for the most recent encrypt failure, surfaced in the toast.
    static LAST_ENCRYPT_ERR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Surface a toast when a message can't be MLS-encrypted. We NEVER fall back to
/// sending a plaintext envelope, so the user is told the send was dropped.
fn notify_encrypt_failure() {
    let detail = LAST_ENCRYPT_ERR.with(|c| c.borrow().clone());
    let msg = if detail.is_empty() {
        "Сообщение не отправлено: сквозное шифрование недоступно".to_string()
    } else {
        format!("Не отправлено (шифрование): {detail}")
    };
    if let Some(nf) = notifications_handle() {
        nf.push(crate::state::notifications::ToastKind::Error, msg);
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

/// The globally registered `UsersState` — same rationale as [`service_handle`].
#[must_use]
pub fn users_handle() -> Option<crate::state::users::UsersState> {
    USERS_STATE.with(|c| c.borrow().clone())
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

/// Decrypted plaintext of a RECEIVED MLS message, cached per message id. MLS
/// application messages can be decrypted only once (the secret is deleted for
/// forward secrecy), but the app re-pulls and re-converts the whole history on
/// every sync/reload — re-decryption fails with `SecretReuseError`. Caching the
/// plaintext lets re-converts skip decryption. Same on-device trust as the MLS
/// state itself.
fn decrypted_get(id: Uuid) -> Option<Vec<u8>> {
    use base64::Engine as _;
    local_storage()
        .and_then(|s| s.get_item(&format!("mdec:{id}")).ok().flatten())
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
}

fn decrypted_put(id: Uuid, plaintext: &[u8]) {
    use base64::Engine as _;
    if let Some(s) = local_storage() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(plaintext);
        let _ = s.set_item(&format!("mdec:{id}"), &b64);
    }
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

// Reactions are E2E control messages, but the sender can't self-decrypt their
// own Reaction envelope — so own reactions are re-applied from this cache on
// reload (message_id -> emojis this device reacted with).
const OWN_REACTIONS_KEY: &str = "messenger_own_reactions";
// Message ids of our own Reaction envelopes: undecryptable to us, so we drop
// them from the timeline instead of rendering ciphertext garbage.
const OWN_REACTION_MSGS_KEY: &str = "messenger_own_reaction_msgs";

fn own_reactions_load() -> std::collections::HashMap<Uuid, std::collections::HashSet<String>> {
    local_storage()
        .and_then(|s| s.get_item(OWN_REACTIONS_KEY).ok().flatten())
        .and_then(|j| serde_json::from_str::<Vec<(String, Vec<String>)>>(&j).ok())
        .map(|v| {
            v.into_iter()
                .filter_map(|(k, e)| k.parse::<Uuid>().ok().map(|id| (id, e.into_iter().collect())))
                .collect()
        })
        .unwrap_or_default()
}

fn own_reactions_set(msg_id: Uuid, emoji: &str, add: bool) {
    let mut map = own_reactions_load();
    let entry = map.entry(msg_id).or_default();
    if add {
        entry.insert(emoji.to_string());
    } else {
        entry.remove(emoji);
    }
    if entry.is_empty() {
        map.remove(&msg_id);
    }
    if let Some(s) = local_storage() {
        let v: Vec<(String, Vec<String>)> = map
            .iter()
            .map(|(k, e)| (k.to_string(), e.iter().cloned().collect()))
            .collect();
        if let Ok(j) = serde_json::to_string(&v) {
            let _ = s.set_item(OWN_REACTIONS_KEY, &j);
        }
    }
}

fn own_reaction_msgs_load() -> std::collections::HashSet<Uuid> {
    local_storage()
        .and_then(|s| s.get_item(OWN_REACTION_MSGS_KEY).ok().flatten())
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
        .map(|v| v.into_iter().filter_map(|k| k.parse::<Uuid>().ok()).collect())
        .unwrap_or_default()
}

fn own_reaction_msgs_record(id: Uuid) {
    let mut set = own_reaction_msgs_load();
    if set.insert(id) {
        if let Some(s) = local_storage() {
            let v: Vec<String> = set.iter().map(ToString::to_string).collect();
            if let Ok(j) = serde_json::to_string(&v) {
                let _ = s.set_item(OWN_REACTION_MSGS_KEY, &j);
            }
        }
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

/// Establish a real MLS group on the client and register it server-side.
///
/// Shared by group creation and (new) direct chats. Claims a KeyPackage for
/// every active device of every member, builds the MLS group locally with the
/// creator as founder, and posts the commit + per-device welcomes to
/// `POST /v1/groups`. Members join later via the existing welcome path
/// (`list_welcomes` → `join_via_welcome` → `ack`).
///
/// The local MLS GroupId equals the server group id (chosen here and stored by
/// the server under it), so encryption and API addressing line up.
///
/// # Errors
///
/// Returns a user-facing message (Russian) on any failure — no group is created.
pub async fn establish_group(
    api: &ApiClient,
    group_type: &str,
    member_user_ids: &[Uuid],
) -> Result<Uuid, String> {
    use messenger_proto::mls::{CreateGroupRequest, MemberDeviceInit, WelcomePayload};

    // Creator identity from the session — works from detached tasks.
    let Some(state) = SESSION_STATE.with(|c| c.borrow().as_ref().copied()) else {
        return Err("сессия не инициализирована".into());
    };
    let identity = match state.get_untracked() {
        SessionState::Authenticated { identity, .. } => identity,
        _ => return Err("не авторизован".into()),
    };

    let group_id = Uuid::now_v7();
    let creator_uid = identity.user_id;
    let creator_did = identity.device_id;

    // member_devices begins with the creator as owner (founder leaf 0).
    let mut member_devices = vec![MemberDeviceInit {
        user_id: creator_uid,
        device_id: creator_did,
        leaf_index: 0,
        role_in_chat: "owner".into(),
    }];

    // Claim a KeyPackage per active device of every (non-creator) member.
    let mut keypackages: Vec<Vec<u8>> = Vec::new();
    let mut recipient_devices: Vec<Uuid> = Vec::new();
    let mut leaf: i32 = 1;
    for &uid in member_user_ids {
        if uid == creator_uid {
            continue;
        }
        let active = api
            .list_user_devices(uid)
            .await
            .map_err(|e| format!("не удалось получить устройства участника: {e}"))?;
        if active.is_empty() {
            return Err("у участника нет доступных устройств".into());
        }
        // Add EVERY active device of the member — each device has its own MLS
        // leaf (per-device signature key), so the chat is readable on all of a
        // user's devices. A device whose KeyPackage can't be claimed is skipped,
        // not fatal; but a member with zero usable devices is an error.
        let mut added_for_user = 0;
        for d in active {
            let Ok(resp) = api.claim_keypackage(uid, d.id).await else {
                continue;
            };
            keypackages.push(resp.key_package);
            recipient_devices.push(d.id);
            member_devices.push(MemberDeviceInit {
                user_id: uid,
                device_id: d.id,
                leaf_index: leaf,
                role_in_chat: "member".into(),
            });
            leaf += 1;
            added_for_user += 1;
        }
        if added_for_user == 0 {
            return Err("у участника нет доступных ключей устройства".into());
        }
    }

    if keypackages.is_empty() {
        return Err("нужен хотя бы один участник".into());
    }

    // Add the creator's OTHER active devices (besides this founder device) so the
    // chat is readable on all of the creator's devices too. Best-effort per
    // device — a device whose KeyPackage can't be claimed is simply left out.
    if let Ok(my_devices) = api.list_user_devices(creator_uid).await {
        for d in my_devices.into_iter().filter(|d| d.id != creator_did) {
            if let Ok(resp) = api.claim_keypackage(creator_uid, d.id).await {
                keypackages.push(resp.key_package);
                recipient_devices.push(d.id);
                member_devices.push(MemberDeviceInit {
                    user_id: creator_uid,
                    device_id: d.id,
                    leaf_index: leaf,
                    role_in_chat: "owner".into(),
                });
                leaf += 1;
            }
        }
    }

    // Build the MLS group locally (creator = founder; members added).
    let Some(rt) = take_mls_runtime().await else {
        return Err("MLS не инициализирован".into());
    };
    let out = rt.create_group(&identity, group_id, &keypackages).await;
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
    let out = out.map_err(|e| format!("создание MLS-группы не удалось: {e}"))?;

    // OpenMLS emits ONE batched welcome covering all added members; replicate it
    // per recipient device so each device gets a welcome row to consume.
    let welcome_blob = out.welcomes.into_iter().next().unwrap_or_default();
    let welcomes: Vec<WelcomePayload> = recipient_devices
        .iter()
        .map(|&rd| WelcomePayload {
            recipient_device_id: rd,
            welcome_ciphertext: welcome_blob.clone(),
        })
        .collect();

    let req = CreateGroupRequest {
        group_id,
        group_type: group_type.to_string(),
        initial_commit: out.initial_commit,
        welcomes,
        member_devices,
    };
    api.create_group(&req)
        .await
        .map_err(|e| format!("регистрация группы не удалась: {e}"))?;

    Ok(group_id)
}

/// Read the authenticated identity from the session (works from detached tasks).
fn session_identity() -> Result<Arc<messenger_core::identity::ClientIdentity>, String> {
    let state = SESSION_STATE
        .with(|c| c.borrow().as_ref().copied())
        .ok_or("сессия не инициализирована")?;
    match state.get_untracked() {
        SessionState::Authenticated { identity, .. } => Ok(identity),
        _ => Err("не авторизован".into()),
    }
}

/// Add a user (all active devices) to an existing group via an MLS commit.
///
/// Claims the new devices' KeyPackages, proposes an Add, posts the commit +
/// per-device welcomes, then merges locally. Server enforces owner/admin-only.
///
/// # Errors
///
/// Returns a user-facing (Russian) message on any failure.
pub async fn group_add_member(api: &ApiClient, group_id: Uuid, username: &str) -> Result<(), String> {
    use messenger_proto::mls::{MemberChange, PostCommitRequest, WelcomePayload};

    let identity = session_identity()?;
    let uid = api
        .lookup_user_by_username(username)
        .await
        .map_err(|_| format!("пользователь {username} не найден"))?
        .user_id;
    let active = api
        .list_user_devices(uid)
        .await
        .map_err(|e| format!("не удалось получить устройства: {e}"))?;
    if active.is_empty() {
        return Err("у участника нет доступных устройств".into());
    }

    // Add every active device of the user (each gets its own per-device MLS
    // leaf). A device whose KeyPackage can't be claimed is skipped; zero usable
    // devices is an error.
    let mut keypackages: Vec<Vec<u8>> = Vec::new();
    let mut devices: Vec<Uuid> = Vec::new();
    for d in &active {
        let Ok(resp) = api.claim_keypackage(uid, d.id).await else {
            continue;
        };
        keypackages.push(resp.key_package);
        devices.push(d.id);
    }
    if keypackages.is_empty() {
        return Err("у участника нет доступных ключей устройства".into());
    }

    let Some(rt) = take_mls_runtime().await else {
        return Err("MLS не инициализирован".into());
    };
    let result = async {
        let pc = rt
            .propose_add(group_id, &identity, &keypackages)
            .await
            .map_err(|e| format!("propose_add: {e}"))?;
        let welcome = pc.welcomes.into_iter().next().unwrap_or_default();
        let welcomes: Vec<WelcomePayload> = devices
            .iter()
            .map(|&d| WelcomePayload {
                recipient_device_id: d,
                welcome_ciphertext: welcome.clone(),
            })
            .collect();
        let member_changes: Vec<MemberChange> = devices
            .iter()
            .map(|&d| MemberChange {
                kind: "add".into(),
                user_id: uid,
                device_id: d,
                leaf_index: None,
                role_in_chat: Some("member".into()),
            })
            .collect();
        let req = PostCommitRequest {
            expected_epoch: pc.epoch as i64,
            commit: pc.commit,
            welcomes,
            member_changes,
        };
        api.post_commit(group_id, &req)
            .await
            .map_err(|e| format!("commit отклонён: {e}"))?;
        rt.merge_pending(group_id)
            .await
            .map_err(|e| format!("merge: {e}"))?;
        Ok::<(), String>(())
    }
    .await;
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
    result
}

/// Remove a user (all their devices) from a group via an MLS commit.
///
/// Targets the user's real tree leaves (looked up via `member_leaves`, never the
/// advisory server `leaf_index`). Server enforces owner/admin-only.
///
/// # Errors
///
/// Returns a user-facing (Russian) message on any failure.
pub async fn group_remove_member(
    api: &ApiClient,
    group_id: Uuid,
    target_uid: Uuid,
) -> Result<(), String> {
    use messenger_proto::mls::{MemberChange, PostCommitRequest};

    let identity = session_identity()?;
    let members = api
        .list_group_members(group_id)
        .await
        .map_err(|e| format!("не удалось получить участников: {e}"))?;
    let target_devices: Vec<Uuid> = members
        .devices
        .iter()
        .filter(|d| d.user_id == target_uid && d.removed_at_epoch.is_none())
        .map(|d| d.device_id)
        .collect();

    let Some(rt) = take_mls_runtime().await else {
        return Err("MLS не инициализирован".into());
    };
    let result = async {
        let leaves = rt
            .member_leaves(group_id)
            .await
            .map_err(|e| format!("leaves: {e}"))?;
        let target_leaves: Vec<u32> = leaves
            .iter()
            .filter(|(_, uid)| *uid == target_uid)
            .map(|(l, _)| *l)
            .collect();
        if target_leaves.is_empty() {
            return Err("участник не найден в группе".into());
        }
        let pc = rt
            .propose_remove(group_id, &identity, &target_leaves)
            .await
            .map_err(|e| format!("propose_remove: {e}"))?;
        let member_changes: Vec<MemberChange> = target_devices
            .iter()
            .map(|&d| MemberChange {
                kind: "remove".into(),
                user_id: target_uid,
                device_id: d,
                leaf_index: None,
                role_in_chat: None,
            })
            .collect();
        let req = PostCommitRequest {
            expected_epoch: pc.epoch as i64,
            commit: pc.commit,
            welcomes: Vec::new(),
            member_changes,
        };
        api.post_commit(group_id, &req)
            .await
            .map_err(|e| format!("commit отклонён: {e}"))?;
        rt.merge_pending(group_id)
            .await
            .map_err(|e| format!("merge: {e}"))?;
        Ok::<(), String>(())
    }
    .await;
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
    result
}

/// Leave a group: remove your own devices via an MLS commit.
///
/// Does NOT merge the pending commit (you're leaving — the local group state is
/// discarded by the caller, which reloads the chat list). If you're the owner,
/// transfer ownership first (`ApiClient::transfer_owner`).
///
/// # Errors
///
/// Returns a user-facing (Russian) message on any failure.
pub async fn group_leave(api: &ApiClient, group_id: Uuid) -> Result<(), String> {
    use messenger_proto::mls::{MemberChange, PostCommitRequest};

    let identity = session_identity()?;
    let me = identity.user_id;
    let members = api
        .list_group_members(group_id)
        .await
        .map_err(|e| format!("не удалось получить участников: {e}"))?;
    let my_devices: Vec<Uuid> = members
        .devices
        .iter()
        .filter(|d| d.user_id == me && d.removed_at_epoch.is_none())
        .map(|d| d.device_id)
        .collect();

    let Some(rt) = take_mls_runtime().await else {
        return Err("MLS не инициализирован".into());
    };
    let result = async {
        let leaves = rt
            .member_leaves(group_id)
            .await
            .map_err(|e| format!("leaves: {e}"))?;
        let my_leaves: Vec<u32> = leaves
            .iter()
            .filter(|(_, uid)| *uid == me)
            .map(|(l, _)| *l)
            .collect();
        if my_leaves.is_empty() {
            return Err("вы не участник этой группы".into());
        }
        let pc = rt
            .propose_remove(group_id, &identity, &my_leaves)
            .await
            .map_err(|e| format!("propose_remove: {e}"))?;
        let member_changes: Vec<MemberChange> = my_devices
            .iter()
            .map(|&d| MemberChange {
                kind: "remove".into(),
                user_id: me,
                device_id: d,
                leaf_index: None,
                role_in_chat: None,
            })
            .collect();
        let req = PostCommitRequest {
            expected_epoch: pc.epoch as i64,
            commit: pc.commit,
            welcomes: Vec::new(),
            member_changes,
        };
        api.post_commit(group_id, &req)
            .await
            .map_err(|e| format!("commit отклонён: {e}"))?;
        // No merge — we're out; the caller forgets the group locally.
        Ok::<(), String>(())
    }
    .await;
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
    result
}

/// Opportunistically rotate our leaf key in a group (MLS self-update) for
/// post-compromise security. Rate-limited to once per 24h per group via
/// localStorage; conflict-safe (on a server epoch conflict the staged commit is
/// discarded and we re-sync on the next pull). No-op for groups without local
/// MLS state. Fire-and-forget from the chat-load path.
pub async fn maybe_rekey_group(api: &ApiClient, group_id: Uuid) {
    use messenger_proto::mls::PostCommitRequest;
    const DAY_MS: f64 = 24.0 * 3600.0 * 1000.0;

    let key = format!("ms_rekey_{group_id}");
    let now = js_sys::Date::now();
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    if let Some(s) = &storage {
        if let Ok(Some(v)) = s.get_item(&key) {
            if v.parse::<f64>().is_ok_and(|last| now - last < DAY_MS) {
                return;
            }
        }
    }

    let Ok(identity) = session_identity() else { return };
    let Some(rt) = take_mls_runtime().await else { return };
    let outcome = async {
        let pc = rt
            .self_update(group_id, &identity)
            .await
            .map_err(|e| format!("{e}"))?;
        let req = PostCommitRequest {
            expected_epoch: pc.epoch as i64,
            commit: pc.commit,
            welcomes: Vec::new(),
            member_changes: Vec::new(),
        };
        match api.post_commit(group_id, &req).await {
            Ok(_) => rt.merge_pending(group_id).await.map_err(|e| format!("{e}")),
            Err(e) => {
                // Epoch conflict / rejection — discard our staged commit so the
                // local group falls back to the last merged epoch and re-syncs.
                let _ = rt.clear_pending_commit(group_id).await;
                Err(format!("rekey rejected: {e}"))
            }
        }
    }
    .await;
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));

    if outcome.is_ok() {
        if let Some(s) = &storage {
            let _ = s.set_item(&key, &now.to_string());
        }
    } else {
        tracing::debug!(%group_id, "rekey skipped/failed");
    }
}

thread_local! {
    /// `Date.now()` of the last KeyPackage pool check, to rate-limit it.
    static LAST_KP_CHECK: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// `Date.now()` of the last owned-group self-heal pass, to rate-limit it.
    static LAST_HEAL_CHECK: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// Group name last re-announced per owned group (group_id → name), so the
    /// owner re-broadcasts a group's name to late joiners exactly once per
    /// change instead of every sync. Session-local: re-announcing once after a
    /// relaunch is cheap and self-correcting.
    static NAME_ANNOUNCED: RefCell<std::collections::HashMap<Uuid, String>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Clear the self-heal rate-limit so the next `heal_owned_groups` call runs
/// immediately instead of waiting out the ~45s window. Used when a WS
/// `KeyChange` tells us a member just added a device — we want the owner to pull
/// it into the group right away, not on the next idle poll.
pub fn reset_heal_rate_limit() {
    LAST_HEAL_CHECK.with(|c| c.set(0.0));
}

/// Self-heal the membership of groups this device OWNS: add any active device of
/// a member that isn't yet in the MLS tree.
///
/// This is what makes a device added to the account AFTER a chat was created
/// (e.g. a 2nd phone provisioned later) actually join existing chats — provision-
/// time retroactive adds are unreliable (the approver may not be locally joined).
/// Owner-only so there's a single healer per group; epoch races just fail the
/// commit and retry next pass. Rate-limited; no-op until MLS is ready.
pub async fn heal_owned_groups(api: &ApiClient) {
    use messenger_proto::mls::{MemberChange, PostCommitRequest, WelcomePayload};
    use std::collections::HashSet;
    const CHECK_INTERVAL_MS: f64 = 45_000.0;

    let now = js_sys::Date::now();
    if now - LAST_HEAL_CHECK.with(std::cell::Cell::get) < CHECK_INTERVAL_MS {
        return;
    }
    LAST_HEAL_CHECK.with(|c| c.set(now));

    let Ok(identity) = session_identity() else { return };
    if !is_mls_initialized() {
        LAST_HEAL_CHECK.with(|c| c.set(0.0));
        return;
    }
    let Ok(groups) = api.list_groups(None).await else { return };

    for g in groups.groups {
        // Only the owner adds members (server enforces this too).
        if g.role_in_chat != "owner" {
            continue;
        }
        let Ok(members) = api.list_group_members(g.id).await else { continue };
        let in_tree: HashSet<Uuid> = members
            .devices
            .iter()
            .filter(|d| d.removed_at_epoch.is_none())
            .map(|d| d.device_id)
            .collect();

        // Find (user, device) pairs missing from the tree WITHOUT claiming yet —
        // claiming consumes a KeyPackage, so it must happen only once we actually
        // hold the runtime (below), or a busy runtime would burn packages for an
        // add we then abandon.
        let mut missing: Vec<(Uuid, Uuid)> = Vec::new();
        for m in &members.members {
            if m.left_at_epoch.is_some() {
                continue;
            }
            let Ok(active) = api.list_user_devices(m.user_id).await else { continue };
            for d in active {
                if !in_tree.contains(&d.id) {
                    missing.push((m.user_id, d.id));
                }
            }
        }
        if missing.is_empty() {
            continue;
        }

        // Acquire the runtime BEFORE claiming. If it's held by another task, bail
        // for this pass (nothing claimed yet → nothing wasted) and retry soon.
        let Some(rt) = take_mls_runtime().await else {
            LAST_HEAL_CHECK.with(|c| c.set(0.0));
            return;
        };
        let result = async {
            // Claim a KeyPackage per missing device now that we hold the runtime.
            // Best-effort: a device we can't claim (e.g. exhausted pool) is skipped.
            let mut keypackages: Vec<Vec<u8>> = Vec::new();
            let mut changes: Vec<MemberChange> = Vec::new();
            let mut recipients: Vec<Uuid> = Vec::new();
            for (uid, did) in &missing {
                if let Ok(resp) = api.claim_keypackage(*uid, *did).await {
                    keypackages.push(resp.key_package);
                    recipients.push(*did);
                    changes.push(MemberChange {
                        kind: "add".into(),
                        user_id: *uid,
                        device_id: *did,
                        leaf_index: None,
                        role_in_chat: Some("member".into()),
                    });
                }
            }
            if keypackages.is_empty() {
                return Ok(());
            }
            // No local MLS state for this group (we own it server-side but never
            // joined locally) -> can't add anyone; skip quietly.
            let pc = rt
                .propose_add(g.id, &identity, &keypackages)
                .await
                .map_err(|e| format!("propose_add: {e}"))?;
            let welcome = pc.welcomes.into_iter().next().unwrap_or_default();
            let welcomes: Vec<WelcomePayload> = recipients
                .iter()
                .map(|&d| WelcomePayload {
                    recipient_device_id: d,
                    welcome_ciphertext: welcome.clone(),
                })
                .collect();
            let req = PostCommitRequest {
                // The server validates against ITS counter, not our local MLS
                // epoch — and the creator's local epoch runs one ahead of the
                // server's recorded `current_epoch` (the initial add-members
                // commit is merged locally but stored as epoch 0 server-side).
                // Use the authoritative value from `list_groups`.
                expected_epoch: g.current_epoch,
                commit: pc.commit,
                welcomes,
                member_changes: changes,
            };
            api.post_commit(g.id, &req)
                .await
                .map_err(|e| format!("post_commit: {e}"))?;
            rt.merge_pending(g.id)
                .await
                .map_err(|e| format!("merge: {e}"))?;
            Ok::<(), String>(())
        }
        .await;
        // On rejection (epoch race / stale local state) drop the pending commit,
        // otherwise it blocks every future propose_add with `PendingCommit`. The
        // next pass retries once incoming commits have caught the epoch up.
        if result.is_err() {
            let _ = rt.clear_pending_commit(g.id).await;
        }
        MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
        if let Err(e) = result {
            tracing::warn!(group = %g.id, "self-heal add skipped: {e}");
        }
    }
}

/// Keep our KeyPackage pool healthy so peers can always start a chat with us.
///
/// Publishes a single reusable **last-resort** KeyPackage once per device (the
/// server reuses it forever, so claims can never exhaust → no
/// `ERR_KEYPACKAGE_EXHAUSTED`) and tops up regular KeyPackages to a target.
/// Generated through the device MLS runtime so the private init keys persist
/// (needed to later join via the welcome that consumed the package). Rate-limited
/// to once per 5 min; safe to call every sync tick. No-op until MLS is ready.
pub async fn ensure_keypackages(api: &ApiClient) {
    use messenger_proto::keypackages::{KeyPackageUpload, PublishKeyPackagesRequest};
    const TARGET: i32 = 6;
    const CHECK_INTERVAL_MS: f64 = 300_000.0;

    let now = js_sys::Date::now();
    if now - LAST_KP_CHECK.with(std::cell::Cell::get) < CHECK_INTERVAL_MS {
        return;
    }
    LAST_KP_CHECK.with(|c| c.set(now));

    let Ok(identity) = session_identity() else { return };
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());

    // One-time reconcile: older builds (and storage/session resets) left
    // KeyPackages on the server whose private bundles this device no longer
    // holds locally. A peer then claims an un-joinable package and fails with
    // `NoMatchingKeyPackage`, so messages never arrive. Purge the server pool
    // once and fall through to republish a fresh, locally-backed batch
    // (clearing the last-resort flag so a fresh last-resort is published too).
    let need_reconcile = storage
        .as_ref()
        .and_then(|s| s.get_item("ms_kp_reconciled_v1").ok().flatten())
        .is_none();
    if need_reconcile {
        let _ = api.delete_my_keypackages().await;
        if let Some(s) = &storage {
            let _ = s.remove_item("ms_lastresort_published_v1");
            let _ = s.set_item("ms_kp_reconciled_v1", "1");
        }
    }

    let need_lr = storage
        .as_ref()
        .and_then(|s| s.get_item("ms_lastresort_published_v1").ok().flatten())
        .is_none();
    let remaining = if need_reconcile {
        0
    } else {
        api.keypackage_count().await.map(|s| s.remaining).unwrap_or(0)
    };
    if !need_lr && remaining >= TARGET {
        return;
    }

    let Some(rt) = take_mls_runtime().await else {
        // MLS not initialized yet — retry on the next tick.
        LAST_KP_CHECK.with(|c| c.set(0.0));
        return;
    };
    let to_upload = |kp: messenger_core::mls::keypackage::GeneratedKeyPackage| KeyPackageUpload {
        key_package: kp.key_package_bytes,
        init_key_hash: kp.init_key_hash,
        expires_at: kp.expires_at,
        is_last_resort: kp.is_last_resort,
    };
    let published_lr = async {
        let mut uploads = Vec::new();
        if need_lr {
            if let Ok(kp) = rt.generate_keypackage(&identity, 2_592_000, true).await {
                uploads.push(to_upload(kp));
            }
        }
        for _ in 0..(TARGET - remaining).max(0) {
            if let Ok(kp) = rt.generate_keypackage(&identity, 604_800, false).await {
                uploads.push(to_upload(kp));
            }
        }
        if uploads.is_empty() {
            return false;
        }
        let lr = uploads.iter().any(|u| u.is_last_resort);
        match api
            .publish_keypackages(&PublishKeyPackagesRequest { key_packages: uploads })
            .await
        {
            Ok(_) => lr,
            Err(e) => {
                tracing::warn!(error = %e, "publish_keypackages failed");
                false
            }
        }
    }
    .await;
    MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));

    if published_lr {
        if let Some(s) = &storage {
            let _ = s.set_item("ms_lastresort_published_v1", "1");
        }
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
            let Some(envelope_ct) = svc.encrypt_envelope(group_id, &envelope).await else {
                return; // never send a read receipt in plaintext
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

        let Some(ciphertext) = self.encrypt_envelope(group_id, &envelope).await else {
            // Never fall back to plaintext — abort the send and tell the user.
            tracing::warn!(%group_id, "send_text: MLS encryption unavailable, send aborted");
            notify_encrypt_failure();
            return None;
        };

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
        thread_root: Option<Uuid>,
    ) -> Option<Uuid> {
        use messenger_core::attachment_crypto::{encrypt_attachment_chunked, DEFAULT_CHUNK_SIZE};
        use rand::RngCore;

        let api = build_api_client()?;

        // 1. Fresh per-attachment AES-256-GCM key. Lives only inside the MLS envelope.
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        // Voice is always chunked so the player can stream it (start on the first
        // chunk instead of waiting for the whole blob).
        let ciphertext = match encrypt_attachment_chunked(&key, &payload.bytes, DEFAULT_CHUNK_SIZE) {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("attachment encrypt failed: {e:?}");
                return None;
            }
        };
        let padded_size = ciphertext.len() as u64;
        let size_bucket = size_bucket_for(padded_size);

        // 2. Upload the ciphertext blob.
        let upload = match api.upload_attachment_smart(ciphertext, padded_size, size_bucket).await {
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
            reply_to_message_id: thread_root,
            thread_root_id: thread_root,
            created_at: now,
            sender_display_name_override: me.clone(),
        };
        // MLS-encrypt; if the group state isn't set up the send is aborted —
        // we never put a plaintext envelope on the wire.
        let Some(envelope_ct) = self.encrypt_envelope(group_id, &envelope).await else {
            // Never fall back to plaintext — abort the send.
            web_sys::console::warn_1(&"[send_voice] MLS encryption unavailable, send aborted".into());
            notify_encrypt_failure();
            self.mark_attachment_failed(group_id, client_message_id);
            return None;
        };

        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: envelope_ct,
            parent_message_id: thread_root,
            reply_to_message_id: thread_root,
            thread_root_id: thread_root,
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
                reply_to_message_id: thread_root,
                thread_root_id: thread_root,
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
        thread_root: Option<Uuid>,
    ) -> Option<Uuid> {
        use messenger_core::attachment_crypto::{
            encrypt_attachment, encrypt_attachment_chunked, DEFAULT_CHUNK_SIZE,
        };
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
                // Poster isn't generated until after transcode; the optimistic
                // bubble shows the placeholder, reconciled to the poster on send.
                thumb: None,
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
                reply_to_message_id: thread_root,
                thread_root_id: thread_root,
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

        // Normalize the payload depending on how the user chose to send it.
        //
        //  • `Media` (Telegram-style "Photo/Video"): compress/transcode into a
        //    small, streamable form. Images → downscaled JPEG; videos → H.264/AAC
        //    *fragmented* MP4 (mime gains `codecs="…"` so MediaSource plays it).
        //    On web/desktop this runs in ffmpeg.wasm's worker — off the main
        //    thread, so the composer never freezes. (Android native MediaCodec
        //    transcode is a follow-up; until then a Tauri WebView falls back to
        //    the in-process H.264 remux for already-H.264 clips, else raw.)
        //  • `File`: upload the original bytes untouched.
        //
        // Any transcode failure degrades to the raw bytes so the send still goes.
        use crate::chat::input_bar::AttachmentKind;
        const REMUX_MAX_BYTES: usize = 48 * 1024 * 1024;
        let want_media = payload.kind == AttachmentKind::Media;
        let is_native = crate::tauri_bridge::is_tauri_context();
        let raw = || {
            (
                std::borrow::Cow::Borrowed(payload.bytes.as_slice()),
                payload.mime.clone(),
            )
        };
        let (media_bytes, media_mime): (std::borrow::Cow<[u8]>, String) =
            if want_media && payload.mime.starts_with("image/") {
                match crate::media_transcode::compress_image(&payload.bytes, &payload.mime).await {
                    // Keep whichever is smaller — re-encoding a small/optimized
                    // image can grow it.
                    Ok((b, m)) if b.len() < payload.bytes.len() => (std::borrow::Cow::Owned(b), m),
                    Ok(_) => raw(),
                    Err(e) => {
                        tracing::warn!("image compress failed, sending original: {e}");
                        raw()
                    }
                }
            } else if want_media && payload.mime.starts_with("video/") {
                // Fast path on every platform: hardware WebCodecs transcode. In
                // the Android WebView (also Chromium) WebCodecs is backed by the
                // platform MediaCodec, so this is the native hardware transcode —
                // no separate Kotlin plugin needed when it's available.
                let hw = crate::media_transcode::transcode_video_hw(&payload.bytes, |p| {
                    web_sys::console::log_1(&format!("[transcode hw] {:.0}%", p * 100.0).into());
                })
                .await;
                match hw {
                    Ok((b, m)) => (std::borrow::Cow::Owned(b), m),
                    Err(e) if !is_native => {
                        // Web/desktop: ffmpeg.wasm software transcode (slow but
                        // decodes anything), then raw bytes.
                        web_sys::console::warn_1(
                            &format!("[transcode] hw path failed ({e}); trying ffmpeg").into(),
                        );
                        match crate::media_transcode::transcode_video(&payload.bytes, |p| {
                            web_sys::console::log_1(&format!("[transcode sw] {:.0}%", p * 100.0).into());
                        })
                        .await
                        {
                            Ok((b, m)) => (std::borrow::Cow::Owned(b), m),
                            Err(e2) => {
                                tracing::warn!("video transcode failed, sending original: {e2}");
                                raw()
                            }
                        }
                    }
                    Err(e) => {
                        // Native (Android) without WebCodecs: ffmpeg.wasm isn't
                        // bundled there, so try the pure-Rust H.264 remux (stream-
                        // copy) for already-H.264 clips, else send raw.
                        web_sys::console::warn_1(
                            &format!("[transcode] hw path failed ({e}); trying remux").into(),
                        );
                        if payload.bytes.len() <= REMUX_MAX_BYTES {
                            match messenger_core::video_remux::remux_to_fmp4(&payload.bytes) {
                                Some((fmp4, mime)) => (std::borrow::Cow::Owned(fmp4), mime),
                                None => raw(),
                            }
                        } else {
                            raw()
                        }
                    }
                }
            } else {
                raw()
            };
        let display_size = media_bytes.len() as u64;

        // Poster thumbnail for videos: a small JPEG of an early frame, embedded in
        // the (E2E-encrypted) message so the bubble shows a frame instead of a
        // film-strip placeholder. Generated from the transcoded bytes, which are
        // always playable here. `None` on failure → placeholder fallback.
        let video_thumb: Option<Vec<u8>> = if media_mime.starts_with("video/") {
            crate::media_transcode::video_poster(media_bytes.as_ref(), &media_mime).await
        } else {
            None
        };

        // Stream-friendly (chunked) encryption for playable media — video and
        // audio — so the player can start on the first chunk. Images and other
        // files stay whole-blob (nothing to stream).
        let stream_friendly =
            media_mime.starts_with("video/") || media_mime.starts_with("audio/");
        let encrypted = if stream_friendly {
            encrypt_attachment_chunked(&key, &media_bytes, DEFAULT_CHUNK_SIZE)
        } else {
            encrypt_attachment(&key, &media_bytes)
        };
        let ciphertext = match encrypted {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("attachment encrypt failed: {e:?}");
                self.mark_attachment_failed(group_id, client_message_id);
                return None;
            }
        };
        let padded_size = ciphertext.len() as u64;
        let size_bucket = size_bucket_for(padded_size);

        let upload = match api.upload_attachment_smart(ciphertext, padded_size, size_bucket).await {
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
                    // Stored mime tracks the actual uploaded bytes (e.g. a
                    // compressed Media image becomes image/jpeg).
                    mime: media_mime.clone(),
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
                    mime: media_mime.clone(),
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
                    mime: media_mime.clone(),
                    filename: payload.name.clone(),
                    size: display_size,
                    thumb: video_thumb.clone(),
                    caption: caption.clone(),
                },
                MessageBody::File {
                    attachment_id: upload.attachment_id,
                    decryption_key: key.to_vec(),
                    mime: media_mime.clone(),
                    name: payload.name.clone(),
                    size: display_size,
                    thumb: video_thumb.clone(),
                    caption: caption.clone(),
                },
            )
        };

        let envelope = ApplicationEnvelope {
            client_message_id,
            kind,
            body,
            reply_to_message_id: thread_root,
            thread_root_id: thread_root,
            created_at: now,
            sender_display_name_override: me.clone(),
        };
        // MLS-encrypt; if the group state isn't set up the send is aborted —
        // we never put a plaintext envelope on the wire.
        let Some(envelope_ct) = self.encrypt_envelope(group_id, &envelope).await else {
            // Never fall back to plaintext — abort the send.
            web_sys::console::warn_1(&"[send_attachment] MLS encryption unavailable, send aborted".into());
            notify_encrypt_failure();
            self.mark_attachment_failed(group_id, client_message_id);
            return None;
        };

        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: envelope_ct,
            parent_message_id: thread_root,
            reply_to_message_id: thread_root,
            thread_root_id: thread_root,
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
            messenger_core::attachment_crypto::decrypt_attachment_auto(&key_arr, &ct).ok()
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
                        // Forwarding re-uploads already-stored bytes verbatim;
                        // never re-compress.
                        kind: crate::chat::input_bar::AttachmentKind::File,
                        caption,
                    },
                    None,
                )
                .await
            }
            MessageBody::File { attachment_id, decryption_key, mime, name, caption, .. } => {
                let bytes = fetch_plain(attachment_id, &decryption_key).await?;
                let size = bytes.len() as u64;
                self.send_attachment(
                    target_group,
                    crate::chat::input_bar::AttachmentPayload {
                        bytes,
                        mime,
                        name,
                        size,
                        is_image: false,
                        kind: crate::chat::input_bar::AttachmentKind::File,
                        caption,
                    },
                    None,
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
                    None,
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
                let upload = match api.upload_attachment_smart(ciphertext, padded_size, size_bucket).await {
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
        let Some(envelope_ct) = self.encrypt_envelope(group_id, &envelope).await else {
            return false; // never broadcast an avatar update in plaintext
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

    /// Broadcast a group-metadata update (name and/or avatar) to a group.
    ///
    /// End-to-end via MLS, mirroring [`MessageService::broadcast_avatar`]. The
    /// `avatar` tuple is `(blob_id, key, mime)`; pass `None` to leave the avatar
    /// untouched. Recipients apply it through the `GroupUpdate` side-channel.
    pub async fn send_group_update(
        &self,
        group_id: Uuid,
        name: Option<String>,
        avatar: Option<(Option<Uuid>, Vec<u8>, String)>,
    ) -> Option<Uuid> {
        let Some(api) = build_api_client() else { return None };
        let (avatar_blob_id, decryption_key, mime) = avatar
            .unwrap_or((None, Vec::new(), String::new()));

        let client_message_id = Uuid::now_v7();
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::GroupUpdate,
            body: AppMessageBody::GroupUpdate {
                name,
                avatar_blob_id,
                decryption_key,
                mime,
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: js_sys::Date::now() as i64 / 1000,
            sender_display_name_override: current_display_name(),
        };
        let Some(envelope_ct) = self.encrypt_envelope(group_id, &envelope).await else {
            return None; // never broadcast group metadata in plaintext
        };
        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: envelope_ct,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };
        match api.post_message(group_id, &req).await {
            Ok(r) => Some(r.message_id),
            Err(e) => {
                tracing::warn!(%group_id, error = %e, "group update broadcast failed");
                None
            }
        }
    }

    /// Post a system note ("X добавлен(а)", etc.) to a group as an MLS
    /// application message. Other members render it as a centered system pill;
    /// the sender doesn't see their own (own MLS messages aren't self-decryptable).
    pub async fn send_system_note(&self, group_id: Uuid, text: &str) -> bool {
        let Some(api) = build_api_client() else { return false };
        let client_message_id = Uuid::now_v7();
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::SystemNote,
            body: AppMessageBody::SystemNote {
                code: text.to_string(),
                params: std::collections::HashMap::new(),
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: js_sys::Date::now() as i64 / 1000,
            sender_display_name_override: current_display_name(),
        };
        // System notes are control content — only send when MLS can encrypt
        // (don't leak a plaintext note).
        let Some(ct) = self.encrypt_envelope(group_id, &envelope).await else {
            return false;
        };
        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: ct,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };
        api.post_message(group_id, &req).await.is_ok()
    }

    /// Set a group's avatar (owner): compress, encrypt, upload, broadcast the
    /// `GroupUpdate`, finalize the blob, and apply locally (we can't decrypt our
    /// own MLS message to recover it). Returns `true` on success.
    pub async fn set_group_avatar(&self, group_id: Uuid, bytes: Vec<u8>, mime: String) -> bool {
        use messenger_core::attachment_crypto::encrypt_attachment;
        use rand::RngCore;

        let Some(api) = build_api_client() else { return false };
        let (img_bytes, img_mime) = crate::media_transcode::compress_image(&bytes, &mime)
            .await
            .unwrap_or((bytes, mime));

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let Ok(ciphertext) = encrypt_attachment(&key, &img_bytes) else {
            return false;
        };
        let padded_size = ciphertext.len() as u64;
        let size_bucket = size_bucket_for(padded_size);
        let upload = match api
            .upload_attachment_smart(ciphertext, padded_size, size_bucket)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(error = %e, "group avatar upload failed");
                return false;
            }
        };

        // Optimistic local apply.
        if let Some(users) = USERS_STATE.with(|c| c.borrow().clone()) {
            let data_url = crate::state::avatar_store::bytes_to_data_url(&img_mime, &img_bytes);
            users.remember_avatar(group_id, &data_url);
        }

        match self
            .send_group_update(
                group_id,
                None,
                Some((Some(upload.attachment_id), key.to_vec(), img_mime)),
            )
            .await
        {
            Some(msg_id) => {
                // Bind the blob to its message so recipients may download it.
                finalize_attachment_retrying(&api, upload.attachment_id, msg_id).await;
                true
            }
            None => false,
        }
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

    /// Make sure every GROUP this device owns has re-broadcast its name to the
    /// members. A device that joined late (welcome / owner self-heal) never
    /// receives the original `GroupUpdate{name}` — application messages aren't
    /// replayed to members who weren't in the group when they were sent — so it
    /// shows the bare group UUID forever. The owner re-announces the name here
    /// (rate-limited per change), and the late joiner applies it on receipt.
    /// Direct chats are skipped: their "name" is the peer's username, which is
    /// per-viewer and not a shared group name.
    pub async fn ensure_name_broadcasts(&self) {
        let Some(api) = build_api_client() else { return };
        let Some(chats) = chats_handle() else { return };
        let Ok(groups) = api.list_groups(None).await else { return };
        let cache = chats.display_name_cache.get_untracked();
        for g in groups.groups {
            // Only the owner is authoritative for a group's name; direct chats
            // carry no shared name.
            if g.role_in_chat != "owner" || g.group_type == "direct" {
                continue;
            }
            let Some(name) = cache.get(&g.id) else { continue };
            // Skip the UUID placeholder — that isn't a real name, and we never
            // want to broadcast it.
            if name.is_empty() || name.parse::<Uuid>().is_ok() {
                continue;
            }
            let already = NAME_ANNOUNCED.with(|c| c.borrow().get(&g.id).cloned());
            if already.as_deref() == Some(name.as_str()) {
                continue;
            }
            if self
                .send_group_update(g.id, Some(name.clone()), None)
                .await
                .is_some()
            {
                NAME_ANNOUNCED.with(|c| {
                    c.borrow_mut().insert(g.id, name.clone());
                });
                web_sys::console::log_1(
                    &format!("[name] ensure: re-announce '{name}' to {}", g.id).into(),
                );
            }
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
        let Some(ciphertext) = self.encrypt_envelope(group_id, &envelope).await else {
            notify_encrypt_failure();
            return None; // never send an edit in plaintext
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

        // MLS-encrypt the DeleteNotice; abort if unavailable — never plaintext.
        let Some(ciphertext) = self.encrypt_envelope(group_id, &envelope).await else {
            return false; // never send a delete notice in plaintext
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
        group_id: Uuid,
        message_id: Uuid,
        emoji: &str,
    ) -> bool {
        let api = match build_api_client() {
            Some(c) => c,
            None => return false,
        };

        // Do we already have this reaction?
        let has_own = self
            .messages
            .by_group
            .get_untracked()
            .values()
            .flatten()
            .find(|m| m.id == message_id)
            .map_or(false, |m| m.reactions.iter().any(|r| r.emoji == emoji && r.has_own));

        // Optimistic toggle — reflect immediately. When a peer also reacted with
        // the same emoji, decrement (don't drop the whole reaction).
        let emoji_owned = emoji.to_string();
        self.messages.by_group.update(|map| {
            for msgs in map.values_mut() {
                if let Some(msg) = msgs.iter_mut().find(|m| m.id == message_id) {
                    if has_own {
                        if let Some(existing) =
                            msg.reactions.iter_mut().find(|r| r.emoji == emoji_owned)
                        {
                            existing.count = existing.count.saturating_sub(1);
                            existing.has_own = false;
                        }
                        msg.reactions.retain(|r| r.count > 0);
                    } else if let Some(existing) =
                        msg.reactions.iter_mut().find(|r| r.emoji == emoji_owned)
                    {
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
        });

        // Persist our own reaction (the sender can't self-decrypt the Reaction
        // envelope on reload), and broadcast it E2E to the group via MLS.
        own_reactions_set(message_id, emoji, !has_own);

        let action = if has_own {
            messenger_core::mls::application::ReactionAction::Remove
        } else {
            messenger_core::mls::application::ReactionAction::Add
        };
        let client_message_id = Uuid::now_v7();
        let now = js_sys::Date::now() as i64 / 1000;
        let envelope = ApplicationEnvelope {
            client_message_id,
            kind: AppMessageKind::Reaction,
            body: AppMessageBody::Reaction {
                target_message_id: message_id,
                emoji: emoji.to_string(),
                action,
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: now,
            sender_display_name_override: current_display_name(),
        };
        let Some(envelope_ct) = self.encrypt_envelope(group_id, &envelope).await else {
            return false; // never send a reaction in plaintext
        };
        let req = PostMessageRequest {
            expected_epoch: 0,
            mls_ciphertext: envelope_ct,
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id,
        };
        match api.post_message(group_id, &req).await {
            Ok(resp) => {
                // Our own Reaction envelope is undecryptable to us — remember it
                // so the timeline drops it instead of showing garbage.
                own_reaction_msgs_record(resp.message_id);
                true
            }
            Err(e) => {
                tracing::warn!(%message_id, error = %e, "failed to broadcast reaction");
                false
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
            LAST_ENCRYPT_ERR.with(|c| *c.borrow_mut() = "MLS не инициализирован".to_string());
            return None;
        };
        let result = rt
            .encrypt_application_message(group_id, &identity, &plaintext)
            .await;
        MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));

        match result {
            Ok(ct) => {
                LAST_ENCRYPT_ERR.with(|c| c.borrow_mut().clear());
                Some(ct)
            }
            Err(e) => {
                web_sys::console::error_1(&format!("[encrypt_envelope] MLS encrypt failed for {group_id}: {e}").into());
                LAST_ENCRYPT_ERR.with(|c| *c.borrow_mut() = format!("{e}"));
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

    /// Process an incoming MLS commit (membership change) to advance our epoch.
    ///
    /// Best-effort: errors (e.g. an already-applied commit seen again on a full
    /// reload) are logged and ignored so they don't abort the rest of the pull.
    async fn process_incoming_commit(group_id: Uuid, commit: &[u8]) {
        let Some(rt) = take_mls_runtime().await else { return };
        let res = rt.process_commit(group_id, commit).await;
        MLS_CACHE.with(|c| *c.borrow_mut() = Some(rt));
        if let Err(e) = res {
            tracing::debug!(%group_id, error = %e, "process_commit skipped (already applied?)");
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
        // target message -> emoji -> set of users who currently react with it.
        // Messages arrive chronologically, so add/remove apply in order.
        let mut reactions: HashMap<Uuid, HashMap<String, HashSet<Uuid>>> = HashMap::new();
        let own_device = current_device_id();
        for msg in stored {
            // Membership commits (add/remove) must advance our local MLS epoch,
            // in chronological order, or later application messages won't
            // decrypt. Skip our own device's commits — we already merged those.
            if msg.wire_format == "commit" {
                if Some(msg.sender_device_id) != own_device {
                    Self::process_incoming_commit(group_id, &msg.mls_ciphertext).await;
                }
                continue;
            }
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
                Converted::Reaction { target, emoji, add, sender } => {
                    let users = reactions.entry(target).or_default().entry(emoji).or_default();
                    if add {
                        users.insert(sender);
                    } else {
                        users.remove(&sender);
                    }
                }
                Converted::Drop => {}
            }
        }
        // Our own messages can't be recovered from MLS on refresh (no
        // self-decrypt), so re-apply their cached body, our edits, and deletes.
        let own_msgs = own_msgs_load();
        let own_deletes = own_deletes_load();
        // Our own Reaction envelopes are undecryptable to us — drop them rather
        // than show ciphertext garbage. Own reactions come from the cache below.
        let own_reaction_msgs = own_reaction_msgs_load();
        if !own_reaction_msgs.is_empty() {
            result.retain(|m| !own_reaction_msgs.contains(&m.id));
        }
        let own_reactions = own_reactions_load();
        let me = current_user_id();

        for m in &mut result {
            // Fold reactions (peers' from MLS + ours from the local cache) into
            // this message.
            let peer = reactions.get(&m.id);
            let mine = own_reactions.get(&m.id);
            if peer.is_some() || mine.is_some() {
                let mut list: Vec<DisplayReaction> = Vec::new();
                if let Some(emoji_map) = peer {
                    for (emoji, users) in emoji_map {
                        if users.is_empty() {
                            continue;
                        }
                        let has_me = me.is_some_and(|me| users.contains(&me));
                        let own_here = mine.is_some_and(|s| s.contains(emoji));
                        // This device's own reaction isn't in the (undecryptable)
                        // envelope set, so add it if the cache has it.
                        let count = users.len() as u32 + u32::from(own_here && !has_me);
                        list.push(DisplayReaction {
                            emoji: emoji.clone(),
                            count,
                            has_own: has_me || own_here,
                        });
                    }
                }
                if let Some(s) = mine {
                    for emoji in s {
                        if !list.iter().any(|r| &r.emoji == emoji) {
                            list.push(DisplayReaction {
                                emoji: emoji.clone(),
                                count: 1,
                                has_own: true,
                            });
                        }
                    }
                }
                m.reactions = list;
            }
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
        // Use the cached plaintext if we've already decrypted this message —
        // MLS forbids decrypting an application message twice (SecretReuseError),
        // and the whole history is re-converted on every sync/reload.
        let decrypted = if let Some(cached) = decrypted_get(msg.id) {
            Some(cached)
        } else if !msg.mls_ciphertext.is_empty() {
            let d = self.decrypt_ciphertext(group_id, &msg.mls_ciphertext).await;
            if let Some(ref pt) = d {
                decrypted_put(msg.id, pt);
            }
            d
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
                            msg.id,
                            avatar_blob_id,
                            decryption_key.clone(),
                            mime.clone(),
                        );
                        return Converted::Drop;
                    }
                    // Group metadata side-channel: apply the name and/or avatar to
                    // the chat, then drop from the timeline.
                    if let AppMessageBody::GroupUpdate {
                        ref name,
                        avatar_blob_id,
                        ref decryption_key,
                        ref mime,
                    } = envelope.body
                    {
                        if let Some(name) = name {
                            Self::apply_group_name(group_id, name);
                        }
                        if let Some(blob) = avatar_blob_id {
                            Self::apply_group_avatar(
                                group_id,
                                msg.id,
                                blob,
                                decryption_key.clone(),
                                mime.clone(),
                            );
                        }
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
                    // Reaction: add/remove an emoji on an earlier message.
                    // Consumed here and folded into that message's reaction list
                    // by the caller, so it never shows as its own bubble.
                    if let AppMessageBody::Reaction {
                        target_message_id,
                        ref emoji,
                        ref action,
                    } = envelope.body
                    {
                        return Converted::Reaction {
                            target: target_message_id,
                            emoji: emoji.clone(),
                            add: matches!(
                                action,
                                messenger_core::mls::application::ReactionAction::Add
                            ),
                            sender: msg.sender_user_id,
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
                    // Undecryptable MLS ciphertext (our own message — MLS can't
                    // self-decrypt — or a secret already consumed): never render
                    // raw ciphertext as text.
                    if decrypted.is_none() && !msg.mls_ciphertext.is_empty() {
                        let is_own = Some(msg.sender_user_id) == current_user_id();
                        // Our own CONTENT message: emit an empty placeholder the
                        // caller overrides from the own-messages cache. Anything
                        // else (our own control message, or a received message
                        // whose secret was already consumed) is dropped.
                        if is_own && own_msgs_load().contains_key(&msg.id) {
                            (MessageKind::Text, MessageBody::Text(String::new()), None, None, msg.created_at, None)
                        } else {
                            return Converted::Drop;
                        }
                    } else {
                        // Last resort: treat as plain UTF-8 text (legacy plaintext).
                        let text = if payload.is_empty() {
                            String::new()
                        } else {
                            String::from_utf8_lossy(payload).to_string()
                        };
                        (MessageKind::Text, MessageBody::Text(text), None, None, msg.created_at, None)
                    }
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

    /// Apply an incoming group-name update. Guarded so a GroupUpdate can never
    /// rename a direct chat (those names are the peer's username); applies to
    /// group chats and not-yet-loaded groups.
    fn apply_group_name(group_id: Uuid, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(cs) = CHATS_STATE.with(|c| c.borrow().clone()) else {
            return;
        };
        let is_direct = cs.chats.get_untracked().iter().any(|ch| {
            ch.group_id == group_id
                && ch.chat_type == crate::state::chats::ChatType::Direct
        });
        if !is_direct {
            cs.set_display_name(group_id, name);
        }
    }

    /// Fetch + decrypt a group avatar blob and cache it (keyed by group_id in
    /// the same avatar map used for direct peers). Only ever moves forward in
    /// time (UUIDv7 msg ids), so re-processing history is a no-op.
    fn apply_group_avatar(group_id: Uuid, msg_id: Uuid, blob_id: Uuid, key: Vec<u8>, mime: String) {
        let already = AVATAR_APPLIED.with(|c| c.borrow().get(&group_id).copied());
        if already.is_some_and(|cur| msg_id <= cur) {
            return;
        }
        AVATAR_APPLIED.with(|c| {
            c.borrow_mut().insert(group_id, msg_id);
        });
        let Some(users) = USERS_STATE.with(|c| c.borrow().clone()) else {
            return;
        };
        spawn_local(async move {
            let Some(api) = build_api_client() else { return };
            let ciphertext = match api.download_attachment(blob_id, None).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(%blob_id, error = %e, "group avatar download failed");
                    return;
                }
            };
            let Ok(key_arr) = <[u8; 32]>::try_from(key.as_slice()) else {
                return;
            };
            let Ok(plain) =
                messenger_core::attachment_crypto::decrypt_attachment(&key_arr, &ciphertext)
            else {
                tracing::warn!(%blob_id, "group avatar decrypt failed");
                return;
            };
            let mime = if mime.is_empty() { "image/jpeg".to_string() } else { mime };
            let data_url = crate::state::avatar_store::bytes_to_data_url(&mime, &plain);
            users.remember_avatar(group_id, &data_url);
        });
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
    fn apply_avatar_update(sender: Uuid, msg_id: Uuid, blob_id: Option<Uuid>, key: Vec<u8>, mime: String) {
        if sender.is_nil() || Some(sender) == current_user_id() {
            return;
        }
        // Only ever move FORWARD in time: ignore an AvatarUpdate that's older
        // than (or equal to) the one already applied. UUIDv7 message ids are
        // time-ordered, so this picks the newest avatar and means re-opening a
        // chat (which re-processes the whole history) is a no-op — no flicker,
        // no re-download, and a stale older update can't win the race.
        let already = AVATAR_APPLIED.with(|c| c.borrow().get(&sender).copied());
        if already.is_some_and(|cur| msg_id <= cur) {
            return;
        }
        AVATAR_APPLIED.with(|c| {
            c.borrow_mut().insert(sender, msg_id);
        });
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
                    // A newer update may have superseded us while downloading.
                    let still_latest = AVATAR_APPLIED
                        .with(|c| c.borrow().get(&sender).copied())
                        .is_some_and(|cur| msg_id >= cur);
                    if !still_latest {
                        return;
                    }
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
                ref thumb,
                ref caption,
            } => (
                MessageKind::File,
                MessageBody::File {
                    attachment_id,
                    decryption_key: decryption_key.clone(),
                    mime: mime.clone(),
                    name: filename.clone(),
                    size,
                    thumb: thumb.clone(),
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
            | AppMessageBody::GroupUpdate { .. }
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

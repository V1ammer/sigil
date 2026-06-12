//! Chat list state — the sidebar's primary data.

use std::collections::HashMap;

use leptos::prelude::*;
use messenger_core::api::client::ApiClient;
use messenger_proto::mls::GroupSummary;
use uuid::Uuid;

use crate::state::messages::MessageKind;

#[derive(Clone, Debug)]
pub enum AvatarSource {
    Initials(String),
    Image(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatType {
    Direct,
    Group,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub group_id: Uuid,
    pub chat_type: ChatType,
    pub display_name: String,
    pub avatar: Option<AvatarSource>,
    pub last_message_preview: Option<String>,
    /// Kind of the latest message — used by the chat list to render an icon
    /// prefix (📷, 🎤, 📎...) for non-text bodies.
    pub last_message_kind: Option<MessageKind>,
    pub last_message_at: Option<i64>,
    pub unread_count: u32,
    pub muted: bool,
    pub pinned: bool,
    pub current_epoch: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatFilter {
    All,
    Unread,
    Direct,
    Groups,
}

/// Key for persisting display name in localStorage.
const DISPLAY_NAME_CACHE_KEY: &str = "messenger_chat_display_names";
/// Key for persisting per-chat user preferences (pin, mute, archive).
const CHAT_PREFS_KEY: &str = "messenger_chat_prefs";

/// Per-chat client-side preferences. The server does not store these — they are
/// purely a local UX convenience.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ChatPrefs {
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub archived: bool,
}

/// Reactive chat-list state.
#[derive(Clone)]
pub struct ChatsState {
    pub chats: RwSignal<Vec<Chat>>,
    pub selected: RwSignal<Option<Uuid>>,
    pub search: RwSignal<String>,
    pub filter: RwSignal<ChatFilter>,
    /// Cache of group_id → display_name for direct chats.
    pub display_name_cache: RwSignal<HashMap<Uuid, String>>,
    /// Per-chat preferences (pin, mute, archive) — local only.
    pub prefs: RwSignal<HashMap<Uuid, ChatPrefs>>,
}

impl ChatsState {
    #[must_use]
    pub fn new() -> Self {
        let cache = Self::load_cache_from_storage();
        let prefs = Self::load_prefs_from_storage();
        Self {
            chats: RwSignal::new(Vec::new()),
            selected: RwSignal::new(None),
            search: RwSignal::new(String::new()),
            filter: RwSignal::new(ChatFilter::All),
            display_name_cache: RwSignal::new(cache),
            prefs: RwSignal::new(prefs),
        }
    }

    /// Read the prefs for a chat (defaults to all-false).
    pub fn prefs_for(&self, group_id: Uuid) -> ChatPrefs {
        self.prefs
            .get_untracked()
            .get(&group_id)
            .cloned()
            .unwrap_or_default()
    }

    fn update_prefs<F: FnOnce(&mut ChatPrefs)>(&self, group_id: Uuid, f: F) {
        self.prefs.update(|map| {
            let entry = map.entry(group_id).or_default();
            f(entry);
        });
        self.persist_prefs();
        // Also reflect in the visible chat row so sorting picks it up.
        self.chats.update(|chats| {
            if let Some(c) = chats.iter_mut().find(|c| c.group_id == group_id) {
                let p = self.prefs.get_untracked();
                if let Some(pref) = p.get(&group_id) {
                    c.pinned = pref.pinned;
                    c.muted = pref.muted;
                }
            }
        });
    }

    pub fn toggle_pin(&self, group_id: Uuid) {
        self.update_prefs(group_id, |p| p.pinned = !p.pinned);
    }

    pub fn toggle_mute(&self, group_id: Uuid) {
        self.update_prefs(group_id, |p| p.muted = !p.muted);
    }

    pub fn toggle_archive(&self, group_id: Uuid) {
        self.update_prefs(group_id, |p| p.archived = !p.archived);
    }

    fn persist_prefs(&self) {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            let serialized: Vec<(String, ChatPrefs)> = self
                .prefs
                .get_untracked()
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            if let Ok(json) = serde_json::to_string(&serialized) {
                let _ = storage.set_item(CHAT_PREFS_KEY, &json);
            }
        }
    }

    fn load_prefs_from_storage() -> HashMap<Uuid, ChatPrefs> {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            if let Ok(Some(json)) = storage.get_item(CHAT_PREFS_KEY) {
                if let Ok(entries) = serde_json::from_str::<Vec<(String, ChatPrefs)>>(&json) {
                    return entries
                        .into_iter()
                        .filter_map(|(id_str, p)| id_str.parse::<Uuid>().ok().map(|id| (id, p)))
                        .collect();
                }
            }
        }
        HashMap::new()
    }

    /// Load chats from the server via API, replacing current list.
    ///
    /// For direct chats, tries to resolve the display name from the local cache.
    ///
    /// # Errors
    ///
    /// Propagates `ApiError` from the server.
    pub async fn load_from_server(&self, api: &ApiClient) -> Result<(), messenger_core::api::ApiError> {
        let resp = api.list_groups(None).await?;
        let cache = self.display_name_cache.get_untracked();
        let prefs = self.prefs.get_untracked();
        let chats: Vec<Chat> = resp
            .groups
            .into_iter()
            .map(|g: GroupSummary| {
                let chat_type = if g.group_type == "direct" {
                    ChatType::Direct
                } else {
                    ChatType::Group
                };
                // Resolve display name: local cache > UUID fallback
                let display_name = cache
                    .get(&g.id)
                    .cloned()
                    .unwrap_or_else(|| g.id.to_string());
                let p = prefs.get(&g.id).cloned().unwrap_or_default();
                Chat {
                    group_id: g.id,
                    chat_type,
                    display_name,
                    avatar: None,
                    last_message_preview: None,
                    last_message_kind: None,
                    last_message_at: None,
                    unread_count: 0,
                    muted: p.muted,
                    pinned: p.pinned,
                    current_epoch: g.current_epoch,
                }
            })
            .collect();
        self.chats.set(chats);
        Ok(())
    }

    /// Create a direct chat with a user by username.
    ///
    /// Caches the target username for display before reloading the list.
    ///
    /// # Errors
    ///
    /// Returns a string error message on failure.
    pub async fn create_direct_chat(&self, api: &ApiClient, username: &str) -> Result<Uuid, String> {
        let resp = api
            .create_direct_chat(username)
            .await
            .map_err(|e| format!("{e}"))?;
        // Cache the target username for this group before reload
        self.display_name_cache
            .update(|cache| {
                cache.insert(resp.group_id, username.to_string());
            });
        self.persist_cache();
        // Reload chats from server to include the new group
        self.load_from_server(api)
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(resp.group_id)
    }

    /// Set (and persist) a chat's display name, updating the loaded list too.
    /// Used to backfill names learned from message envelopes — the welcome
    /// recipient of a direct chat never typed the peer's username.
    pub fn set_display_name(&self, group_id: Uuid, name: &str) {
        if name.is_empty() {
            return;
        }
        self.display_name_cache.update(|cache| {
            cache.insert(group_id, name.to_string());
        });
        self.persist_cache();
        self.chats.update(|list| {
            if let Some(chat) = list.iter_mut().find(|c| c.group_id == group_id) {
                chat.display_name = name.to_string();
            }
        });
    }

    /// Persist the display-name cache to localStorage.
    fn persist_cache(&self) {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            let serialized: Vec<(String, String)> = self
                .display_name_cache
                .get_untracked()
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            if let Ok(json) = serde_json::to_string(&serialized) {
                let _ = storage.set_item(DISPLAY_NAME_CACHE_KEY, &json);
            }
        }
    }

    /// Load the display-name cache from localStorage.
    fn load_cache_from_storage() -> HashMap<Uuid, String> {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            if let Ok(Some(json)) = storage.get_item(DISPLAY_NAME_CACHE_KEY) {
                if let Ok(entries) = serde_json::from_str::<Vec<(String, String)>>(&json) {
                    return entries
                        .into_iter()
                        .filter_map(|(id_str, name)| {
                            id_str.parse::<Uuid>().ok().map(|id| (id, name))
                        })
                        .collect();
                }
            }
        }
        HashMap::new()
    }

    /// Returns a memoised derived signal that filters, searches and sorts the
    /// chat list. Archived chats are hidden unless the search query is non-empty.
    pub fn filtered(&self) -> impl Fn() -> Vec<Chat> + 'static {
        let chats = self.chats;
        let search = self.search;
        let filter = self.filter;
        let prefs = self.prefs;
        move || {
            let mut list = chats.get();
            let s = search.get().to_lowercase();
            let prefs_map = prefs.get();
            if s.is_empty() {
                list.retain(|c| !prefs_map.get(&c.group_id).map_or(false, |p| p.archived));
            } else {
                list.retain(|c| c.display_name.to_lowercase().contains(&s));
            }
            match filter.get() {
                ChatFilter::Unread => list.retain(|c| c.unread_count > 0),
                ChatFilter::Direct => list.retain(|c| c.chat_type == ChatType::Direct),
                ChatFilter::Groups => list.retain(|c| c.chat_type == ChatType::Group),
                ChatFilter::All => {}
            }
            // pinned → top, then by last_message_at desc
            list.sort_by(|a, b| {
                b.pinned
                    .cmp(&a.pinned)
                    .then(b.last_message_at.cmp(&a.last_message_at))
            });
            list
        }
    }

    /// Bump a chat's `last_message_at` if `ts_ms` is newer than what's stored.
    /// Called from every code path that inserts a message into `MessagesState`
    /// so the sidebar order tracks freshness without coupling to messages state.
    pub fn touch_last_message(&self, group_id: Uuid, ts_ms: i64) {
        self.chats.update(|list| {
            if let Some(chat) = list.iter_mut().find(|c| c.group_id == group_id) {
                if chat.last_message_at.map_or(true, |cur| ts_ms > cur) {
                    chat.last_message_at = Some(ts_ms);
                }
            }
        });
    }

    /// Update last-message metadata (timestamp + preview text + kind) when the
    /// incoming timestamp is newer than what's stored. Preview drives the
    /// Telegram-style snippet in the sidebar.
    pub fn set_last_message(
        &self,
        group_id: Uuid,
        ts_ms: i64,
        preview: Option<String>,
        kind: Option<MessageKind>,
    ) {
        self.chats.update(|list| {
            if let Some(chat) = list.iter_mut().find(|c| c.group_id == group_id) {
                if chat.last_message_at.map_or(true, |cur| ts_ms >= cur) {
                    chat.last_message_at = Some(ts_ms);
                    chat.last_message_preview = preview;
                    chat.last_message_kind = kind;
                }
            }
        });
    }
}

impl Default for ChatsState {
    fn default() -> Self {
        Self::new()
    }
}

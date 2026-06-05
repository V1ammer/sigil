//! Chat list state — the sidebar's primary data.

use std::collections::HashMap;

use leptos::prelude::*;
use messenger_core::api::client::ApiClient;
use messenger_proto::mls::GroupSummary;
use uuid::Uuid;

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

/// Reactive chat-list state.
#[derive(Clone)]
pub struct ChatsState {
    pub chats: RwSignal<Vec<Chat>>,
    pub selected: RwSignal<Option<Uuid>>,
    pub search: RwSignal<String>,
    pub filter: RwSignal<ChatFilter>,
    /// Cache of group_id → display_name for direct chats.
    pub display_name_cache: RwSignal<HashMap<Uuid, String>>,
}

impl ChatsState {
    #[must_use]
    pub fn new() -> Self {
        let cache = Self::load_cache_from_storage();
        Self {
            chats: RwSignal::new(Vec::new()),
            selected: RwSignal::new(None),
            search: RwSignal::new(String::new()),
            filter: RwSignal::new(ChatFilter::All),
            display_name_cache: RwSignal::new(cache),
        }
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
                Chat {
                    group_id: g.id,
                    chat_type,
                    display_name,
                    avatar: None,
                    last_message_preview: None,
                    last_message_at: None,
                    unread_count: 0,
                    muted: false,
                    pinned: false,
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
    /// chat list.
    pub fn filtered(&self) -> impl Fn() -> Vec<Chat> + 'static {
        let chats = self.chats;
        let search = self.search;
        let filter = self.filter;
        move || {
            let mut list = chats.get();
            let s = search.get().to_lowercase();
            if !s.is_empty() {
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
}

impl Default for ChatsState {
    fn default() -> Self {
        Self::new()
    }
}

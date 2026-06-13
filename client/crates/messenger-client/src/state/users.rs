//! User-display-name cache.
//!
//! The server is intentionally blind to usernames after registration — it only
//! stores a blind index. Display names therefore have to come from message
//! envelopes (`sender_display_name_override`) and are cached client-side so
//! reply previews, group members, and historical messages keep their labels
//! across reloads.

use std::collections::HashMap;

use leptos::prelude::*;
use uuid::Uuid;

const STORAGE_KEY: &str = "messenger_user_display_names";
const AVATAR_STORAGE_KEY: &str = "messenger_user_avatars";
const PEER_STORAGE_KEY: &str = "messenger_group_peers";
const USERNAME_STORAGE_KEY: &str = "messenger_user_usernames";

#[derive(Clone)]
pub struct UsersState {
    pub name_by_id: RwSignal<HashMap<Uuid, String>>,
    /// Peer avatars as data URLs, delivered via MLS `AvatarUpdate`.
    pub avatar_by_id: RwSignal<HashMap<Uuid, String>>,
    /// Direct-chat peer: group_id → the other participant's user_id.
    /// Learned from incoming message senders; lets the chat list resolve
    /// a peer avatar without any server-side membership lookup.
    pub peer_by_group: RwSignal<HashMap<Uuid, Uuid>>,
    /// Plaintext usernames learned client-side (own identity, and peers we
    /// started a direct chat with by username). The server stays blind to
    /// usernames — this is the only place they're known, used e.g. by the
    /// admin user list to label rows.
    pub username_by_id: RwSignal<HashMap<Uuid, String>>,
}

impl UsersState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            name_by_id: RwSignal::new(Self::load_map(STORAGE_KEY)),
            avatar_by_id: RwSignal::new(Self::load_map(AVATAR_STORAGE_KEY)),
            peer_by_group: RwSignal::new(Self::load_map(PEER_STORAGE_KEY)),
            username_by_id: RwSignal::new(Self::load_map(USERNAME_STORAGE_KEY)),
        }
    }

    pub fn username_for(&self, user_id: Uuid) -> Option<String> {
        self.username_by_id.get_untracked().get(&user_id).cloned()
    }

    /// Remember a plaintext username for a user. No-op if empty/unchanged.
    pub fn remember_username(&self, user_id: Uuid, username: &str) {
        let username = username.trim();
        if username.is_empty() {
            return;
        }
        if self.username_by_id.get_untracked().get(&user_id).map(String::as_str)
            == Some(username)
        {
            return;
        }
        self.username_by_id.update(|map| {
            map.insert(user_id, username.to_string());
        });
        Self::persist_map(USERNAME_STORAGE_KEY, &self.username_by_id.get_untracked());
    }

    pub fn get(&self, user_id: Uuid) -> Option<String> {
        self.name_by_id.get_untracked().get(&user_id).cloned()
    }

    /// Remember a display name for a user. No-op if the new name is empty.
    pub fn remember(&self, user_id: Uuid, name: &str) {
        if name.is_empty() {
            return;
        }
        let current = self.name_by_id.get_untracked();
        if current.get(&user_id).map(String::as_str) == Some(name) {
            return;
        }
        self.name_by_id.update(|map| {
            map.insert(user_id, name.to_string());
        });
        self.persist();
    }

    /// Best-effort display: cached name, otherwise the first 8 characters of
    /// the UUID with an ellipsis (so the UI never shows the full hex blob).
    pub fn label_for(&self, user_id: Uuid) -> String {
        if let Some(name) = self.get(user_id) {
            return name;
        }
        let id_str = user_id.to_string();
        id_str.chars().take(8).collect::<String>() + "…"
    }

    // --- Avatars (data URLs, delivered E2E via MLS AvatarUpdate) ---

    pub fn avatar_for(&self, user_id: Uuid) -> Option<String> {
        self.avatar_by_id.get_untracked().get(&user_id).cloned()
    }

    pub fn remember_avatar(&self, user_id: Uuid, data_url: &str) {
        if data_url.is_empty() {
            return;
        }
        if self.avatar_by_id.get_untracked().get(&user_id).map(String::as_str)
            == Some(data_url)
        {
            return;
        }
        self.avatar_by_id.update(|map| {
            map.insert(user_id, data_url.to_string());
        });
        Self::persist_map(AVATAR_STORAGE_KEY, &self.avatar_by_id.get_untracked());
    }

    pub fn forget_avatar(&self, user_id: Uuid) {
        if !self.avatar_by_id.get_untracked().contains_key(&user_id) {
            return;
        }
        self.avatar_by_id.update(|map| {
            map.remove(&user_id);
        });
        Self::persist_map(AVATAR_STORAGE_KEY, &self.avatar_by_id.get_untracked());
    }

    // --- Direct-chat peer mapping ---

    pub fn peer_of(&self, group_id: Uuid) -> Option<Uuid> {
        self.peer_by_group.get_untracked().get(&group_id).copied()
    }

    pub fn remember_peer(&self, group_id: Uuid, user_id: Uuid) {
        if self.peer_by_group.get_untracked().get(&group_id) == Some(&user_id) {
            return;
        }
        self.peer_by_group.update(|map| {
            map.insert(group_id, user_id);
        });
        Self::persist_map(PEER_STORAGE_KEY, &self.peer_by_group.get_untracked());
    }

    fn persist(&self) {
        Self::persist_map(STORAGE_KEY, &self.name_by_id.get_untracked());
    }

    fn persist_map<V: serde::Serialize + Clone>(key: &str, map: &HashMap<Uuid, V>) {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok()).flatten() else {
            return;
        };
        let serialized: Vec<(String, V)> =
            map.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        if let Ok(json) = serde_json::to_string(&serialized) {
            let _ = storage.set_item(key, &json);
        }
    }

    fn load_map<V: serde::de::DeserializeOwned>(key: &str) -> HashMap<Uuid, V> {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok()).flatten() else {
            return HashMap::new();
        };
        let Ok(Some(json)) = storage.get_item(key) else {
            return HashMap::new();
        };
        let Ok(entries) = serde_json::from_str::<Vec<(String, V)>>(&json) else {
            return HashMap::new();
        };
        entries
            .into_iter()
            .filter_map(|(id, v)| id.parse::<Uuid>().ok().map(|id| (id, v)))
            .collect()
    }
}

impl Default for UsersState {
    fn default() -> Self {
        Self::new()
    }
}

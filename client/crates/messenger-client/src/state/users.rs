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

#[derive(Clone)]
pub struct UsersState {
    pub name_by_id: RwSignal<HashMap<Uuid, String>>,
}

impl UsersState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            name_by_id: RwSignal::new(Self::load_from_storage()),
        }
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

    fn persist(&self) {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok()).flatten() else {
            return;
        };
        let serialized: Vec<(String, String)> = self
            .name_by_id
            .get_untracked()
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        if let Ok(json) = serde_json::to_string(&serialized) {
            let _ = storage.set_item(STORAGE_KEY, &json);
        }
    }

    fn load_from_storage() -> HashMap<Uuid, String> {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok()).flatten() else {
            return HashMap::new();
        };
        let Ok(Some(json)) = storage.get_item(STORAGE_KEY) else {
            return HashMap::new();
        };
        let Ok(entries) = serde_json::from_str::<Vec<(String, String)>>(&json) else {
            return HashMap::new();
        };
        entries
            .into_iter()
            .filter_map(|(id, name)| id.parse::<Uuid>().ok().map(|id| (id, name)))
            .collect()
    }
}

impl Default for UsersState {
    fn default() -> Self {
        Self::new()
    }
}

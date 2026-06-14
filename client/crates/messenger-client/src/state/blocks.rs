//! Client-side user blocklist.
//!
//! Blocking is a purely local concern: the server is zero-knowledge about who
//! a user has blocked. A blocked peer can't be messaged (the composer is
//! replaced with a notice) and their messages are hidden from the timeline.
//! The set of blocked user ids is persisted to `localStorage`.

use std::collections::HashSet;

use leptos::prelude::*;
use uuid::Uuid;

const STORAGE_KEY: &str = "messenger_blocked_users";

/// Reactive set of blocked user ids.
#[derive(Clone, Copy)]
pub struct BlockState {
    pub blocked: RwSignal<HashSet<Uuid>>,
}

impl BlockState {
    /// Create the state, seeding from `localStorage`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            blocked: RwSignal::new(load()),
        }
    }

    /// Whether `user_id` is currently blocked.
    pub fn is_blocked(&self, user_id: Uuid) -> bool {
        self.blocked.with(|s| s.contains(&user_id))
    }

    /// Block a user and persist.
    pub fn block(&self, user_id: Uuid) {
        self.blocked.update(|s| {
            s.insert(user_id);
        });
        self.persist();
    }

    /// Unblock a user and persist.
    pub fn unblock(&self, user_id: Uuid) {
        self.blocked.update(|s| {
            s.remove(&user_id);
        });
        self.persist();
    }

    fn persist(&self) {
        let csv = self
            .blocked
            .with_untracked(|s| s.iter().map(Uuid::to_string).collect::<Vec<_>>().join(","));
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            let _ = storage.set_item(STORAGE_KEY, &csv);
        }
    }
}

impl Default for BlockState {
    fn default() -> Self {
        Self::new()
    }
}

fn load() -> HashSet<Uuid> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .map(|csv| {
            csv.split(',')
                .filter_map(|s| Uuid::parse_str(s.trim()).ok())
                .collect()
        })
        .unwrap_or_default()
}

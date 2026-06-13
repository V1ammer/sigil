//! Typing-indicator state.
//!
//! Peers' "is typing" status, delivered over the WebSocket as
//! `ServerFrame::Typing { group_id, user_id, started }`. The server only sends
//! these to *other* members (never echoes to the typer), so any entry here is a
//! peer typing in that group.
//!
//! Each `started` refreshes a safety timeout: if no `stopped` arrives (e.g. the
//! peer's connection drops mid-type), the indicator auto-clears so it can't
//! hang forever.

use std::collections::HashMap;

use leptos::prelude::*;
use uuid::Uuid;

/// How long a "typing" entry survives without a refresh, in ms.
const TYPING_TTL_MS: u32 = 6000;

#[derive(Clone, Copy)]
pub struct TypingState {
    /// group_id → (typing user_id, generation token). Presence means "typing".
    by_group: RwSignal<HashMap<Uuid, (Uuid, u64)>>,
    /// Monotonic token, bumped on every `start`, so a stale auto-clear timeout
    /// doesn't wipe a fresher "typing" state.
    generation: RwSignal<u64>,
}

impl TypingState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_group: RwSignal::new(HashMap::new()),
            generation: RwSignal::new(0),
        }
    }

    /// Reactive: is someone currently typing in this group?
    pub fn is_typing(&self, group_id: Uuid) -> bool {
        self.by_group.with(|m| m.contains_key(&group_id))
    }

    /// Apply a typing frame. `started=false` clears immediately; `started=true`
    /// marks the peer as typing and arms an auto-clear after `TYPING_TTL_MS`.
    pub fn set(&self, group_id: Uuid, user_id: Uuid, started: bool) {
        if !started {
            self.by_group.update(|m| {
                m.remove(&group_id);
            });
            return;
        }
        let token = self.generation.get_untracked().wrapping_add(1);
        self.generation.set(token);
        self.by_group.update(|m| {
            m.insert(group_id, (user_id, token));
        });
        // Safety auto-clear: only fires if this exact token is still the latest.
        let by_group = self.by_group;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(TYPING_TTL_MS).await;
            by_group.update(|m| {
                if m.get(&group_id).map(|(_, t)| *t) == Some(token) {
                    m.remove(&group_id);
                }
            });
        });
    }

    /// Drop any typing state for a group (e.g. when its message arrives).
    pub fn clear(&self, group_id: Uuid) {
        if self.by_group.with_untracked(|m| m.contains_key(&group_id)) {
            self.by_group.update(|m| {
                m.remove(&group_id);
            });
        }
    }
}

impl Default for TypingState {
    fn default() -> Self {
        Self::new()
    }
}

//! Thread (reply) state — currently open thread.

use leptos::prelude::*;
use uuid::Uuid;

/// Which thread panel is currently open (if any).
#[derive(Clone)]
pub struct ThreadsState {
    pub open_thread_root_id: RwSignal<Option<Uuid>>,
}

impl ThreadsState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            open_thread_root_id: RwSignal::new(None),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open_thread_root_id.get().is_some()
    }

    pub fn open(&self, root_message_id: Uuid) {
        self.open_thread_root_id.set(Some(root_message_id));
    }

    pub fn close(&self) {
        self.open_thread_root_id.set(None);
    }
}

impl Default for ThreadsState {
    fn default() -> Self {
        Self::new()
    }
}

//! Toast / notification queue.

use leptos::prelude::*;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: Uuid,
    pub kind: ToastKind,
    pub message: String,
    pub auto_dismiss_ms: Option<u32>,
}

/// Global toast notification queue.
///
/// Push toasts via [`push`](NotificationsState::push) — they auto-dismiss
/// after `auto_dismiss_ms` milliseconds.
#[derive(Clone)]
pub struct NotificationsState {
    pub toasts: RwSignal<Vec<Toast>>,
}

impl NotificationsState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            toasts: RwSignal::new(Vec::new()),
        }
    }

    /// Push a new toast onto the queue.
    pub fn push(&self, kind: ToastKind, message: impl Into<String>) {
        let id = Uuid::now_v7();
        let toast = Toast {
            id,
            kind,
            message: message.into(),
            auto_dismiss_ms: Some(4000),
        };
        self.toasts.update(|v| v.push(toast));

        // Auto-dismiss after 4s.
        let tx = self.toasts;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(4000).await;
            tx.update(|v| v.retain(|t| t.id != id));
        });
    }

    /// Dismiss a toast immediately by id.
    pub fn dismiss(&self, id: Uuid) {
        self.toasts.update(|v| v.retain(|t| t.id != id));
    }

    /// Clear all toasts.
    pub fn clear(&self) {
        self.toasts.set(Vec::new());
    }
}

impl Default for NotificationsState {
    fn default() -> Self {
        Self::new()
    }
}

//! Persistent user settings (synced to local store).

use leptos::prelude::*;

/// User-facing settings that are persisted to local storage.
#[derive(Clone)]
pub struct SettingsState {
    pub font_size: RwSignal<String>,
    pub notifications_enabled: RwSignal<bool>,
    pub notification_sound: RwSignal<bool>,
    pub message_preview: RwSignal<bool>,
    pub read_receipts: RwSignal<bool>,
    pub history_retention: RwSignal<String>,
    pub auto_delete: RwSignal<String>,
}

impl SettingsState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_size: RwSignal::new("medium".into()),
            notifications_enabled: RwSignal::new(true),
            notification_sound: RwSignal::new(true),
            message_preview: RwSignal::new(true),
            read_receipts: RwSignal::new(false),
            history_retention: RwSignal::new("forever".into()),
            auto_delete: RwSignal::new("off".into()),
        }
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

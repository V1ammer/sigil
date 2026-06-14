//! Persistent user settings (synced to local store via localStorage).
//!
//! Each setting is a reactive `RwSignal` that is backed by `localStorage` for
//! persistence across reloads.

use leptos::prelude::*;

/// Load a string value from localStorage, returning `None` if missing.
fn load_str(key: &str) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()
        .flatten()?
        .get_item(key)
        .ok()
        .flatten()
}

/// Save a string value to localStorage.
fn save_str(key: &str, value: &str) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item(key, value);
    }
}

/// Remove a key from localStorage.
fn remove_key(key: &str) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.remove_item(key);
    }
}

/// Load a bool from localStorage with a default.
fn load_bool(key: &str, default: bool) -> bool {
    load_str(key)
        .and_then(|v| match v.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

/// Load a string from localStorage with a default.
fn load_setting(key: &str, default: &str) -> String {
    load_str(key).unwrap_or_else(|| default.to_string())
}

/// User-facing settings that are persisted to local storage.
#[derive(Clone)]
pub struct SettingsState {
    pub font_size: RwSignal<String>,
    pub notifications_enabled: RwSignal<bool>,
    pub notification_sound: RwSignal<bool>,
    pub notification_vibration: RwSignal<bool>,
    pub notification_filter: RwSignal<String>,
    pub message_preview: RwSignal<bool>,
    pub read_receipts: RwSignal<bool>,
    pub typing_indicators: RwSignal<bool>,
    pub history_retention: RwSignal<String>,
    pub auto_delete: RwSignal<String>,
    pub quiet_hours_enabled: RwSignal<bool>,
    pub quiet_hours_from: RwSignal<String>,
    pub quiet_hours_to: RwSignal<String>,
    /// When true, files at or below `auto_download_max_mb` are auto-fetched as
    /// soon as the message is rendered.
    pub auto_download_files: RwSignal<bool>,
    /// Maximum size (in MB, as a string for stable storage) that auto-download
    /// applies to. Stored as string for parity with the other text-based settings.
    pub auto_download_max_mb: RwSignal<String>,
}

/// localStorage key prefixes.
const PREFIX: &str = "ms_settings_";

impl SettingsState {
    #[must_use]
    pub fn new() -> Self {
        // Restore from localStorage with sensible defaults
        let s = Self {
            font_size: RwSignal::new(load_setting(&format!("{PREFIX}font_size"), "medium")),
            notifications_enabled: RwSignal::new(load_bool(&format!("{PREFIX}notifications_enabled"), true)),
            notification_sound: RwSignal::new(load_bool(&format!("{PREFIX}notification_sound"), true)),
            notification_vibration: RwSignal::new(load_bool(&format!("{PREFIX}notification_vibration"), false)),
            notification_filter: RwSignal::new(load_setting(&format!("{PREFIX}notification_filter"), "all")),
            message_preview: RwSignal::new(load_bool(&format!("{PREFIX}message_preview"), true)),
            // Default ON, like mainstream messengers — receipts only flow
            // between users who both keep this enabled.
            read_receipts: RwSignal::new(load_bool(&format!("{PREFIX}read_receipts"), true)),
            typing_indicators: RwSignal::new(load_bool(&format!("{PREFIX}typing_indicators"), true)),
            history_retention: RwSignal::new(load_setting(&format!("{PREFIX}history_retention"), "forever")),
            auto_delete: RwSignal::new(load_setting(&format!("{PREFIX}auto_delete"), "off")),
            quiet_hours_enabled: RwSignal::new(load_bool(&format!("{PREFIX}quiet_hours_enabled"), false)),
            quiet_hours_from: RwSignal::new(load_setting(&format!("{PREFIX}quiet_hours_from"), "22:00")),
            quiet_hours_to: RwSignal::new(load_setting(&format!("{PREFIX}quiet_hours_to"), "08:00")),
            auto_download_files: RwSignal::new(load_bool(&format!("{PREFIX}auto_download_files"), false)),
            auto_download_max_mb: RwSignal::new(load_setting(&format!("{PREFIX}auto_download_max_mb"), "10")),
        };

        // Wire persistence effects
        {
            let fs = s.font_size;
            Effect::new(move |_| save_str(&format!("{PREFIX}font_size"), &fs.get()));
        }
        {
            let n = s.notifications_enabled;
            Effect::new(move |_| save_str(&format!("{PREFIX}notifications_enabled"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.notification_sound;
            Effect::new(move |_| save_str(&format!("{PREFIX}notification_sound"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.notification_vibration;
            Effect::new(move |_| save_str(&format!("{PREFIX}notification_vibration"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.notification_filter;
            Effect::new(move |_| save_str(&format!("{PREFIX}notification_filter"), &n.get()));
        }
        {
            let n = s.message_preview;
            Effect::new(move |_| save_str(&format!("{PREFIX}message_preview"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.read_receipts;
            Effect::new(move |_| save_str(&format!("{PREFIX}read_receipts"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.typing_indicators;
            Effect::new(move |_| save_str(&format!("{PREFIX}typing_indicators"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.history_retention;
            Effect::new(move |_| save_str(&format!("{PREFIX}history_retention"), &n.get()));
        }
        {
            let n = s.auto_delete;
            Effect::new(move |_| save_str(&format!("{PREFIX}auto_delete"), &n.get()));
        }
        {
            let n = s.quiet_hours_enabled;
            Effect::new(move |_| save_str(&format!("{PREFIX}quiet_hours_enabled"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.quiet_hours_from;
            Effect::new(move |_| save_str(&format!("{PREFIX}quiet_hours_from"), &n.get()));
        }
        {
            let n = s.quiet_hours_to;
            Effect::new(move |_| save_str(&format!("{PREFIX}quiet_hours_to"), &n.get()));
        }
        {
            let n = s.auto_download_files;
            Effect::new(move |_| save_str(&format!("{PREFIX}auto_download_files"), if n.get() { "true" } else { "false" }));
        }
        {
            let n = s.auto_download_max_mb;
            Effect::new(move |_| save_str(&format!("{PREFIX}auto_download_max_mb"), &n.get()));
        }

        s
    }

    /// Wipe all persisted settings from localStorage.
    pub fn wipe_all() {
        let keys = [
            "font_size", "notifications_enabled", "notification_sound",
            "notification_vibration", "notification_filter", "message_preview",
            "read_receipts", "typing_indicators", "history_retention", "auto_delete",
            "quiet_hours_enabled", "quiet_hours_from", "quiet_hours_to",
            "auto_download_files", "auto_download_max_mb",
        ];
        for k in &keys {
            remove_key(&format!("{PREFIX}{k}"));
        }
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

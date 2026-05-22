//! Transient UI state — panels, sheets, dialogs.

use leptos::prelude::*;

/// Transient UI state that does not need persistence.
#[derive(Clone)]
pub struct UiState {
    pub sidebar_collapsed: RwSignal<bool>,
    pub profile_sheet_open: RwSignal<bool>,
    pub new_chat_dialog_open: RwSignal<bool>,
    pub media_viewer_open: RwSignal<bool>,
    pub settings_section: RwSignal<String>,
}

impl UiState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sidebar_collapsed: RwSignal::new(false),
            profile_sheet_open: RwSignal::new(false),
            new_chat_dialog_open: RwSignal::new(false),
            media_viewer_open: RwSignal::new(false),
            settings_section: RwSignal::new(String::new()),
        }
    }

    pub fn toggle_sidebar(&self) {
        self.sidebar_collapsed.update(|v| *v = !*v);
    }

    pub fn open_profile_sheet(&self) {
        self.profile_sheet_open.set(true);
    }

    pub fn close_profile_sheet(&self) {
        self.profile_sheet_open.set(false);
    }

    pub fn open_new_chat_dialog(&self) {
        self.new_chat_dialog_open.set(true);
    }

    pub fn close_new_chat_dialog(&self) {
        self.new_chat_dialog_open.set(false);
    }

    pub fn open_media_viewer(&self) {
        self.media_viewer_open.set(true);
    }

    pub fn close_media_viewer(&self) {
        self.media_viewer_open.set(false);
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

//! Theme provider — manages light/dark/system mode.
//!
//! Persists the choice to local storage and restores it at startup.

use leptos::prelude::*;
use crate::i18n::Locale;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Self::System
    }
}

impl Theme {
    /// Restore from a persisted string.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

/// Provide a theme signal into context and set up the effect that applies
/// "dark" / "light" class on `<html>`.
///
/// If `initial` is `None`, defaults to `System`.
#[must_use]
pub fn provide_theme(initial: Option<Theme>) -> RwSignal<Theme> {
    let theme = RwSignal::new(initial.unwrap_or_default());

    // Apply theme class whenever it changes, and persist to local storage.
    Effect::new(move |_| {
        let t = theme.get();
        apply_theme(t);
        persist_theme(t);
    });

    provide_context(theme);
    theme
}

/// Apply a theme to the `<html>` element.
fn apply_theme(theme: Theme) {
    let doc = document();
    let html = doc.document_element();
    if let Some(el) = html {
        let cl = el.class_list();
        let _ = cl.remove_2("light", "dark");
        match theme {
            Theme::Light => {
                let _ = cl.add_1("light");
            }
            Theme::Dark => {
                let _ = cl.add_1("dark");
            }
            Theme::System => {
                let prefers_dark = window()
                    .match_media("(prefers-color-scheme: dark)")
                    .ok()
                    .flatten()
                    .map(|m| m.matches())
                    .unwrap_or(false);
                if prefers_dark {
                    let _ = cl.add_1("dark");
                } else {
                    let _ = cl.add_1("light");
                }
            }
        }
    }
}

/// Persist the current theme to `localStorage`.
fn persist_theme(theme: Theme) {
    let s = match theme {
        Theme::Light => "light",
        Theme::Dark => "dark",
        Theme::System => "system",
    };
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item("messenger_theme", s);
    }
}

/// Attempt to restore a persisted theme from `localStorage`.
#[must_use]
pub fn restore_theme() -> Option<Theme> {
    let storage = window().local_storage().ok().flatten()?;
    let val = storage.get_item("messenger_theme").ok().flatten()?;
    Some(Theme::from_str(&val))
}

/// Persist the chosen locale to local storage.
pub fn persist_locale(locale: &Locale) {
    let s = match locale {
        Locale::Ru => "ru",
        Locale::En => "en",
        Locale::System => "system",
    };
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item("messenger_locale", s);
    }
}

/// Restore a persisted locale from local storage.
#[must_use]
pub fn restore_locale() -> Option<Locale> {
    let storage = window().local_storage().ok().flatten()?;
    let val = storage.get_item("messenger_locale").ok().flatten()?;
    Some(Locale::from_str(&val))
}

/// Apply font size class to html element.
pub fn apply_font_size(size: &str) {
    let doc = document();
    let html = doc.document_element();
    if let Some(el) = html {
        let cl = el.class_list();
        let _ = cl.remove_2("text-sm", "text-lg");
        match size {
            "small" => {
                let _ = cl.add_1("text-sm");
            }
            "large" => {
                let _ = cl.add_1("text-lg");
            }
            _ => {
                let _ = cl.add_1("text-base");
            }
        }
    }
}

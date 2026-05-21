//! Theme provider — manages light/dark/system mode.
use leptos::prelude::*;

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

/// Provide a theme signal into context and set up the effect that applies
/// "dark" / "light" class on `<html>`.
#[must_use]
pub fn provide_theme() -> RwSignal<Theme> {
    let theme = RwSignal::new(Theme::System);
    Effect::new(move |_| {
        let t = theme.get();
        apply_theme(t);
    });
    provide_context(theme);
    theme
}

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

/// Apply font size class to html element.
pub fn apply_font_size(size: &str) {
    let doc = document();
    let html = doc.document_element();
    if let Some(el) = html {
        let cl = el.class_list();
        let _ = cl.remove_2("text-sm", "text-lg");
        match size {
            "small" => { let _ = cl.add_1("text-sm"); }
            "large" => { let _ = cl.add_1("text-lg"); }
            _ => { let _ = cl.add_1("text-base"); }
        }
    }
}

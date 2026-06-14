//! Reactive i18n with ru/en locale switching.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::i18n::*;
//!
//! // Inside a component:
//! let text = t!("connect.title");
//!
//! // Or with the struct directly:
//! let i18n = use_i18n();
//! let text = i18n.t("connect.title");
//! ```

pub mod ru;
pub mod en;

use std::collections::HashMap;
use std::sync::LazyLock;
use leptos::prelude::*;

// Re-export dictionary types so callers can use `ru_dict()` etc.
pub use ru::ru_dict;
pub use en::en_dict;

// Backward-compatible alias for C02 code that still references `Language`.
#[doc(hidden)]
pub type Language = Locale;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Locale {
    Ru,
    En,
    System,
}

impl Default for Locale {
    fn default() -> Self {
        Self::System
    }
}

impl Locale {
    /// Convert a string like "ru", "en", "system" into a `Locale`.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s {
            "ru" => Self::Ru,
            "en" => Self::En,
            _ => Self::System,
        }
    }

    /// Return the two-letter language code.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ru => "ru",
            Self::En => "en",
            Self::System => system_locale(),
        }
    }
}

/// Detect the browser's UI language.
fn system_locale() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        let lang = web_sys::window()
            .and_then(|w| w.navigator().language())
            .unwrap_or_default();
        if lang.starts_with("ru") {
            "ru"
        } else {
            "en"
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "en"
    }
}

/// Merged translation dictionaries keyed by language code.
static DICTS: LazyLock<HashMap<&'static str, HashMap<&'static str, &'static str>>> =
    LazyLock::new(|| {
        let mut d = HashMap::new();
        d.insert("ru", ru::ru_dict());
        d.insert("en", en::en_dict());
        d
    });

/// Reactive i18n handle.
///
/// Call [`provide_i18n`] at the app root, then use [`use_i18n`] in components
/// or the `t!()` macro.
#[derive(Clone)]
pub struct I18n {
    pub locale: RwSignal<Locale>,
}

impl I18n {
    /// Create a new instance with the system locale.
    #[must_use]
    pub fn new() -> Self {
        Self {
            locale: RwSignal::new(Locale::default()),
        }
    }

    /// Translate `key` into the current locale.
    ///
    /// Falls back to the key itself if no translation is found.
    pub fn t(&self, key: &'static str) -> String {
        let loc = self.locale.get().as_str();
        DICTS
            .get(loc)
            .and_then(|d| d.get(key))
            .map(|s| s.to_string())
            .unwrap_or_else(|| key.to_string())
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide an `I18n` instance as context.
pub fn provide_i18n() -> I18n {
    let i = I18n::new();
    provide_context(i.clone());
    i
}

/// Retrieve the `I18n` from context.
///
/// # Panics
///
/// Panics if not provided — call `provide_i18n` at the app root.
pub fn use_i18n() -> I18n {
    use_context::<I18n>().expect("I18n must be provided via provide_i18n()")
}

thread_local! {
    static I18N_HANDLE: std::cell::RefCell<Option<I18n>> = const { std::cell::RefCell::new(None) };
}

/// Register the app's `I18n` so code paths outside the leptos owner (background
/// loops, the message-service send path) can still translate. Call once at App.
pub fn register_i18n(i: I18n) {
    I18N_HANDLE.with(|c| *c.borrow_mut() = Some(i));
}

/// Translate `key` via the globally-registered `I18n`, falling back to the key.
/// Use only where `t!()`/`use_i18n()` can't (no leptos owner).
#[must_use]
pub fn tr(key: &'static str) -> String {
    I18N_HANDLE.with(|c| {
        c.borrow()
            .as_ref()
            .map_or_else(|| key.to_string(), |i| i.t(key))
    })
}

/// Macro shorthand for `use_i18n().t(key)`.
#[macro_export]
macro_rules! t {
    ($key:expr) => {{
        $crate::i18n::use_i18n().t($key)
    }};
}

// ---------------------------------------------------------------------------
// Legacy free functions (keep for backward compatibility with existing code)
// ---------------------------------------------------------------------------

/// Legacy translate function — kept for smooth C02→C06 migration.
///
/// Prefer the `I18n` struct + `t!()` macro in new code.
pub fn t(lang: Locale, key: &'static str) -> String {
    let loc = lang.as_str();
    DICTS
        .get(loc)
        .and_then(|d| d.get(key))
        .map(|s| s.to_string())
        .unwrap_or_else(|| key.to_string())
}

/// Format a timestamp into HH:MM.
pub fn format_time(timestamp_ms: f64, lang: Locale) -> String {
    let _ = lang; // time format is locale-independent (HH:MM)
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp_ms));
    let h = date.get_hours();
    let m = date.get_minutes();
    format!("{:02}:{:02}", h, m)
}

/// Format a duration in seconds to "M:SS".
pub fn format_duration(seconds: u32) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{}:{:02}", m, s)
}

/// Format a date to a readable string (Today, Yesterday, or "12 January").
pub fn format_date(timestamp_ms: f64, lang: Locale) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp_ms));
    let now = js_sys::Date::new_0();
    let today_start = {
        let d = js_sys::Date::new_0();
        d.set_hours(0);
        d.set_minutes(0);
        d.set_seconds(0);
        d.set_milliseconds(0);
        d.get_time()
    };
    let yesterday_start = today_start - 86_400_000.0;
    let date_time = date.get_time();

    if date_time >= today_start {
        t(lang, "time.today")
    } else if date_time >= yesterday_start {
        t(lang, "time.yesterday")
    } else {
        let months_ru = [
            "января", "февраля", "марта", "апреля", "мая", "июня",
            "июля", "августа", "сентября", "октября", "ноября", "декабря",
        ];
        let months_en = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December",
        ];
        let month = (date.get_month()) as usize;
        let day = date.get_date();
        match lang {
            Locale::Ru => format!("{} {}", day, months_ru[month]),
            Locale::En | Locale::System => format!("{} {}", months_en[month], day),
        }
    }
}

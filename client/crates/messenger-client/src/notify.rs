//! Real OS / browser notifications for incoming messages.
//!
//! Two backends, chosen at runtime:
//! - **Tauri** (Android, desktop): `tauri-plugin-notification` via `invoke`.
//! - **Plain browser**: the Web `Notification` API.
//!
//! Both share the same user-settings gate as the notification *sound*
//! (master toggle, message-preview, Do Not Disturb, startup grace) plus the
//! notification *filter*; per-chat mute and "is the user looking at this chat"
//! are decided by the caller in the WS event loop. Strings are plain Russian
//! literals on purpose: this runs in the WS event loop where the Leptos owner
//! is gone, so the `t!` i18n macro would panic.

use crate::state::messages::MessageBody;

/// Build the one-line preview shown in a notification body.
///
/// Returns `None` for messages that shouldn't notify (empty text, system
/// events).
#[must_use]
pub fn preview_text(body: &MessageBody) -> Option<String> {
    let s = match body {
        MessageBody::Text(t) => {
            let t = t.trim();
            if t.is_empty() {
                return None;
            }
            truncate(t, 140)
        }
        MessageBody::Voice { caption, .. } => caption
            .clone()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| "🎤 Голосовое сообщение".to_string()),
        MessageBody::Image { caption, .. } => caption
            .clone()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| "🖼 Фото".to_string()),
        MessageBody::File {
            caption, mime, name, ..
        } => caption.clone().filter(|c| !c.trim().is_empty()).unwrap_or_else(|| {
            if mime.starts_with("video/") {
                "🎬 Видео".to_string()
            } else {
                format!("📎 {name}")
            }
        }),
        MessageBody::System { .. } => return None,
    };
    Some(s)
}

/// Truncate `s` to at most `max` chars, appending an ellipsis when cut.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Fire an OS / browser notification for an incoming message.
///
/// Gated by the same settings as the sound, plus the notification filter.
/// When message-preview is disabled the title and body are replaced with a
/// generic placeholder so nothing sensitive shows on the lock screen.
pub fn notify_incoming(title: &str, body: &str) {
    if !crate::sound::setting_on("ms_settings_notifications_enabled")
        || crate::sound::in_do_not_disturb()
        || crate::sound::within_startup_grace()
    {
        return;
    }
    // Filter: "none" suppresses; "all"/"mentions" both notify (no mention
    // parsing yet — treated as "all").
    if crate::sound::setting_str("ms_settings_notification_filter").as_deref() == Some("none") {
        return;
    }

    // message_preview defaults on; off → hide who/what.
    let (title, body) = if crate::sound::setting_on("ms_settings_message_preview") {
        (title.to_string(), body.to_string())
    } else {
        ("Sigil".to_string(), "Новое сообщение".to_string())
    };

    if crate::tauri_bridge::is_tauri_context() {
        leptos::task::spawn_local(async move {
            let _ = crate::tauri_bridge::show_native_notification(&title, &body).await;
        });
    } else {
        web_notify(&title, &body);
    }
}

/// Show a notification through the browser `Notification` API.
///
/// Silently no-ops unless permission has already been granted (we request it
/// from [`arm_notifications`]).
fn web_notify(title: &str, body: &str) {
    use web_sys::{Notification, NotificationOptions, NotificationPermission};

    if Notification::permission() != NotificationPermission::Granted {
        return;
    }
    let opts = NotificationOptions::new();
    opts.set_body(body);
    opts.set_icon("/favicon.png");
    let _ = Notification::new_with_options(title, &opts);
}

/// Prime notification permission at startup.
///
/// - **Tauri**: request POST_NOTIFICATIONS once (Android 13+).
/// - **Browser**: request permission on the first user gesture (browsers
///   require one). Mirrors the audio-unlock listener in [`crate::sound`].
pub fn arm_notifications() {
    if crate::tauri_bridge::is_tauri_context() {
        leptos::task::spawn_local(async {
            let _ = crate::tauri_bridge::request_notification_permission().await;
        });
        return;
    }

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use web_sys::{Notification, NotificationPermission};

    // Already decided (granted or denied) — nothing to ask.
    if Notification::permission() != NotificationPermission::Default {
        return;
    }
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };

    let cb = Closure::<dyn FnMut()>::new(move || {
        if Notification::permission() == NotificationPermission::Default {
            // Promise result ignored — the browser persists the choice.
            let _ = Notification::request_permission();
        }
    });
    let f = cb.as_ref().unchecked_ref::<js_sys::Function>();
    let _ = doc.add_event_listener_with_callback("pointerdown", f);
    let _ = doc.add_event_listener_with_callback("keydown", f);
    cb.forget();
}

//! Android-back / browser-back handler stack.
//!
//! Each time an overlay (chat view, image lightbox, dialog) opens, the caller
//! pushes a history entry and a close-handler closure. When the user hits the
//! Android back button, the WebView calls `history.back()`, the global
//! `popstate` listener pops the most recent handler and runs it — which sets
//! the overlay's `is_open` signal to false. The browser history naturally
//! drains until empty, at which point Tauri's default behavior exits the app.
//!
//! To close an overlay programmatically (X button, "save & close" button) call
//! [`pop`], which triggers `history.back()` and lets the same handler run —
//! keeping the history stack in sync with the UI.

use std::cell::RefCell;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

thread_local! {
    static HANDLERS: RefCell<Vec<Box<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
    static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
}

fn install_listener() {
    INSTALLED.with(|cell| {
        if *cell.borrow() {
            return;
        }
        *cell.borrow_mut() = true;
        let Some(win) = web_sys::window() else { return };
        let closure = Closure::wrap(Box::new(|| {
            let handler = HANDLERS.with(|h| h.borrow_mut().pop());
            if let Some(f) = handler {
                f();
            }
        }) as Box<dyn Fn()>);
        let _ = win.add_event_listener_with_callback(
            "popstate",
            closure.as_ref().unchecked_ref(),
        );
        // Listener lives for the lifetime of the page.
        closure.forget();
    });
}

/// Register a close-handler that runs when Android back / browser back fires.
/// Pushes a placeholder history entry so the back button has something to pop.
pub fn push<F: Fn() + 'static>(close: F) {
    install_listener();
    let Some(win) = web_sys::window() else { return };
    let Ok(history) = win.history() else { return };
    let _ = history.push_state_with_url(&JsValue::NULL, "", None);
    HANDLERS.with(|h| h.borrow_mut().push(Box::new(close)));
}

/// Programmatically dismiss the topmost overlay (e.g. an in-app close button).
/// Triggers `popstate` so the handler in [`push`] runs and the history stack
/// stays consistent with the UI.
pub fn pop() {
    let Some(win) = web_sys::window() else { return };
    let Ok(history) = win.history() else { return };
    let _ = history.back();
}

/// Whether any overlay (chat, thread, dialog, lightbox) is currently open.
#[must_use]
pub fn has_overlay() -> bool {
    HANDLERS.with(|h| !h.borrow().is_empty())
}

/// Handle the Android hardware-back button. Tauri's default exits the app
/// instead of running the WebView's history, so the native `MainActivity`
/// intercepts back and calls this via `window.__androidBack()`:
///   - returns `true` and closes the topmost overlay if one is open;
///   - returns `false` (at the chat list / root) so the native side can do its
///     own "press again to exit" handling.
fn handle_android_back() -> bool {
    // 1. An open overlay (chat, thread, dialog) closes first.
    if has_overlay() {
        pop();
        return true;
    }
    // 2. Not on the chat list (e.g. Settings is a route) → go back one entry,
    //    which returns to the chat list. `history.back()` fires popstate so the
    //    router navigates AND any overlay handler runs.
    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    if path != "/chats" && path != "/chats/" && !path.is_empty() {
        pop();
        return true;
    }
    // 3. At the chat list with nothing to close → native "press again to exit".
    false
}

/// Expose `window.__androidBack()` for the native back-button handler. Call once
/// at startup.
pub fn install_android_back_bridge() {
    let Some(win) = web_sys::window() else { return };
    let cb = Closure::<dyn Fn() -> bool>::new(handle_android_back);
    let _ = js_sys::Reflect::set(
        &win,
        &JsValue::from_str("__androidBack"),
        cb.as_ref().unchecked_ref(),
    );
    cb.forget();
}

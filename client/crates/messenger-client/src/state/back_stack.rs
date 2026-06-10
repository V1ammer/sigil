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

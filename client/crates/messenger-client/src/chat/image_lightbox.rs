//! Full-screen image viewer with pinch-zoom, drag-pan, and double-tap toggle.
//!
//! Rendered inline by each image message — `is_open` is local to the message,
//! so opening one viewer doesn't disturb others.
use leptos::ev::PointerEvent;
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::JsCast;

use crate::icons::Icon;

#[derive(Clone, Copy, Default)]
struct GestureStart {
    distance: f64,
    scale: f64,
    tx: f64,
    ty: f64,
    mid_x: f64,
    mid_y: f64,
}

#[must_use]
#[component]
pub fn ImageLightbox(
    #[prop(into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(into)] src: Signal<Option<String>>,
) -> impl IntoView {
    let scale = RwSignal::new(1.0_f64);
    let tx = RwSignal::new(0.0_f64);
    let ty = RwSignal::new(0.0_f64);

    // Pointer + gesture state lives in StoredValue so the event closures stay
    // Send+Sync (Rc<RefCell<_>> would fail Leptos's children-fn bound).
    let pointers: StoredValue<HashMap<i32, (f64, f64)>> =
        StoredValue::new(HashMap::new());
    let gesture: StoredValue<Option<GestureStart>> = StoredValue::new(None);
    let last_tap_ms: StoredValue<f64> = StoredValue::new(0.0);

    Effect::new(move |_| {
        if is_open.get() {
            scale.set(1.0);
            tx.set(0.0);
            ty.set(0.0);
        }
    });

    let on_pointer_down = move |ev: PointerEvent| {
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(ev.pointer_id());
            }
        }
        let id = ev.pointer_id();
        let x = f64::from(ev.client_x());
        let y = f64::from(ev.client_y());
        pointers.update_value(|p| {
            p.insert(id, (x, y));
        });
        let count = pointers.with_value(HashMap::len);
        if count == 2 {
            let pts: Vec<(f64, f64)> = pointers.with_value(|p| p.values().copied().collect());
            let (p1, p2) = (pts[0], pts[1]);
            let dx = p1.0 - p2.0;
            let dy = p1.1 - p2.1;
            let distance = (dx * dx + dy * dy).sqrt().max(1.0);
            gesture.set_value(Some(GestureStart {
                distance,
                scale: scale.get_untracked(),
                tx: tx.get_untracked(),
                ty: ty.get_untracked(),
                mid_x: (p1.0 + p2.0) / 2.0,
                mid_y: (p1.1 + p2.1) / 2.0,
            }));
        }
    };

    let on_pointer_move = move |ev: PointerEvent| {
        let id = ev.pointer_id();
        let x = f64::from(ev.client_x());
        let y = f64::from(ev.client_y());
        let prev = pointers.try_update_value(|p| p.insert(id, (x, y))).flatten();
        let count = pointers.with_value(HashMap::len);
        if count == 2 {
            let Some(start) = gesture.get_value() else { return };
            let pts: Vec<(f64, f64)> = pointers.with_value(|p| p.values().copied().collect());
            let (p1, p2) = (pts[0], pts[1]);
            let dx = p1.0 - p2.0;
            let dy = p1.1 - p2.1;
            let distance = (dx * dx + dy * dy).sqrt().max(1.0);
            let mid_x = (p1.0 + p2.0) / 2.0;
            let mid_y = (p1.1 + p2.1) / 2.0;
            let new_scale = (start.scale * (distance / start.distance)).clamp(1.0, 5.0);
            scale.set(new_scale);
            tx.set(start.tx + (mid_x - start.mid_x));
            ty.set(start.ty + (mid_y - start.mid_y));
        } else if count == 1 && scale.get_untracked() > 1.001 {
            if let Some(prev) = prev {
                tx.update(|v| *v += x - prev.0);
                ty.update(|v| *v += y - prev.1);
            }
        }
    };

    let on_pointer_up = move |ev: PointerEvent| {
        let id = ev.pointer_id();
        pointers.update_value(|p| {
            p.remove(&id);
        });
        let count = pointers.with_value(HashMap::len);
        if count < 2 {
            gesture.set_value(None);
        }
        if count == 0 {
            let now = web_sys::window()
                .and_then(|w| w.performance())
                .map_or(0.0, |p| p.now());
            let last = last_tap_ms.get_value();
            if now - last < 300.0 {
                if scale.get_untracked() > 1.01 {
                    scale.set(1.0);
                    tx.set(0.0);
                    ty.set(0.0);
                } else {
                    scale.set(2.5);
                }
                last_tap_ms.set_value(0.0);
            } else {
                last_tap_ms.set_value(now);
            }
        }
    };

    let close_arc = Arc::new(on_close);
    let close_handler = {
        let cf = close_arc.clone();
        move |_| {
            if let Some(f) = cf.as_ref() {
                f();
            }
        }
    };

    let style_fn = move || {
        let s = scale.get();
        let x = tx.get();
        let y = ty.get();
        let snapped = (s - 1.0).abs() < 0.001 && x.abs() < 0.1 && y.abs() < 0.1;
        format!(
            "transform: translate({x}px, {y}px) scale({s}); transform-origin: center center; transition: {}; will-change: transform;",
            if snapped { "transform 0.2s ease" } else { "none" }
        )
    };

    view! {
        <Show when=move || is_open.get()>
            <div
                class="fixed inset-0 z-[60] flex flex-col bg-black select-none"
                style="touch-action: none; overscroll-behavior: contain;"
            >
                <button
                    class="absolute top-2 left-2 z-10 flex h-10 w-10 items-center justify-center rounded-full bg-black/40 text-white/90 hover:bg-black/60 transition-colors"
                    on:click=close_handler.clone()
                >
                    <Icon name="x" class_name="h-6 w-6"/>
                </button>

                <div
                    class="flex-1 flex items-center justify-center overflow-hidden"
                    on:pointerdown=on_pointer_down
                    on:pointermove=on_pointer_move
                    on:pointerup=on_pointer_up
                    on:pointercancel=on_pointer_up
                >
                    {move || src.get().map(|url| view! {
                        <img
                            src=url
                            class="max-h-full max-w-full object-contain pointer-events-none"
                            style=style_fn
                        />
                    })}
                </div>
            </div>
        </Show>
    }
}

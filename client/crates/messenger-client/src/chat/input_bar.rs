//! Message input bar — auto-resizing textarea, reply/edit preview, send/mic toggle,
//! recording UI, and attach menu.
use leptos::prelude::*;
use leptos::ev::KeyboardEvent;

use std::sync::Arc;
use wasm_bindgen::prelude::JsCast;
use crate::i18n::{Language, t};
use crate::icons::Icon;
use crate::components::textarea::Textarea;
use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem};
use crate::components::tooltip::Tooltip;

/// Preview mode for the input bar.
#[derive(Clone, Default)]
pub enum InputPreview {
    #[default]
    None,
    Reply {
        sender_name: String,
        content: String,
    },
    Edit {
        message_id: String,
        content: String,
    },
}

#[must_use]
#[component]
pub fn InputBar(
    #[prop(optional, into)] locale: Signal<Language>,
    #[prop(optional, into)] preview: InputPreview,
    #[prop(optional)] on_send: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_send_voice: Option<Box<dyn Fn(u32) + Send + Sync + 'static>>,
    #[prop(optional)] on_cancel_preview: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_attach_file: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_attach_photo: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let is_recording = RwSignal::new(false);
    let recording_duration = RwSignal::new(0u32);
    let recording_timer_id: RwSignal<Option<i32>> = RwSignal::new(None);

    // Auto-resize textarea reference
    let textarea_ref: NodeRef<leptos::html::Textarea> = NodeRef::new();

    // Wrap non-Clone callbacks in Arc for cloning inside view!
    let on_attach_photo_arc = on_attach_photo.map(Arc::new);
    let on_attach_file_arc = on_attach_file.map(Arc::new);

    // Extract closures outside view! to avoid proc macro parsing issues
    let on_attach_photo_cb = Box::new({
        let f = on_attach_photo_arc.clone();
        move || { if let Some(f) = f.as_ref() { f(); } }
    }) as Box<dyn Fn() + Send + Sync + 'static>;
    let on_attach_file_cb = Box::new({
        let f = on_attach_file_arc.clone();
        move || { if let Some(f) = f.as_ref() { f(); } }
    }) as Box<dyn Fn() + Send + Sync + 'static>;

    let has_text = move || !text.get().trim().is_empty();

    let on_send_arc = on_send.map(Arc::new);
    let on_send_for_keydown = on_send_arc.clone();

    let handle_send = move |()| {
        let msg = text.get().trim().to_string();
        if !msg.is_empty() {
            if let Some(ref f) = on_send_arc {
                f(msg);
            }
            text.set(String::new());
            // Clear textarea value via node_ref
            if let Some(el) = textarea_ref.get() {
                let textarea: &web_sys::HtmlTextAreaElement = el.unchecked_ref();
                textarea.set_value("");
                let _ = textarea.set_attribute("style", "height: auto");
            }
        }
    };

    let handle_send = Arc::new(handle_send);

    let handle_key_down = move |ev: KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            let msg = text.get().trim().to_string();
            if !msg.is_empty() {
                if let Some(ref f) = on_send_for_keydown {
                    f(msg);
                }
                text.set(String::new());
            }
        }
    };

    let handle_change = move |val: String| {
        text.set(val);
        // Auto-resize
        if let Some(el) = textarea_ref.get() {
            let textarea: &web_sys::HtmlTextAreaElement = el.unchecked_ref();
            let _ = textarea.set_attribute("style", "height: auto");
            let scroll_height = textarea.scroll_height();
            let new_height = scroll_height.min(120).max(36);
            let _ = textarea.set_attribute("style", &format!("height: {}px", new_height));
        }
    };

    // Voice recording – hold to record, release to send
    let on_send_voice_arc = on_send_voice.map(Arc::new);
    let recording_timer_id: RwSignal<Option<i32>> = RwSignal::new(None);

    let start_recording_core = std::sync::Arc::new({
        let is_recording = is_recording.clone();
        let recording_duration = recording_duration.clone();
        let recording_timer_id = recording_timer_id.clone();
        move || {
            if is_recording.get() { return; }
            is_recording.set(true);
            recording_duration.set(0);
            let dur = recording_duration;
            let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
                dur.update(|d| *d += 1);
            });
            if let Some(window) = web_sys::window() {
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        callback.as_ref().unchecked_ref(),
                        1_000,
                    )
                    .ok();
                if let Some(timer_id) = id {
                    recording_timer_id.set(Some(timer_id));
                }
            }
        }
    }) as std::sync::Arc<dyn Fn() + Send + Sync + 'static>;

    let stop_recording_core = std::sync::Arc::new({
        let is_recording = is_recording.clone();
        let recording_duration = recording_duration.clone();
        let recording_timer_id: RwSignal<Option<i32>> = recording_timer_id.clone();
        let on_send_voice = on_send_voice_arc.clone();
        move || {
            if !is_recording.get() { return; }
            is_recording.set(false);
            if let Some(id) = recording_timer_id.get() {
                if let Some(window) = web_sys::window() {
                    window.clear_interval_with_handle(id);
                }
                recording_timer_id.set(None);
            }
            let dur = recording_duration.get();
            if dur > 0 {
                if let Some(ref f) = on_send_voice {
                    f(dur);
                }
            }
            recording_duration.set(0);
        }
    }) as std::sync::Arc<dyn Fn() + Send + Sync + 'static>;

    let on_mouse_down = {
        let core = start_recording_core.clone();
        move |_: leptos::ev::MouseEvent| core()
    };
    let on_mouse_up = {
        let core = stop_recording_core.clone();
        move |_: leptos::ev::MouseEvent| core()
    };
    let on_mouse_leave = {
        let core = stop_recording_core.clone();
        move |_: leptos::ev::MouseEvent| core()
    };
    let on_touch_start = {
        let core = start_recording_core.clone();
        move |_: leptos::ev::TouchEvent| core()
    };
    let on_touch_end = {
        let core = stop_recording_core.clone();
        move |_: leptos::ev::TouchEvent| core()
    };
    let on_touch_move = move |_: leptos::ev::TouchEvent| {};
    // Pre-clone for separate reactive blocks inside view!
    let on_mouse_up_recording = on_mouse_up.clone();

    let on_change_cb = Box::new(handle_change) as Box<dyn Fn(String) + Send + Sync + 'static>;
    let on_key_down_cb = Box::new(handle_key_down) as Box<dyn Fn(KeyboardEvent) + Send + Sync + 'static>;

    view! {
        <div class="border-t border-border bg-card px-2 py-1.5">
            // Preview bar (reply or edit indicator)
            {match &preview {
                InputPreview::Reply { sender_name, content } => {
                    view! {
                        <div class="flex items-center gap-2 mb-1.5 px-2 py-1 rounded-lg bg-accent/30 border-l-2 border-primary">
                            <div class="flex items-center gap-1.5 min-w-0 flex-1">
                                <Icon name="reply" class_name="h-4 w-4 shrink-0 text-primary"/>
                                <div class="min-w-0">
                                    <p class="text-xs font-medium text-primary truncate">{sender_name.clone()}</p>
                                    <p class="text-xs text-muted-foreground line-clamp-1">{content.clone()}</p>
                                </div>
                            </div>
                            <button
                                class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md hover:bg-accent"
                                on:click=move |_| { if let Some(f) = on_cancel_preview.as_ref() { f(); } }
                            >
                                <Icon name="x" class_name="h-3.5 w-3.5"/>
                            </button>
                        </div>
                    }.into_any()
                }
                InputPreview::Edit { content, .. } => {
                    view! {
                        <div class="flex items-center gap-2 mb-1.5 px-2 py-1 rounded-lg bg-accent/30 border-l-2 border-warning">
                            <div class="flex items-center gap-1.5 min-w-0 flex-1">
                                <Icon name="edit" class_name="h-4 w-4 shrink-0 text-warning"/>
                                <div class="min-w-0">
                                    <p class="text-xs font-medium text-warning-foreground">{t(locale.get(), "message.editing")}</p>
                                    <p class="text-xs text-muted-foreground line-clamp-1">{content.clone()}</p>
                                </div>
                            </div>
                            <button
                                class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md hover:bg-accent"
                                on:click=move |_| { if let Some(f) = on_cancel_preview.as_ref() { f(); } }
                            >
                                <Icon name="x" class_name="h-3.5 w-3.5"/>
                            </button>
                        </div>
                    }.into_any()
                }
                InputPreview::None => view! {}.into_any(),
            }}

            // Recording UI
            {move || {
                if !is_recording.get() { return None; }
                let on_click_stop = on_mouse_up_recording.clone();
                let dur = move || format!("{}:{:02}", recording_duration.get() / 60, recording_duration.get() % 60);
                Some(view! {
                    <div class="flex items-center gap-2 mb-1.5 px-2 py-1.5 rounded-lg bg-destructive/10">
                        <div class="flex items-center gap-2">
                            <span class="h-3 w-3 animate-pulse rounded-full bg-destructive"/>
                            <span class="text-sm font-medium text-destructive-foreground">{t(locale.get(), "message.recording")}</span>
                        </div>
                        <span class="text-sm tabular-nums text-muted-foreground">
                            {dur}
                        </span>
                        <div class="flex-1 flex items-center gap-px h-5">
                            {(0..30).map(|_| {
                                let h = (fast_rand() * 80.0 + 10.0) as u32;
                                view! {
                                    <div class="flex-1 rounded-full bg-destructive/40" style=format!("height:{h}%")/>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                        <button
                            class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground hover:bg-muted/80 transition-colors"
                            on:click=on_click_stop
                        >
                            <Icon name="x" class_name="h-3.5 w-3.5"/>
                        </button>
                    </div>
                })
            }}

            // Input row
            <div class="flex items-end gap-1.5">
                // Attach button
                <DropdownMenu>
                    <DropdownMenuTrigger>
                        <Tooltip text={t(locale.get(), "attach.file")}>
                            <button class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors">
                                <Icon name="paperclip" class_name="h-4 w-4 text-muted-foreground"/>
                            </button>
                        </Tooltip>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent class="min-w-[10rem]" align="start">
                        <DropdownMenuItem on_click=on_attach_photo_cb>
                            <Icon name="image" class_name="mr-2 h-4 w-4"/>
                            {t(locale.get(), "attach.photo")}
                        </DropdownMenuItem>
                        <DropdownMenuItem on_click=on_attach_file_cb>
                            <Icon name="file" class_name="mr-2 h-4 w-4"/>
                            {t(locale.get(), "attach.file")}
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>

                // Textarea
                <div class="flex-1 relative">
                    <Textarea
                        placeholder={t(locale.get(), "message.placeholder")}
                        class="min-h-[36px] max-h-[120px] resize-none py-1.5 text-sm"
                        rows=1u32
                        on_change=on_change_cb
                        on_key_down=on_key_down_cb
                        node_ref=textarea_ref
                    />
                </div>

            // Send / Mic button
            {move || {
                let handle_send = handle_send.clone();
                if has_text() {
                    view! {
                        <button
                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                            on:click=move |_| handle_send(())
                        >
                            <Icon name="send" class_name="h-4 w-4"/>
                        </button>
                    }.into_any()
                } else if !is_recording.get() {
                    let on_mouse_down = on_mouse_down.clone();
                    let on_mouse_up = on_mouse_up.clone();
                    let on_mouse_leave = on_mouse_leave.clone();
                    let on_touch_start = on_touch_start.clone();
                    let on_touch_end = on_touch_end.clone();
                    let on_touch_move = on_touch_move.clone();
                    view! {
                        <button
                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors active:bg-primary/20"
                            on:mousedown=on_mouse_down
                            on:mouseup=on_mouse_up
                            on:mouseleave=on_mouse_leave
                            on:touchstart=on_touch_start
                            on:touchend=on_touch_end
                            on:touchmove=on_touch_move
                        >
                            <Icon name="mic" class_name="h-4 w-4 text-muted-foreground"/>
                        </button>
                    }.into_any()
                } else {
                    let on_mouse_up = on_mouse_up.clone();
                    let on_mouse_leave = on_mouse_leave.clone();
                    let on_touch_end = on_touch_end.clone();
                    view! {
                        <button
                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-destructive text-destructive-foreground transition-colors"
                            on:mouseup=on_mouse_up
                            on:mouseleave=on_mouse_leave
                            on:touchend=on_touch_end
                        >
                            <Icon name="mic" class_name="h-4 w-4"/>
                        </button>
                    }.into_any()
                }
            }}
            </div>
        </div>
    }
}

/// Simple deterministic pseudo-random for waveform simulation.
fn fast_rand() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<f64> = const { Cell::new(0.5) };
    }
    SEED.with(|s| {
        let val = (s.get() * 1.618 + 0.5) % 1.0;
        s.set(val);
        val
    })
}

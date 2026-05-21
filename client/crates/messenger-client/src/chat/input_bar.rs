//! Message input bar — auto-resizing textarea, reply/edit preview, send/mic toggle,
//! recording UI, emoji picker, and attach menu.
use leptos::prelude::*;
use leptos::ev::KeyboardEvent;

use std::sync::Arc;
use wasm_bindgen::prelude::JsCast;
use crate::i18n::{Language, t};
use crate::icons::Icon;
use crate::components::textarea::Textarea;
use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator};
use crate::components::sheet::{Sheet, SheetHeader, SheetTitle};
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
    #[prop(optional, into)] lang: Signal<Language>,
    #[prop(optional, into)] preview: InputPreview,
    #[prop(optional)] on_send: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_cancel_preview: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_attach_file: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_attach_photo: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let is_recording = RwSignal::new(false);
    let show_emoji_picker = RwSignal::new(false);
    let recording_duration = RwSignal::new(0u32);
    let is_focused = RwSignal::new(false);

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
    let on_close_emoji_cb = Box::new(move || show_emoji_picker.set(false)) as Box<dyn Fn() + Send + Sync + 'static>;

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
            let new_height = scroll_height.min(200).max(40);
            let _ = textarea.set_attribute("style", &format!("height: {}px", new_height));
        }
    };

    let start_recording = move |_| {
        is_recording.set(true);
        recording_duration.set(0);
    };

    let stop_recording = move |_| {
        is_recording.set(false);
    };

    let insert_emoji = move |emoji: &str| {
        let current = text.get();
        text.set(format!("{current}{emoji}"));
    };

    let on_change_cb = Box::new(handle_change) as Box<dyn Fn(String) + Send + Sync + 'static>;
    let on_key_down_cb = Box::new(handle_key_down) as Box<dyn Fn(KeyboardEvent) + Send + Sync + 'static>;

    view! {
        <div class="border-t border-border bg-card px-3 py-3">
            // Preview bar (reply or edit indicator)
            {match &preview {
                InputPreview::Reply { sender_name, content } => {
                    view! {
                        <div class="flex items-center gap-2 mb-2 px-2 py-1.5 rounded-lg bg-accent/30 border-l-2 border-primary">
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
                        <div class="flex items-center gap-2 mb-2 px-2 py-1.5 rounded-lg bg-accent/30 border-l-2 border-warning">
                            <div class="flex items-center gap-1.5 min-w-0 flex-1">
                                <Icon name="edit" class_name="h-4 w-4 shrink-0 text-warning"/>
                                <div class="min-w-0">
                                    <p class="text-xs font-medium text-warning-foreground">"Editing"</p>
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
            <Show when=move || is_recording.get()>
                <div class="flex items-center gap-3 mb-2 px-2 py-2 rounded-lg bg-destructive/10">
                    <div class="flex items-center gap-2">
                        <span class="h-3 w-3 animate-pulse rounded-full bg-destructive"/>
                        <span class="text-sm font-medium text-destructive-foreground">"Recording..."</span>
                    </div>
                    <span class="text-sm tabular-nums text-muted-foreground">
                        {move || format!("{}:{:02}", recording_duration.get() / 60, recording_duration.get() % 60)}
                    </span>
                    <div class="flex-1 flex items-center gap-px h-6">
                        // Simulated waveform during recording
                        {(0..40).map(|_| {
                            let h = (fast_rand() * 80.0 + 10.0) as u32;
                            view! {
                                <div class="flex-1 rounded-full bg-destructive/40" style=format!("height:{h}%")/>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                    <button
                        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-destructive text-destructive-foreground hover:bg-destructive/90 transition-colors"
                        on:click=stop_recording
                    >
                        <Icon name="mic-off" class_name="h-4 w-4"/>
                    </button>
                </div>
            </Show>

            // Input row
            <div class="flex items-end gap-2">
                // Attach button
                <DropdownMenu>
                    <DropdownMenuTrigger>
                        <Tooltip text={t(lang.get(), "attach.file").to_string()}>
                            <button class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors">
                                <Icon name="paperclip" class_name="h-5 w-5 text-muted-foreground"/>
                            </button>
                        </Tooltip>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent class="min-w-[10rem]" align="start">
                        <DropdownMenuItem on_click=on_attach_photo_cb>
                            <Icon name="image" class_name="mr-2 h-4 w-4"/>
                            {t(lang.get(), "attach.photo")}
                        </DropdownMenuItem>
                        <DropdownMenuItem on_click=on_attach_file_cb>
                            <Icon name="file" class_name="mr-2 h-4 w-4"/>
                            {t(lang.get(), "attach.file")}
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>

                // Emoji picker button
                <button
                    class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors"
                    on:click=move |_| show_emoji_picker.set(!show_emoji_picker.get())
                >
                    <Icon name="smile" class_name="h-5 w-5 text-muted-foreground"/>
                </button>

                // Textarea
                <div class="flex-1 relative">
                    <Textarea
                        placeholder={t(lang.get(), "message.placeholder").to_string()}
                        class="min-h-[40px] max-h-[200px] resize-none py-2.5 text-sm"
                        rows=1u32
                        on_change=on_change_cb
                        on_key_down=on_key_down_cb
                        node_ref=textarea_ref
                    />
                </div>

            // Send / Mic button
            {move || {
                let handle_send = handle_send.clone();
                if has_text() || is_recording.get() {
                    view! {
                        <button
                            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                            on:click=move |_| handle_send(())
                        >
                            <Icon name="send" class_name="h-5 w-5"/>
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button
                            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors"
                            on:click=start_recording
                        >
                            <Icon name="mic" class_name="h-5 w-5 text-muted-foreground"/>
                        </button>
                    }.into_any()
                }
            }}
            </div>
        </div>

        // Emoji picker sheet
        <Sheet
            is_open=Signal::derive(move || show_emoji_picker.get())
            on_close=on_close_emoji_cb
            side="bottom".to_string()
        >
            <SheetHeader>
                <SheetTitle>"Emoji"</SheetTitle>
            </SheetHeader>
            <div class="grid grid-cols-8 gap-2 p-2">
                {EMOJI_LIST.iter().map(|emoji| {
                    let e = *emoji;
                    view! {
                        <button
                            class="flex h-9 w-9 items-center justify-center rounded-md hover:bg-accent text-lg transition-colors"
                            on:click=move |_| insert_emoji(e)
                        >
                            {e}
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </Sheet>
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

/// Common emoji palette.
const EMOJI_LIST: &[&str] = &[
    "😀", "😃", "😄", "😁", "😅", "😂", "🤣", "😊",
    "😍", "🥰", "😘", "😜", "👍", "👎", "👌", "✌️",
    "❤️", "🔥", "💯", "✅", "⭐", "🎉", "🎊", "🙏",
    "😂", "😢", "😭", "😤", "😡", "🥺", "😳", "🤔",
    "🎈", "🎁", "🎂", "💪", "🤝", "👋", "✋", "💀",
];

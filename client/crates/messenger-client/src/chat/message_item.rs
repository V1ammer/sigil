//! Individual message bubble — renders text, voice, image, video, file, system, and
//! deleted messages with reactions, reply quoting, thread indicator, and context menu.
use leptos::prelude::*;
use leptos::ev::PointerEvent;
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use crate::i18n::{Language, t, format_time};
use crate::mock::Message;
use crate::icons::Icon;
use super::voice_message::VoiceMessage;
use super::image_lightbox::ImageLightbox;
use crate::components::context_menu::{ContextMenu, ContextMenuTrigger, ContextMenuContent, ContextMenuItem, ContextMenuSeparator};
use crate::components::sheet::{Sheet, SheetHeader, SheetTitle};
use crate::components::tooltip::Tooltip;

#[must_use]
#[component]
pub fn MessageItem(
    #[prop(optional, into)] lang: Signal<Language>,
    message: Message,
    #[prop(optional, into)] is_first_in_group: bool,
    #[prop(optional, into)] is_last_in_group: bool,
    #[prop(optional, into)] is_mobile: Signal<bool>,
    #[prop(optional)] on_thread_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_thread_open: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_media_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_avatar_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_reply: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_edit: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_reaction: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
) -> impl IntoView {
    let msg = message;
    let is_own = msg.is_own;

    // System message — full-width centered
    if msg.msg_type == "system" {
        return view! {
            <div class="flex justify-center py-1">
                <span class="inline-flex items-center gap-1.5 rounded-full bg-muted/50 px-3 py-1 text-xs text-muted-foreground">
                    <Icon name="alert-circle" class_name="h-3 w-3 shrink-0"/>
                    {msg.content.clone()}
                </span>
            </div>
        }.into_any();
    }

    // Deleted message
    if msg.is_deleted {
        return view! {
            <div class=format!("flex {} mb-1", if is_own { "justify-end" } else { "justify-start" })>
                <div class="max-w-[75%] rounded-lg px-3 py-2 italic text-muted-foreground bg-muted/30 text-sm">
                    {t(lang.get(), "message.deleted")}
                </div>
            </div>
        }.into_any();
    }

    let align = if is_own { "justify-end" } else { "justify-start" };
    let bubble_class = if is_own {
        "bg-message-own text-message-own-foreground rounded-2xl rounded-br-sm"
    } else {
        "bg-message-other text-message-other-foreground rounded-2xl rounded-bl-sm"
    };

    // Mobile action sheet
    let menu_open = RwSignal::new(false);

    // Long-press tracking: fire menu after 400ms of hold without movement > 10px.
    // Cancel on pointermove past threshold or pointerup before timer fires.
    // State lives in RwSignals so handlers stay Send + Sync (Leptos bound).
    const LONG_PRESS_MS: i32 = 400;
    const MOVE_CANCEL_PX: f64 = 10.0;
    let timer_id: RwSignal<Option<i32>> = RwSignal::new(None);
    let pointer_start: RwSignal<(f64, f64)> = RwSignal::new((0.0, 0.0));

    let clear_timer = move || {
        if let Some(id) = timer_id.get_untracked() {
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(id);
            }
            timer_id.set(None);
        }
    };

    let on_pointerdown = move |e: PointerEvent| {
        // Only handle primary pointer (touch or left mouse). Right mouse handled by oncontextmenu.
        if e.button() > 0 {
            return;
        }
        clear_timer();
        pointer_start.set((e.client_x() as f64, e.client_y() as f64));
        let cb = Closure::<dyn FnMut()>::new(move || {
            menu_open.set(true);
            timer_id.set(None);
        });
        if let Some(window) = web_sys::window() {
            if let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                LONG_PRESS_MS,
            ) {
                timer_id.set(Some(id));
            }
        }
        // Leak one closure per press — released on next press or app teardown.
        cb.forget();
    };

    let on_pointermove = move |e: PointerEvent| {
        let (sx, sy) = pointer_start.get_untracked();
        let dx = e.client_x() as f64 - sx;
        let dy = e.client_y() as f64 - sy;
        if (dx * dx + dy * dy).sqrt() > MOVE_CANCEL_PX {
            clear_timer();
        }
    };

    let on_pointerup = move |_: PointerEvent| clear_timer();
    let on_pointercancel = move |_: PointerEvent| clear_timer();

    let on_contextmenu = move |e: leptos::ev::MouseEvent| {
        // Desktop right-click: open the same action sheet, prevent native menu.
        e.prevent_default();
        menu_open.set(true);
    };

    let show_avatar = !is_own && is_first_in_group;
    let show_sender_name = !is_own && is_first_in_group && msg.sender_name != "System";

    // Clone things we need inside closures
    let msg_clone_for_content = msg.clone();
    let msg_clone_for_context = msg.clone();
    let lang_clone = lang;

    // Build the context menu content outside view! to avoid proc macro issues
    let on_reply = on_reply.map(std::sync::Arc::new);
    let on_edit = on_edit.map(std::sync::Arc::new);
    let on_delete = on_delete.map(std::sync::Arc::new);
    let on_thread_open = on_thread_open.map(std::sync::Arc::new);
    let on_reaction = on_reaction.map(std::sync::Arc::new);
    // Clone for the quick-reaction picker inside the bottom sheet — the bubble's
    // reaction tap-handlers consume the original further down.
    let on_reaction_for_sheet = on_reaction.clone();
    let menu_content = {
        let msg_ctx = msg.clone();
        let lang_clone = lang.get();
        let on_reply_arc = on_reply.clone();
        let on_edit_arc = on_edit.clone();
        let on_delete_arc = on_delete.clone();
        let on_thread_arc = on_thread_open.clone();
        Box::new(move || {
            let views = message_context_menu_items(
                &msg_ctx,
                lang_clone,
                on_reply_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                on_edit_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                on_delete_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                on_thread_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
            );

    view! {
                <ContextMenuContent>
                    {views}
                </ContextMenuContent>
            }.into_any()
        }) as Box<dyn Fn() -> AnyView + Send + Sync + 'static>
    };
    let on_close_sheet_cb = Box::new(move || menu_open.set(false)) as Box<dyn Fn() + Send + Sync + 'static>;
    let on_media_click_arc = std::sync::Arc::new(on_media_click);

    view! {
        <div class=format!("flex {} relative group mb-0.5", align)>
            // 75% cap lives here on a wrapper sized from the stable list column.
            // On the nested flex-col it caused a recursive sizing loop that
            // shrank bubbles to min-content and forced mid-word wraps.
            <div class="max-w-[75%]">
            // Action menu wrapper — same bottom sheet on desktop right-click and mobile long-press.
            <ContextMenu
                menu=menu_content
            >
                <ContextMenuTrigger>
                    <div
                        class="flex items-end gap-2"
                        style="touch-action: pan-y"
                        on:pointerdown=on_pointerdown
                        on:pointermove=on_pointermove
                        on:pointerup=on_pointerup
                        on:pointercancel=on_pointercancel
                        on:contextmenu=on_contextmenu
                    >
                        // Avatar column for other users
                        {if show_avatar {
                            view! {
                                <button
                                    class="mb-1 shrink-0"
                                    on:click=move |_| {
                                        if let Some(f) = on_avatar_click.as_ref() { f(); }
                                    }
                                >
                                    <div class="h-8 w-8 rounded-full bg-muted flex items-center justify-center text-xs font-medium text-muted-foreground">
                                        {crate::components::avatar::get_initials(&msg.sender_name)}
                                    </div>
                                </button>
                            }.into_any()
                        } else if !is_own && !is_first_in_group {
                            // Spacer to maintain alignment when there's no avatar
                            view! { <div class="w-8 shrink-0" /> }.into_any()
                        } else {
                            view! {}.into_any()
                        }}

                        <div class=format!("flex flex-col min-w-0 {}", if is_own { "items-end" } else { "items-start" })>
                            // Sender name (first in group only)
                            {if show_sender_name {
                                view! {
                                    <span class="ml-1 mb-0.5 text-xs font-medium text-muted-foreground">
                                        {msg.sender_name.clone()}
                                    </span>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}

                            // Reply quote
                            {if let Some(ref reply) = msg.reply_to {
                                view! {
                                    <button
                                        class=format!(
                                            "mb-0.5 max-w-full cursor-pointer items-center gap-2 rounded-lg border-l-2 border-primary bg-card/50 px-2.5 py-1.5 text-left transition-colors hover:bg-accent/50 {}",
                                            if is_own { "self-end" } else { "self-start" }
                                        )
                                        on:click=move |_| {}
                                    >
                                        <div class="min-w-0">
                                            <p class="text-xs font-medium text-primary truncate">{reply.sender_name.clone()}</p>
                                            <p class="text-xs text-muted-foreground line-clamp-1">{reply.content.clone()}</p>
                                        </div>
                                    </button>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}

                            <div class=format!("px-3 py-2 text-sm shadow-sm break-words {bubble_class}")>
                                // Message content based on type
                                {render_content(msg.clone(), on_media_click_arc.clone(), lang.get())}

                                // Status row: edited + time + status icon
                                <div class=format!("flex items-center gap-1 mt-1 {}", if is_own { "justify-end" } else { "justify-start" })>
                                    {if msg.is_edited {
                                        view! {
                                            <span class="text-[10px] opacity-70">{t(lang.get(), "message.edited")}</span>
                                        }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }}
                                    <span class="text-[10px] opacity-70 leading-none">
                                        {format_time(msg.timestamp, lang.get())}
                                    </span>
                                    {if is_own {
                                        view! {
                                            <span class="text-[10px] leading-none">
                                                {match msg.status.as_str() {
                                                    "sending" => view! { <Icon name="loader" class_name="h-3 w-3 opacity-60"/> }.into_any(),
                                                    "sent" => view! { <Icon name="check" class_name="h-3 w-3 opacity-60"/> }.into_any(),
                                                    "delivered" => view! { <Icon name="check-check" class_name="h-3 w-3 opacity-60"/> }.into_any(),
                                                    "read" => view! { <Icon name="check-check" class_name="h-3 w-3 text-read"/> }.into_any(),
                                                    _ => view! {}.into_any(),
                                                }}
                                            </span>
                                        }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }}
                                </div>
                            </div>

                            // Reactions row
                            {if msg.reactions.is_empty() {
                                view! {}.into_any()
                            } else {
                                view! {
                                    <div class=format!("flex flex-wrap gap-0.5 -mt-1.5 {}", if is_own { "justify-end" } else { "justify-start" })>
                                        {msg.reactions.iter().map(|r| {
                                            let bg = if r.has_own { "bg-primary/20" } else { "bg-muted/70" };
                                            let emoji = r.emoji.clone();
                                            let count = r.count;
                                            let react_cb = on_reaction.clone();
                                            let emoji_for_span = emoji.clone();
                                            view! {
                                                <button
                                                    class=format!("inline-flex items-center gap-0.5 rounded-full px-1.5 py-0.5 text-xs shadow-sm hover:bg-accent transition-colors {bg}")
                                                    on:click=move |_| {
                                                        if let Some(ref f) = react_cb {
                                                            f(emoji.clone());
                                                        }
                                                    }
                                                >
                                                    <span>{emoji_for_span}</span>
                                                    {if count > 1 {
                                                        view! { <span class="text-[10px] font-medium text-foreground">{count}</span> }.into_any()
                                                    } else {
                                                        view! {}.into_any()
                                                    }}
                                                </button>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }}

                            // Thread indicator
                            {if let Some(count) = msg.thread_count {
                                view! {
                                    <button
                                        class=format!(
                                            "mt-0.5 flex items-center gap-1 rounded px-2 py-0.5 text-xs font-medium text-primary hover:bg-accent/50 transition-colors {}",
                                            if is_own { "self-end" } else { "self-start" }
                                        )
                                        on:click=move |_| {
                                            if let Some(f) = on_thread_click.as_ref() { f(); }
                                        }
                                    >
                                        <Icon name="message-square" class_name="h-3 w-3"/>
                                        <span>{format!("{count} {}", t(lang.get(), "message.replies"))}</span>
                                    </button>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                        </div>
                    </div>
                </ContextMenuTrigger>
            </ContextMenu>
            </div>
        </div>

        // Mobile action sheet (bottom sheet)
        <Sheet
            is_open=Signal::derive(move || menu_open.get())
            on_close=on_close_sheet_cb
            side="bottom".to_string()
        >
            <SheetHeader>
                <SheetTitle>{t(lang.get(), "message.reply")}</SheetTitle>
            </SheetHeader>
            // Quick reaction emojis — tap to react and dismiss the sheet.
            {
                let close_after = menu_open;
                let emojis = ["👍", "❤️", "😄", "🎉", "😢", "🙏"];
                view! {
                    <div class="flex items-center justify-around gap-1 pb-3 mb-2 border-b border-border">
                        {emojis.into_iter().map(|e| {
                            let cb = on_reaction_for_sheet.clone();
                            let emoji_owned = e.to_string();
                            view! {
                                <button
                                    class="flex h-11 w-11 items-center justify-center rounded-full text-xl hover:bg-accent active:scale-95 transition-transform"
                                    on:click=move |_| {
                                        if let Some(ref f) = cb { f(emoji_owned.clone()); }
                                        close_after.set(false);
                                    }
                                >
                                    {e}
                                </button>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }
            }
            <div class="space-y-1">
                {message_context_menu_items(
                    &msg_clone_for_context,
                    lang_clone.get(),
                    on_reply.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                    on_edit.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                    on_delete.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                    on_thread_open.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                )
                    .into_iter()
                    .map(|item| item)
                    .collect::<Vec<_>>()
                }
            </div>
        </Sheet>
    }.into_any()
}

/// Render message content based on type.
fn render_content(msg: Message, on_media_click: std::sync::Arc<Option<Box<dyn Fn() + Send + Sync + 'static>>>, l: Language) -> AnyView {
    match msg.msg_type.as_str() {
        "voice" => {
            // Duration arrives in seconds via the bridge; the player uses ms.
            let duration_ms = msg.duration.unwrap_or(0) * 1000;
            view! {
                <VoiceMessage
                    duration_ms=duration_ms
                    waveform=msg.waveform.clone()
                    transcription=msg.transcription.clone().unwrap_or_default()
                    is_own=msg.is_own
                    attachment_id=msg.attachment_id.clone().unwrap_or_default()
                    decryption_key=msg.decryption_key.clone().unwrap_or_default()
                    mime=msg.mime_type.clone().unwrap_or_default()
                />
            }.into_any()
        }
        "image" => {
            let _ = on_media_click.clone();
            let attachment_id = msg.attachment_id.clone();
            let decryption_key = msg.decryption_key.clone();
            let mime = msg.mime_type.clone().unwrap_or_else(|| "image/jpeg".into());
            let blob_url: RwSignal<Option<String>> = RwSignal::new(None);
            let err: RwSignal<Option<String>> = RwSignal::new(None);
            let lightbox_open: RwSignal<bool> = RwSignal::new(false);

            // Auto-fetch and decrypt on first render. Caches the object URL.
            if let (Some(aid), Some(key_b64)) = (attachment_id, decryption_key) {
                leptos::task::spawn_local(async move {
                    use base64::Engine as _;
                    let api = match crate::state::session::build_api_client() {
                        Some(a) => a,
                        None => { err.set(Some("no api".into())); return; }
                    };
                    let attachment_id = match aid.parse::<uuid::Uuid>() {
                        Ok(u) => u,
                        Err(_) => { err.set(Some("bad id".into())); return; }
                    };
                    let ct = match api.download_attachment(attachment_id, None).await {
                        Ok(b) => b,
                        Err(e) => { err.set(Some(format!("dl: {e}"))); return; }
                    };
                    let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&key_b64) {
                        Ok(b) if b.len() == 32 => b,
                        _ => { err.set(Some("bad key".into())); return; }
                    };
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&key_bytes);
                    let plain = match messenger_core::attachment_crypto::decrypt_attachment(&key, &ct) {
                        Ok(p) => p,
                        Err(e) => { err.set(Some(format!("decrypt: {e:?}"))); return; }
                    };
                    let u8a = js_sys::Uint8Array::from(plain.as_slice());
                    let arr = js_sys::Array::new();
                    arr.push(&u8a.into());
                    let mut bag = web_sys::BlobPropertyBag::new();
                    bag.type_(&mime);
                    if let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &bag) {
                        if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                            blob_url.set(Some(url));
                        }
                    }
                });
            }

            let on_close_lightbox = Box::new(move || lightbox_open.set(false))
                as Box<dyn Fn() + Send + Sync + 'static>;
            view! {
                <button
                    class="block overflow-hidden rounded-lg -mx-1 -mt-1 mb-1"
                    on:click=move |_| {
                        if blob_url.get_untracked().is_some() {
                            crate::state::back_stack::push(move || lightbox_open.set(false));
                            lightbox_open.set(true);
                        }
                    }
                >
                    <div class="aspect-video max-h-64 w-full bg-muted flex items-center justify-center">
                        {move || {
                            if let Some(url) = blob_url.get() {
                                view! { <img src=url alt="Image" class="h-full w-full object-cover"/> }.into_any()
                            } else if let Some(e) = err.get() {
                                view! {
                                    <div class="flex flex-col items-center gap-1 text-destructive">
                                        <Icon name="image" class_name="h-8 w-8"/>
                                        <span class="text-[10px]">{e}</span>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="flex flex-col items-center gap-2 text-muted-foreground">
                                        <span class="h-8 w-8 inline-block rounded-full border-2 border-current border-t-transparent animate-spin"/>
                                        <span class="text-xs">{t(l, "message.image")}</span>
                                    </div>
                                }.into_any()
                            }
                        }}
                    </div>
                </button>
                <ImageLightbox
                    is_open=Signal::derive(move || lightbox_open.get())
                    on_close=on_close_lightbox
                    src=Signal::derive(move || blob_url.get())
                />
            }.into_any()
        }
        "video" => {
            let mc = on_media_click.clone();
            view! {
                <button
                    class="block overflow-hidden rounded-lg -mx-1 -mt-1 mb-1 relative"
                    on:click=move |_| {
                        if let Some(f) = mc.as_ref() { f(); }
                    }
                >
                    <div class="aspect-video max-h-64 w-full bg-muted flex items-center justify-center">
                        {if let Some(ref thumb) = msg.thumbnail_url {
                            view! {
                                <img src=thumb alt="Video" class="h-full w-full object-cover"/>
                            }.into_any()
                        } else {
                            view! {
                                <div class="flex flex-col items-center gap-2 text-muted-foreground">
                                    <Icon name="film" class_name="h-8 w-8"/>
                                    <span class="text-xs">{t(l, "message.video")}</span>
                                </div>
                            }.into_any()
                        }}
                        <div class="absolute inset-0 flex items-center justify-center">
                            <div class="flex h-12 w-12 items-center justify-center rounded-full bg-black/50 text-white">
                                <Icon name="play" class_name="h-6 w-6"/>
                            </div>
                        </div>
                    </div>
                </button>
            }.into_any()
        }
        "file" => {
            let file_name = msg.file_name.as_deref().unwrap_or("File").to_string();
            let attachment_id = msg.attachment_id.clone();
            let decryption_key = msg.decryption_key.clone();
            let mime = msg.mime_type.clone().unwrap_or_else(|| "application/octet-stream".into());
            let file_size = msg.file_size;

            let saved_path: RwSignal<Option<String>> = RwSignal::new(None);
            let downloading = RwSignal::new(false);

            // Probe whether this attachment is already on disk.
            if let Some(aid) = attachment_id.clone() {
                leptos::task::spawn_local(async move {
                    if let Ok(Some(p)) = crate::tauri_bridge::file_is_saved(&aid).await {
                        saved_path.set(Some(p));
                    }
                });
            }

            // Shared decrypt+save closure used by both the explicit tap and auto-download.
            let run_save = {
                let attachment_id = attachment_id.clone();
                let decryption_key = decryption_key.clone();
                let mime = mime.clone();
                let file_name = file_name.clone();
                let notifications = use_context::<crate::state::NotificationsState>();
                move || {
                    if downloading.get_untracked() || saved_path.get_untracked().is_some() {
                        return;
                    }
                    let aid = match attachment_id.clone() { Some(s) => s, None => return };
                    let key_b64 = match decryption_key.clone() { Some(s) => s, None => return };
                    let mime = mime.clone();
                    let file_name = file_name.clone();
                    let notifications = notifications.clone();
                    downloading.set(true);
                    leptos::task::spawn_local(async move {
                        use base64::Engine as _;
                        let api = match crate::state::session::build_api_client() {
                            Some(a) => a,
                            None => { downloading.set(false); return; }
                        };
                        let attachment_uuid = match aid.parse::<uuid::Uuid>() {
                            Ok(u) => u,
                            Err(_) => { downloading.set(false); return; }
                        };
                        let ct = match api.download_attachment(attachment_uuid, None).await {
                            Ok(b) => b,
                            Err(_) => { downloading.set(false); return; }
                        };
                        let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&key_b64) {
                            Ok(b) if b.len() == 32 => b,
                            _ => { downloading.set(false); return; }
                        };
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&key_bytes);
                        let plain = match messenger_core::attachment_crypto::decrypt_attachment(&key, &ct) {
                            Ok(p) => p,
                            Err(_) => { downloading.set(false); return; }
                        };
                        match crate::tauri_bridge::file_save(&plain, &file_name, &aid, &mime).await {
                            Ok(path) => {
                                saved_path.set(Some(path));
                                if let Some(n) = notifications.as_ref() {
                                    n.push(crate::state::notifications::ToastKind::Success,
                                        format!("{}: {}", t(l, "message.file.savedToDownloads"), file_name));
                                }
                            }
                            Err(e) => {
                                if let Some(n) = notifications.as_ref() {
                                    n.push(crate::state::notifications::ToastKind::Error,
                                        format!("{}: {}", t(l, "message.file.saveFailed"), e));
                                }
                            }
                        }
                        downloading.set(false);
                    });
                }
            };

            // Auto-download if user enabled it and the file fits the size limit.
            if let Some(settings) = use_context::<crate::state::SettingsState>() {
                let auto = settings.auto_download_files.get_untracked();
                let max_mb: u64 = settings.auto_download_max_mb.get_untracked().parse().unwrap_or(10);
                let within = file_size.map_or(false, |s| s <= max_mb.saturating_mul(1024 * 1024));
                if auto && within && attachment_id.is_some() && decryption_key.is_some() {
                    // Wait briefly so the is_saved probe can resolve first.
                    let run_save_dl = run_save.clone();
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(150).await;
                        if saved_path.get_untracked().is_none() {
                            run_save_dl();
                        }
                    });
                }
            }

            let on_download_click = {
                let run_save = run_save.clone();
                move |ev: leptos::ev::MouseEvent| {
                    ev.stop_propagation();
                    run_save();
                }
            };

            let on_row_click = {
                let mime = mime.clone();
                let run_save = run_save.clone();
                move |_: leptos::ev::MouseEvent| {
                    if let Some(path) = saved_path.get_untracked() {
                        let mime = mime.clone();
                        leptos::task::spawn_local(async move {
                            let _ = crate::tauri_bridge::file_open(&path, &mime).await;
                        });
                    } else if !downloading.get_untracked() {
                        run_save();
                    }
                }
            };

            view! {
                <button
                    class="w-full text-left flex items-center gap-3 rounded-lg -mx-1 -mt-1 mb-1 p-2 bg-muted/30 hover:bg-muted/50 transition-colors"
                    on:click=on_row_click
                >
                    <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                        {move || if saved_path.get().is_some() {
                            view! { <Icon name="check-circle" class_name="h-5 w-5"/> }.into_any()
                        } else {
                            view! { <Icon name="file" class_name="h-5 w-5"/> }.into_any()
                        }}
                    </div>
                    <div class="min-w-0 flex-1">
                        <p class="text-sm font-medium truncate">{file_name.clone()}</p>
                        {file_size.map(|size| {
                            view! {
                                <p class="text-xs text-muted-foreground">{format_file_size(size)}</p>
                            }.into_any()
                        }).unwrap_or_else(|| view! {}.into_any())}
                    </div>
                    {move || if saved_path.get().is_some() {
                        view! {}.into_any()
                    } else {
                        let click = on_download_click.clone();
                        view! {
                            <span
                                class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md hover:bg-accent"
                                on:click=click
                            >
                                {move || if downloading.get() {
                                    view! { <span class="block h-4 w-4 rounded-full border-2 border-current border-t-transparent animate-spin"/> }.into_any()
                                } else {
                                    view! { <Icon name="download" class_name="h-4 w-4"/> }.into_any()
                                }}
                            </span>
                        }.into_any()
                    }}
                </button>
            }.into_any()
        }
        _ => {
            // Text (default)
            view! {
                <p class="whitespace-pre-wrap break-words">{msg.content.clone()}</p>
            }.into_any()
        }
    }
}

/// Format file size in human-readable format.
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Build context menu items list.
fn message_context_menu_items(
    msg: &Message,
    lang: Language,
    on_reply: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    on_edit: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    on_delete: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    on_thread: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
) -> Vec<AnyView> {
    let mut views: Vec<AnyView> = Vec::new();

    // Reply
    let reply_cb = on_reply.map(|f| {
        Box::new(move || f()) as Box<dyn Fn() + Send + Sync + 'static>
    });
    views.push(view! {
        <ContextMenuItem on_click=reply_cb.unwrap_or_else(|| Box::new(|| {}))>
            <Icon name="reply" class_name="mr-2 h-4 w-4"/>
            {t(lang, "message.reply")}
        </ContextMenuItem>
    }.into_any());

    // Reply in thread — opens the thread panel for this message.
    let thread_cb = on_thread.map(|f| {
        Box::new(move || f()) as Box<dyn Fn() + Send + Sync + 'static>
    });
    views.push(view! {
        <ContextMenuItem on_click=thread_cb.unwrap_or_else(|| Box::new(|| {}))>
            <Icon name="message-square" class_name="mr-2 h-4 w-4"/>
            {t(lang, "message.replyThread")}
        </ContextMenuItem>
    }.into_any());

    views.push(view! { <ContextMenuSeparator/> }.into_any());

    // Copy for text messages
    if msg.msg_type == "text" && !msg.content.is_empty() {
        let content = msg.content.clone();
        let copy_cb = Box::new({
            let c = content.clone();
            move || {
                let _ = copy_to_clipboard(&c);
            }
        }) as Box<dyn Fn() + Send + Sync + 'static>;
        views.push(view! {
            <ContextMenuItem on_click=copy_cb>
                <Icon name="copy" class_name="mr-2 h-4 w-4"/>
                {t(lang, "message.copy")}
            </ContextMenuItem>
        }.into_any());
    }

    // Edit for own messages
    if msg.is_own && !msg.is_deleted {
        let edit_cb = on_edit.clone().map(|f| {
            Box::new(move || f()) as Box<dyn Fn() + Send + Sync + 'static>
        });
        views.push(view! {
            <ContextMenuItem on_click=edit_cb.unwrap_or_else(|| Box::new(|| {}))>
                <Icon name="edit" class_name="mr-2 h-4 w-4"/>
                {t(lang, "message.edit")}
            </ContextMenuItem>
        }.into_any());
    }

    views.push(view! {
        <ContextMenuItem>
            <Icon name="forward" class_name="mr-2 h-4 w-4"/>
            {t(lang, "message.forward")}
        </ContextMenuItem>
    }.into_any());

    views.push(view! { <ContextMenuSeparator/> }.into_any());

    // Delete
    let delete_cb = on_delete.map(|f| {
        Box::new(move || f()) as Box<dyn Fn() + Send + Sync + 'static>
    });
    views.push(view! {
        <ContextMenuItem
            class="text-destructive"
            on_click=delete_cb.unwrap_or_else(|| Box::new(|| {}))
        >
            <Icon name="trash" class_name="mr-2 h-4 w-4"/>
            {t(lang, "message.delete")}
        </ContextMenuItem>
    }.into_any());

    views
}

/// Copy text to clipboard (wasm-compatible).
fn copy_to_clipboard(text: &str) -> Result<(), ()> {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
    }
    Ok(())
}

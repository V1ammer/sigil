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

thread_local! {
    /// Session cache of decoded attachment object URLs, keyed by attachment id.
    /// The message list rebuilds every MessageItem whenever a message is sent or
    /// received, which would otherwise re-download and re-decrypt every image.
    /// Object URLs live for the document's lifetime, so reusing them is free.
    static ATTACHMENT_URL_CACHE: std::cell::RefCell<std::collections::HashMap<uuid::Uuid, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());

    /// Attachments the server has confirmed gone (HTTP 404). The blob was GC'd
    /// (its finalize never bound it to a message, back when finalize 500'd), so
    /// it will never return — caching the verdict stops every chat re-render /
    /// sync tick from firing another download at a dead id. Seeded from (and
    /// persisted to) localStorage so a dead id is requested at most once per
    /// device, ever — not once per page load.
    static ATTACHMENT_MISSING: std::cell::RefCell<std::collections::HashSet<uuid::Uuid>> =
        std::cell::RefCell::new(load_missing_attachments());
}

const MISSING_ATTACHMENTS_KEY: &str = "messenger_missing_attachments";

fn load_missing_attachments() -> std::collections::HashSet<uuid::Uuid> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(MISSING_ATTACHMENTS_KEY).ok().flatten())
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
        .map(|v| v.into_iter().filter_map(|s| s.parse().ok()).collect())
        .unwrap_or_default()
}

fn cached_attachment_url(id: uuid::Uuid) -> Option<String> {
    ATTACHMENT_URL_CACHE.with(|c| c.borrow().get(&id).cloned())
}

fn cache_attachment_url(id: uuid::Uuid, url: &str) {
    ATTACHMENT_URL_CACHE.with(|c| {
        c.borrow_mut().insert(id, url.to_string());
    });
}

fn attachment_known_missing(id: uuid::Uuid) -> bool {
    ATTACHMENT_MISSING.with(|c| c.borrow().contains(&id))
}

fn mark_attachment_missing(id: uuid::Uuid) {
    ATTACHMENT_MISSING.with(|c| {
        let mut set = c.borrow_mut();
        if !set.insert(id) {
            return;
        }
        // Persist so the dead id is never re-requested, even after a reload.
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let v: Vec<String> = set.iter().map(ToString::to_string).collect();
            if let Ok(j) = serde_json::to_string(&v) {
                let _ = storage.set_item(MISSING_ATTACHMENTS_KEY, &j);
            }
        }
    });
}

/// Download + decrypt a media attachment (image/video) and expose it as a Blob
/// object URL via `blob_url`. Reuses a cached URL across list rebuilds and
/// retries the download to ride out the sender's upload→finalize race.
fn load_media_blob(
    attachment_id: Option<String>,
    decryption_key: Option<String>,
    mime: String,
    blob_url: RwSignal<Option<String>>,
    err: RwSignal<Option<String>>,
    l: Language,
) {
    let parsed_id = attachment_id
        .as_ref()
        .and_then(|s| s.parse::<uuid::Uuid>().ok())
        .filter(|id| !id.is_nil());
    if let Some(url) = parsed_id.and_then(cached_attachment_url) {
        blob_url.set(Some(url));
        return;
    }
    // Already known gone — show the placeholder without re-hammering the server.
    if let Some(id) = parsed_id {
        if attachment_known_missing(id) {
            err.set(Some(t(l, "message.imageUnavailable").to_string()));
            return;
        }
    }
    let (Some(aid), Some(key_b64)) = (attachment_id, decryption_key) else {
        return;
    };
    leptos::task::spawn_local(async move {
        use base64::Engine as _;
        let Some(api) = crate::state::session::build_api_client() else {
            err.set(Some("no api".into()));
            return;
        };
        let Ok(attachment_id) = aid.parse::<uuid::Uuid>() else {
            err.set(Some("bad id".into()));
            return;
        };
        // Optimistic bubble while our own upload is in flight — no id yet.
        if attachment_id.is_nil() {
            return;
        }
        // Retry: recipient sees the message before the sender finalizes the
        // attachment, so the server briefly 403s.
        let backoffs_ms = [250u32, 500, 1000, 2000, 3000];
        let mut ct: Option<Vec<u8>> = None;
        for (attempt, delay) in std::iter::once(0u32).chain(backoffs_ms).enumerate() {
            if delay > 0 {
                gloo_timers::future::TimeoutFuture::new(delay).await;
            }
            match api.download_attachment(attachment_id, None).await {
                Ok(b) => {
                    ct = Some(b);
                    break;
                }
                // 404 = the blob is gone for good (GC'd). Retrying and re-trying
                // on every future render is pointless — remember it and stop.
                Err(messenger_core::api::ApiError::Api { status: 404, .. }) => {
                    mark_attachment_missing(attachment_id);
                    break;
                }
                Err(e) => tracing::warn!(
                    "media download attempt {} failed (att {}): {e}",
                    attempt + 1,
                    &attachment_id.to_string()[..8]
                ),
            }
        }
        let Some(ct) = ct else {
            err.set(Some(t(l, "message.imageUnavailable").to_string()));
            return;
        };
        let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&key_b64) {
            Ok(b) if b.len() == 32 => b,
            _ => {
                err.set(Some(t(l, "message.imageUnavailable").to_string()));
                return;
            }
        };
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        let Ok(plain) = messenger_core::attachment_crypto::decrypt_attachment_auto(&key, &ct) else {
            err.set(Some(t(l, "message.imageUnavailable").to_string()));
            return;
        };
        let u8a = js_sys::Uint8Array::from(plain.as_slice());
        let arr = js_sys::Array::new();
        arr.push(&u8a.into());
        let mut bag = web_sys::BlobPropertyBag::new();
        bag.type_(&mime);
        if let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &bag) {
            if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                cache_attachment_url(attachment_id, &url);
                blob_url.set(Some(url));
            }
        }
    });
}

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
    #[prop(optional)] on_forward: Option<Box<dyn Fn() + Send + Sync + 'static>>,
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
    let on_forward = on_forward.map(std::sync::Arc::new);
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
        let on_forward_arc = on_forward.clone();
        let on_thread_arc = on_thread_open.clone();
        Box::new(move || {
            let views = message_context_menu_items(
                &msg_ctx,
                lang_clone,
                on_reply_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                on_edit_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                on_delete_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
                on_forward_arc.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
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
                        // Avatar column for other users — show the sender's avatar
                        // image (from UsersState, reactive so it pops in once
                        // downloaded) and fall back to initials only when there's
                        // no avatar.
                        {if show_avatar {
                            let sender_name = msg.sender_name.clone();
                            let users_state = use_context::<crate::state::users::UsersState>();
                            let sender_uid = uuid::Uuid::parse_str(&msg.sender_id).ok();
                            let avatar_url = Signal::derive(move || {
                                let uid = sender_uid?;
                                users_state.as_ref()?.avatar_by_id.with(|m| m.get(&uid).cloned())
                            });
                            view! {
                                <button
                                    class="mb-1 shrink-0"
                                    on:click=move |_| {
                                        if let Some(f) = on_avatar_click.as_ref() { f(); }
                                    }
                                >
                                    <div class="h-8 w-8 overflow-hidden rounded-full bg-muted flex items-center justify-center text-xs font-medium text-muted-foreground">
                                        {move || match avatar_url.get() {
                                            Some(url) => view! {
                                                <img src=url alt="" class="h-full w-full object-cover"/>
                                            }.into_any(),
                                            None => crate::components::avatar::get_initials(&sender_name).into_any(),
                                        }}
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

                            // max-w-full: with items-end/start the column sizes
                            // children to content, so an unbroken word would
                            // otherwise stretch the bubble past the screen and
                            // defeat break-words.
                            <div class=format!("max-w-full px-3 py-2 text-sm shadow-sm break-words {bubble_class}")>
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
                                                    // Media messages already show a loader in the media
                                                    // placeholder while uploading — don't add a second
                                                    // spinner in the status row.
                                                    "sending" if matches!(msg.msg_type.as_str(), "image" | "video" | "audio" | "file" | "voice") => {
                                                        view! {}.into_any()
                                                    }
                                                    "sending" => view! { <Icon name="loader" class_name="h-3 w-3 opacity-60 animate-spin"/> }.into_any(),
                                                    "failed" => view! { <Icon name="alert-circle" class_name="h-3 w-3 text-destructive"/> }.into_any(),
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
                    on_forward.clone().map(|f| f as Arc<dyn Fn() + Send + Sync + 'static>),
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
    // For media messages `content` carries the optional caption, rendered under
    // the media so the two read as one message.
    let caption = msg.content.clone();
    let is_media = matches!(msg.msg_type.as_str(), "voice" | "image" | "video" | "audio" | "file");
    let media = match msg.msg_type.as_str() {
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

            load_media_blob(attachment_id, decryption_key, mime, blob_url, err, l);

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
            let _ = on_media_click.clone();
            let attachment_id = msg.attachment_id.clone();
            let decryption_key = msg.decryption_key.clone();
            let mime = msg.mime_type.clone().unwrap_or_else(|| "video/mp4".into());
            let err: RwSignal<Option<String>> = RwSignal::new(None);
            // Click-to-load: don't download every video on render — only when the
            // user taps play. Streams chunk-by-chunk (MediaSource) when possible,
            // else falls back to a whole-blob download.
            let started: RwSignal<bool> = RwSignal::new(false);
            let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();
            let kicked = RwSignal::new(false);
            {
                let aid = attachment_id.clone();
                let key = decryption_key.clone();
                let m = mime.clone();
                // Kick streaming once the <video> mounts after the user taps play.
                Effect::new(move |_| {
                    if !started.get() || kicked.get_untracked() {
                        return;
                    }
                    let Some(el) = video_ref.get() else { return };
                    kicked.set(true);
                    let media: web_sys::HtmlMediaElement =
                        el.unchecked_ref::<web_sys::HtmlMediaElement>().clone();
                    match (aid.clone(), key.clone()) {
                        (Some(aid), Some(key)) => {
                            // autoplay=false: the element's own `autoplay` attr
                            // starts it as soon as the first chunk buffers.
                            crate::chat::media_stream::play(media, aid, key, m.clone(), false, err, l);
                        }
                        _ => err.set(Some(t(l, "message.video").to_string())),
                    }
                });
            }

            view! {
                <div class="block overflow-hidden rounded-lg -mx-1 -mt-1 mb-1">
                    // Definite width so the bubble doesn't collapse to the play
                    // icon: matches an image's max footprint (max-h-64 × 16:9 ≈
                    // 455px), shrinking responsively via max-w-full.
                    <div class="aspect-video max-h-64 w-[455px] max-w-full bg-black flex items-center justify-center">
                        {move || {
                            if !started.get() {
                                view! {
                                    <button
                                        class="relative h-full w-full flex items-center justify-center"
                                        on:click=move |_| started.set(true)
                                    >
                                        <div class="flex flex-col items-center gap-2 text-muted-foreground">
                                            <Icon name="film" class_name="h-8 w-8"/>
                                            <span class="text-xs">{t(l, "message.video")}</span>
                                        </div>
                                        <div class="absolute inset-0 flex items-center justify-center">
                                            <div class="flex h-12 w-12 items-center justify-center rounded-full bg-black/50 text-white">
                                                <Icon name="play" class_name="h-6 w-6"/>
                                            </div>
                                        </div>
                                    </button>
                                }.into_any()
                            } else if let Some(e) = err.get() {
                                view! {
                                    <div class="flex flex-col items-center gap-1 text-destructive">
                                        <Icon name="film" class_name="h-8 w-8"/>
                                        <span class="text-[10px]">{e}</span>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <video
                                        node_ref=video_ref
                                        controls=true
                                        autoplay=true
                                        playsinline=true
                                        class="h-full w-full object-contain bg-black"
                                    />
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
            }.into_any()
        }
        "audio" => {
            // A sent audio file (music) — played inline with a player styled to
            // match the app (not the native browser <audio controls>). Click-to-
            // load on first play, then a custom play/pause + seek bar.
            let file_name = msg.file_name.as_deref().unwrap_or("Audio").to_string();
            let attachment_id = msg.attachment_id.clone();
            let decryption_key = msg.decryption_key.clone();
            let mime = msg.mime_type.clone().unwrap_or_else(|| "audio/mpeg".into());
            let blob_url: RwSignal<Option<String>> = RwSignal::new(None);
            let err: RwSignal<Option<String>> = RwSignal::new(None);
            let started: RwSignal<bool> = RwSignal::new(false);
            let is_playing = RwSignal::new(false);
            let current_ms = RwSignal::new(0u32);
            let duration_ms = RwSignal::new(0u32);
            let audio_ref: NodeRef<leptos::html::Audio> = NodeRef::new();

            // Play/pause; loads (and arms autoplay) on the first tap.
            let toggle = move |_| {
                if blob_url.get_untracked().is_none() {
                    if !started.get_untracked() {
                        started.set(true);
                        load_media_blob(
                            attachment_id.clone(),
                            decryption_key.clone(),
                            mime.clone(),
                            blob_url,
                            err,
                            l,
                        );
                    }
                    is_playing.set(true);
                    return;
                }
                if let Some(el) = audio_ref.get_untracked() {
                    let audio: &web_sys::HtmlAudioElement = el.unchecked_ref();
                    if is_playing.get_untracked() {
                        let _ = audio.pause();
                        is_playing.set(false);
                    } else {
                        let _ = audio.play();
                        is_playing.set(true);
                    }
                }
            };
            // Start playback once the blob URL lands (the user already tapped).
            Effect::new(move |_| {
                if blob_url.get().is_some() && is_playing.get_untracked() {
                    if let Some(el) = audio_ref.get_untracked() {
                        let audio: &web_sys::HtmlAudioElement = el.unchecked_ref();
                        let _ = audio.play();
                    }
                }
            });
            let on_time = move |ev: leptos::ev::Event| {
                if let Some(t) = ev.target() {
                    let audio: web_sys::HtmlAudioElement = t.unchecked_into();
                    current_ms.set((audio.current_time() * 1000.0) as u32);
                }
            };
            let on_loaded = move |ev: leptos::ev::Event| {
                if let Some(t) = ev.target() {
                    let audio: web_sys::HtmlAudioElement = t.unchecked_into();
                    let d = audio.duration();
                    if d.is_finite() && d > 0.0 {
                        duration_ms.set((d * 1000.0) as u32);
                    }
                }
            };
            let on_ended = move |_: leptos::ev::Event| {
                is_playing.set(false);
                current_ms.set(0);
            };
            // Click anywhere on the bar to seek.
            let seek = move |ev: leptos::ev::MouseEvent| {
                let Some(el) = audio_ref.get_untracked() else { return };
                let audio: &web_sys::HtmlAudioElement = el.unchecked_ref();
                let dur = audio.duration();
                if !dur.is_finite() || dur <= 0.0 {
                    return;
                }
                if let Some(target) = ev.current_target() {
                    let bar: web_sys::Element = target.unchecked_into();
                    let rect = bar.get_bounding_client_rect();
                    if rect.width() > 0.0 {
                        let frac = ((f64::from(ev.client_x()) - rect.left()) / rect.width())
                            .clamp(0.0, 1.0);
                        audio.set_current_time(frac * dur);
                        current_ms.set((frac * dur * 1000.0) as u32);
                    }
                }
            };

            let fmt = |ms: u32| {
                let s = ms / 1000;
                format!("{}:{:02}", s / 60, s % 60)
            };

            view! {
                <div class="flex w-64 max-w-full flex-col gap-2 rounded-lg bg-muted/40 p-2.5">
                    <div class="flex items-center gap-2">
                        <Icon name="music" class_name="h-4 w-4 shrink-0 text-muted-foreground"/>
                        <span class="truncate text-sm text-foreground">{file_name.clone()}</span>
                    </div>
                    <div class="flex items-center gap-2.5">
                        <button
                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-60"
                            on:click=toggle
                        >
                            {move || {
                                if started.get() && blob_url.get().is_none() && err.get().is_none() {
                                    view! { <span class="block h-4 w-4 rounded-full border-2 border-current border-t-transparent animate-spin"/> }.into_any()
                                } else if is_playing.get() {
                                    view! { <Icon name="pause" class_name="h-4 w-4"/> }.into_any()
                                } else {
                                    view! { <Icon name="play" class_name="h-4 w-4"/> }.into_any()
                                }
                            }}
                        </button>
                        <div class="flex-1 min-w-0">
                            <div
                                class="group h-1.5 w-full cursor-pointer rounded-full bg-foreground/15"
                                on:click=seek
                            >
                                <div
                                    class="h-full rounded-full bg-primary"
                                    style=move || {
                                        let d = duration_ms.get().max(1);
                                        let pct = (current_ms.get() as f64 / d as f64 * 100.0).clamp(0.0, 100.0);
                                        format!("width:{pct}%")
                                    }
                                />
                            </div>
                            <div class="mt-1 flex justify-between text-[10px] tabular-nums text-muted-foreground">
                                <span>{move || fmt(current_ms.get())}</span>
                                <span>{move || fmt(duration_ms.get())}</span>
                            </div>
                        </div>
                    </div>

                    // Hidden native element — drives playback; UI above is ours.
                    {move || blob_url.get().map(|url| view! {
                        <audio
                            node_ref=audio_ref
                            src=url
                            preload="auto"
                            on:timeupdate=on_time
                            on:loadedmetadata=on_loaded
                            on:ended=on_ended
                        />
                    })}

                    {move || err.get().map(|e| view! {
                        <span class="text-[10px] text-destructive">{e}</span>
                    })}
                </div>
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
                        let plain = match messenger_core::attachment_crypto::decrypt_attachment_auto(&key, &ct) {
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
    };
    // Append the caption under the media as part of the same bubble.
    if is_media && !caption.trim().is_empty() {
        view! {
            <div>
                {media}
                <p class="whitespace-pre-wrap break-words mt-1.5">{caption}</p>
            </div>
        }.into_any()
    } else {
        media
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
    on_forward: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
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

    let forward_cb = on_forward.map(|f| {
        Box::new(move || f()) as Box<dyn Fn() + Send + Sync + 'static>
    });
    views.push(view! {
        <ContextMenuItem on_click=forward_cb.unwrap_or_else(|| Box::new(|| {}))>
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

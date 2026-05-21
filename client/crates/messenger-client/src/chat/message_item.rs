//! Individual message bubble — renders text, voice, image, video, file, system, and
//! deleted messages with reactions, reply quoting, thread indicator, and context menu.
use leptos::prelude::*;
use leptos::ev::PointerEvent;
use crate::i18n::{Language, t, format_time};
use crate::mock::Message;
use crate::icons::Icon;
use super::voice_message::VoiceMessage;
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
    #[prop(optional)] on_media_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_avatar_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
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

    // Long-press handler for mobile
    let long_press_trigger = move |_e: PointerEvent| {
        menu_open.set(true);
    };

    let show_avatar = !is_own && is_first_in_group;
    let show_sender_name = !is_own && is_first_in_group && msg.sender_name != "System";

    // Clone things we need inside closures
    let msg_clone_for_content = msg.clone();
    let msg_clone_for_context = msg.clone();
    let lang_clone = lang;

    // Build the context menu content outside view! to avoid proc macro issues
    let menu_content = {
        let msg_ctx = msg.clone();
        Box::new(move || {
            let views = message_context_menu_items(&msg_ctx, lang.get());

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
            // Desktop context menu wrapper
            <ContextMenu
                menu=menu_content
            >
                <ContextMenuTrigger>
                    <div class="flex items-end gap-2 max-w-full" on:pointerdown=long_press_trigger>
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

                        <div class=format!("flex flex-col {}", if is_own { "items-end" } else { "items-start" })>
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

                            // Message bubble
                            <div class=format!("px-3 py-2 text-sm shadow-sm break-words max-w-[75%] {bubble_class}")>
                                // Message content based on type
                                {render_content(msg.clone(), on_media_click_arc.clone())}

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
                                            view! {
                                                <button
                                                    class=format!("inline-flex items-center gap-0.5 rounded-full px-1.5 py-0.5 text-xs shadow-sm hover:bg-accent transition-colors {bg}")
                                                    on:click=move |_| {}
                                                >
                                                    <span>{emoji}</span>
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
                                        <span>{count} {t(lang.get(), "message.replies")}</span>
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

        // Mobile action sheet (bottom sheet)
        <Sheet
            is_open=Signal::derive(move || menu_open.get())
            on_close=on_close_sheet_cb
            side="bottom".to_string()
        >
            <SheetHeader>
                <SheetTitle>{t(lang.get(), "message.reply")}</SheetTitle>
            </SheetHeader>
            <div class="space-y-1">
                {message_context_menu_items(&msg_clone_for_context, lang_clone.get())
                    .into_iter()
                    .map(|item| item)
                    .collect::<Vec<_>>()
                }
            </div>
        </Sheet>
    }.into_any()
}

/// Render message content based on type.
fn render_content(msg: Message, on_media_click: std::sync::Arc<Option<Box<dyn Fn() + Send + Sync + 'static>>>) -> AnyView {
    match msg.msg_type.as_str() {
        "voice" => {
            view! {
                <VoiceMessage
                    duration=msg.duration.unwrap_or(0)
                    waveform=msg.waveform.clone()
                    transcription=msg.transcription.clone().unwrap_or_default()
                    is_own=msg.is_own
                />
            }.into_any()
        }
        "image" => {
            let mc = on_media_click.clone();
            view! {
                <button
                    class="block overflow-hidden rounded-lg -mx-1 -mt-1 mb-1"
                    on:click=move |_| {
                        if let Some(f) = mc.as_ref() { f(); }
                    }
                >
                    <div class="aspect-video max-h-64 w-full bg-muted flex items-center justify-center">
                        {if let Some(ref thumb) = msg.thumbnail_url {
                            view! {
                                <img src=thumb alt="Image" class="h-full w-full object-cover"/>
                            }.into_any()
                        } else {
                            view! {
                                <div class="flex flex-col items-center gap-2 text-muted-foreground">
                                    <Icon name="image" class_name="h-8 w-8"/>
                                    <span class="text-xs">"Image"</span>
                                </div>
                            }.into_any()
                        }}
                    </div>
                </button>
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
                                    <span class="text-xs">"Video"</span>
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
            view! {
                <div class="flex items-center gap-3 rounded-lg -mx-1 -mt-1 mb-1 p-2 bg-muted/30">
                    <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                        <Icon name="file" class_name="h-5 w-5"/>
                    </div>
                    <div class="min-w-0 flex-1">
                        <p class="text-sm font-medium truncate">{file_name}</p>
                        {msg.file_size.map(|size| {
                            view! {
                                <p class="text-xs text-muted-foreground">{format_file_size(size)}</p>
                            }.into_any()
                        }).unwrap_or_else(|| view! {}.into_any())}
                    </div>
                    <Tooltip text="Download".to_string()>
                        <button class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md hover:bg-accent">
                            <Icon name="download" class_name="h-4 w-4"/>
                        </button>
                    </Tooltip>
                </div>
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
) -> Vec<AnyView> {
    let mut views: Vec<AnyView> = Vec::new();
    let noop_cb = || Box::new(|| {}) as Box<dyn Fn() + Send + Sync + 'static>;

    // Reply
    views.push(view! {
        <ContextMenuItem on_click=noop_cb()>
            <Icon name="reply" class_name="mr-2 h-4 w-4"/>
            {t(lang, "message.reply")}
        </ContextMenuItem>
    }.into_any());

    // Reply in thread (always show without callback for now)
    views.push(view! {
        <ContextMenuItem>
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
        views.push(view! {
            <ContextMenuItem on_click=noop_cb()>
                <Icon name="edit" class_name="mr-2 h-4 w-4"/>
                {t(lang, "message.edit")}
            </ContextMenuItem>
        }.into_any());
    }

    views.push(view! {
        <ContextMenuItem on_click=noop_cb()>
            <Icon name="forward" class_name="mr-2 h-4 w-4"/>
            {t(lang, "message.forward")}
        </ContextMenuItem>
    }.into_any());

    views.push(view! { <ContextMenuSeparator/> }.into_any());

    // Delete
    views.push(view! {
        <ContextMenuItem
            class="text-destructive"
            on_click=noop_cb()
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

use leptos::prelude::*;
use crate::components::avatar::{Avatar, get_initials};
use crate::components::button::{Button, ButtonVariant, ButtonSize};
use crate::components::badge::Badge;
use crate::components::input::Input;
use crate::components::scroll_area::ScrollArea;
use crate::components::dropdown_menu::{
    DropdownMenu, DropdownMenuTrigger, DropdownMenuContent,
    DropdownMenuItem, DropdownMenuSeparator,
};
use crate::components::context_menu::{
    ContextMenu, ContextMenuTrigger, ContextMenuContent,
    ContextMenuItem, ContextMenuSeparator,
};
use crate::i18n::{Language, t, format_time, format_date};
use crate::mock::{mock_chats, mock_server_info, Chat};
use crate::icons::Icon;

/// Format last message time display.
fn last_message_time(chat: &Chat, lang: Language) -> String {
    if let Some(ref msg) = chat.last_message {
        let now = crate::mock::now_ms();
        let diff = now - msg.timestamp;
        if diff < 24.0 * 60.0 * 60.0 * 1000.0 {
            format_time(msg.timestamp, lang)
        } else {
            format_date(msg.timestamp, lang)
        }
    } else {
        String::new()
    }
}

/// Last message preview text.
fn last_message_preview(chat: &Chat) -> String {
    if let Some(ref msg) = chat.last_message {
        match msg.msg_type.as_str() {
            "voice" => "🎤 Voice message".into(),
            "image" => "📷 Photo".into(),
            "video" => "🎬 Video".into(),
            "file" => format!("📎 {}", msg.file_name.as_deref().unwrap_or("File")),
            "system" => format!("ℹ️ {}", msg.content),
            _ => msg.content.clone(),
        }
    } else {
        String::new()
    }
}

/// Single chat item in the list.
#[must_use]
#[component]
fn ChatItem(
    chat: Chat,
    lang: RwSignal<Language>,
    search_query: RwSignal<String>,
    on_pin_toggle: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
    on_mute_toggle: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
    on_mark_read: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
    on_archive: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
    on_delete: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
) -> impl IntoView {
    let chat_id = chat.id.clone();
    let chat_id_for_pin = chat.id.clone();
    let chat_id_for_mute = chat.id.clone();
    let chat_id_for_read = chat.id.clone();
    let chat_id_for_archive = chat.id.clone();
    let chat_id_for_delete = chat.id.clone();
    let chat_id_for_clear = chat.id.clone();

    let chat_type = chat.chat_type.clone();
    let is_pinned = chat.is_pinned;
    let is_muted = chat.is_muted;
    let chat_name = chat.name.clone();
    let chat_avatar_url = chat.avatar_url.clone();
    let chat_has_security_changes = chat.has_security_changes;
    let chat_unread_count = chat.unread_count;
    let chat_for_closures = chat.clone();
    let chat_type_for_view = chat_type.clone();
    let chat_name2 = chat_name.clone();
    let chat_for_preview = chat_for_closures.clone();

    let menu: Box<dyn Fn() -> AnyView + Send + Sync + 'static> = Box::new({
        let cid = chat.id.clone();
        move || {
            let cid_inner = cid.clone();
            let pin_toggle = on_pin_toggle.clone();
            let mute_toggle = on_mute_toggle.clone();
            let mark_read = on_mark_read.clone();
            let archive = on_archive.clone();
            let delete = on_delete.clone();
            let chat_type_text = if chat_type == "group" {
                t(lang.get(), "sidebar.chatList.leaveGroup")
            } else {
                t(lang.get(), "sidebar.chatList.deleteChat")
            };
            view! {
                <ContextMenuContent class="w-56">
                    <ContextMenuItem
                        on_click=Box::new({
                            let id = cid_inner.clone();
                            let pin_toggle = pin_toggle.clone();
                            move || { if let Some(f) = pin_toggle.as_ref() { f(id.clone()); } }
                        })
                    >
                        <Icon name={if is_pinned { "chevron-right" } else { "pin" }} class_name="mr-2 h-4 w-4" />
                        {if is_pinned { t(lang.get(), "sidebar.chatList.unpin") } else { t(lang.get(), "sidebar.chatList.pin") }}
                    </ContextMenuItem>
                    <ContextMenuItem
                        on_click=Box::new({
                            let id = cid_inner.clone();
                            let mute_toggle = mute_toggle.clone();
                            move || { if let Some(f) = mute_toggle.as_ref() { f(id.clone()); } }
                        })
                    >
                        <Icon name={if is_muted { "bell" } else { "bell-off" }} class_name="mr-2 h-4 w-4" />
                        {if is_muted { t(lang.get(), "sidebar.chatList.unmute") } else { t(lang.get(), "sidebar.chatList.mute") }}
                    </ContextMenuItem>
                    <ContextMenuItem
                        on_click=Box::new({
                            let id = cid_inner.clone();
                            let mark_read = mark_read.clone();
                            move || { if let Some(f) = mark_read.as_ref() { f(id.clone()); } }
                        })
                    >
                        <Icon name="check-check" class_name="mr-2 h-4 w-4" />
                        {t(lang.get(), "sidebar.chatList.markRead")}
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem
                        on_click=Box::new({
                            let id = cid_inner.clone();
                            let archive = archive.clone();
                            move || { if let Some(f) = archive.as_ref() { f(id.clone()); } }
                        })
                    >
                        <Icon name="archive" class_name="mr-2 h-4 w-4" />
                        {t(lang.get(), "sidebar.chatList.archive")}
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem
                        class="text-destructive"
                        on_click=Box::new({
                            let id = cid_inner.clone();
                            let delete = delete.clone();
                            move || { if let Some(f) = delete.as_ref() { f(id.clone()); } }
                        })
                    >
                        <Icon name="log-out" class_name="mr-2 h-4 w-4" />
                        {chat_type_text}
                    </ContextMenuItem>
                </ContextMenuContent>
            }.into_any()
        }
    });

    view! {
        <ContextMenu
            menu=menu
        >
            <ContextMenuTrigger>
                <div class="flex items-center gap-3 px-4 py-3 hover:bg-accent/50 cursor-pointer transition-colors border-b border-border/50 last:border-b-0">
                    <Avatar
                        src=chat_avatar_url.clone()
                        alt=chat_name.clone()
                        class="h-12 w-12 shrink-0"
                    >
                        <span class="text-sm font-semibold text-foreground">
                            {get_initials(&chat_name2)}
                        </span>
                    </Avatar>
                    <div class="min-w-0 flex-1">
                        <div class="flex items-center justify-between">
                            <div class="flex items-center gap-1.5 min-w-0">
                                {move || if is_pinned {
                                    view! {
                                        <Icon name="pin" class_name="h-3 w-3 shrink-0 text-muted-foreground rotate-45" />
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }}
                                <span class="text-sm font-medium text-foreground truncate">{chat_name.clone()}</span>
                                {move || if chat_type_for_view == "group" {
                                    view! {
                                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-muted-foreground">
                                            <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
                                            <circle cx="9" cy="7" r="4" />
                                            <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
                                            <path d="M16 3.13a4 4 0 0 1 0 7.75" />
                                        </svg>
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }}
                                {move || if chat_has_security_changes {
                                    view! {
                                        <Icon name="shield-alert" class_name="h-3 w-3 shrink-0 text-destructive" />
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }}
                                {move || if is_muted {
                                    view! {
                                        <Icon name="bell-off" class_name="h-3 w-3 shrink-0 text-muted-foreground" />
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }}
                            </div>
                            <span class="text-xs text-muted-foreground shrink-0">
                                {move || {
                                    last_message_time(&chat_for_closures, lang.get())
                                }}
                            </span>
                        </div>
                        <div class="flex items-center justify-between mt-0.5">
                            <p class="text-xs text-muted-foreground truncate">
                                {move || {
                                    last_message_preview(&chat_for_preview)
                                }}
                            </p>
                            {move || if chat_unread_count > 0 {
                                view! {
                                    <Badge variant="default".to_string() class="ml-2 h-5 min-w-[20px] px-1.5 text-[10px]">
                                        {if chat_unread_count > 99 {
                                            "99+".to_string()
                                        } else {
                                            chat_unread_count.to_string()
                                        }}
                                    </Badge>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                        </div>
                    </div>
                </div>
            </ContextMenuTrigger>
        </ContextMenu>
    }
}

/// ChatList sidebar component.
#[must_use]
#[component]
pub fn ChatList(
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_chat_select: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
) -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let (server_name, _address, _version) = mock_server_info();
    let all_chats = RwSignal::new(mock_chats());
    let search_query = RwSignal::new(String::new());
    let show_menu = RwSignal::new(false);

    // Filter and sort chats
    let filtered_chats = move || {
        let mut chats = all_chats.get();
        let query = search_query.get().to_lowercase();

        // Filter by search
        if !query.is_empty() {
            chats.retain(|c| c.name.to_lowercase().contains(&query));
        }

        // Sort: pinned first, then by last message time
        chats.sort_by(|a, b| {
            let a_pinned = a.is_pinned;
            let b_pinned = b.is_pinned;
            match (a_pinned, b_pinned) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_time = a.last_message.as_ref().map(|m| m.timestamp).unwrap_or(0.0);
                    let b_time = b.last_message.as_ref().map(|m| m.timestamp).unwrap_or(0.0);
                    b_time.partial_cmp(&a_time).unwrap_or(std::cmp::Ordering::Equal)
                }
            }
        });

        chats
    };

    // Context menu handlers
    let on_pin_toggle = Some(std::sync::Arc::new(move |id: String| {
        all_chats.update(|chats| {
            if let Some(c) = chats.iter_mut().find(|c| c.id == id) {
                c.is_pinned = !c.is_pinned;
            }
        });
    }) as std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>);

    let on_mute_toggle = Some(std::sync::Arc::new(move |id: String| {
        all_chats.update(|chats| {
            if let Some(c) = chats.iter_mut().find(|c| c.id == id) {
                c.is_muted = !c.is_muted;
            }
        });
    }) as std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>);

    let on_mark_read = Some(std::sync::Arc::new(move |id: String| {
        all_chats.update(|chats| {
            if let Some(c) = chats.iter_mut().find(|c| c.id == id) {
                c.unread_count = 0;
            }
        });
    }) as std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>);

    let on_archive = Some(std::sync::Arc::new(move |id: String| {
        all_chats.update(|chats| {
            if let Some(c) = chats.iter_mut().find(|c| c.id == id) {
                c.is_archived = true;
            }
        });
    }) as std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>);

    let on_delete = Some(std::sync::Arc::new(move |id: String| {
        all_chats.update(|chats| {
            chats.retain(|c| c.id != id);
        });
    }) as std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>);

    view! {
        <div class=format!("flex h-full flex-col bg-background border-r border-border {}", class)>
            // Header
            <div class="relative flex items-center justify-between px-4 py-3 border-b border-border">
                <h2 class="text-base font-semibold text-foreground truncate">{server_name}</h2>
                <DropdownMenu>
                    <DropdownMenuTrigger>
                        <Button
                            variant=Signal::derive(move || ButtonVariant::Ghost)
                            size=Signal::derive(move || ButtonSize::Icon)
                        >
                            <Icon name="more-horizontal" class_name="h-5 w-5" />
                        </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align=String::from("end") class="w-56">
                        <DropdownMenuItem>
                            <Icon name="message-square" class_name="mr-2 h-4 w-4" />
                            {t(lang.get(), "sidebar.chatList.newChat")}
                        </DropdownMenuItem>
                        <DropdownMenuItem>
                            <Icon name="settings" class_name="mr-2 h-4 w-4" />
                            {t(lang.get(), "sidebar.chatList.settings")}
                        </DropdownMenuItem>
                        <DropdownMenuItem>
                            <Icon name="archive" class_name="mr-2 h-4 w-4" />
                            {t(lang.get(), "sidebar.chatList.archive")}
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem>
                            <Icon name="log-out" class_name="mr-2 h-4 w-4" />
                            {t(lang.get(), "sidebar.chatList.logout")}
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </div>

            // Search
            <div class="px-4 py-2">
                <div class="relative">
                    <div class="absolute inset-y-0 left-3 flex items-center pointer-events-none">
                        <Icon name="search" class_name="h-4 w-4 text-muted-foreground" />
                    </div>
                    <Input
                        value=search_query.get()
                        on_change=Box::new(move |v| search_query.set(v))
                        placeholder=t(lang.get(), "sidebar.chatList.search")
                        class="pl-9 h-9 text-sm"
                    />
                </div>
            </div>

            // Chat list
            <ScrollArea class="flex-1">
                {move || {
                    let on_chat_select = on_chat_select.clone();
                    let chats = filtered_chats();
                    if chats.is_empty() {
                        view! {
                            <div class="flex flex-col items-center justify-center py-12 text-center px-4">
                                <p class="text-sm text-muted-foreground">
                                    {t(lang.get(), "sidebar.chatList.noChats")}
                                </p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div>
                                {chats.into_iter().map(|chat| {
                                    let chat_for_id = chat.id.clone();
                                    let on_chat_select = on_chat_select.clone();
                                    view! {
                                        <div on:click={
                                            let id = chat_for_id;
                                            move |_| {
                                                if let Some(f) = on_chat_select.as_ref() {
                                                    f(id.clone());
                                                }
                                            }
                                        }>
                                            <ChatItem
                                                chat=chat
                                                lang=lang
                                                search_query=search_query
                                                on_pin_toggle={on_pin_toggle.clone()}
                                                on_mute_toggle={on_mute_toggle.clone()}
                                                on_mark_read={on_mark_read.clone()}
                                                on_archive={on_archive.clone()}
                                                on_delete={on_delete.clone()}
                                            />
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </ScrollArea>

            // FAB for mobile
            <div class="absolute bottom-6 right-4 md:hidden">
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    size=Signal::derive(move || ButtonSize::Icon)
                    class="h-14 w-14 rounded-full shadow-lg"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="12" y1="5" x2="12" y2="19" />
                        <line x1="5" y1="12" x2="19" y2="12" />
                    </svg>
                </Button>
            </div>
        </div>
    }
}

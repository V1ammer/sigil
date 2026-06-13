//! Chat header component with back navigation, profile info, and dropdown menu.
use leptos::prelude::*;
use crate::i18n::{Language, t};
use crate::mock::Chat;
use crate::icons::Icon;
use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator};
use crate::components::avatar::Avatar;

#[must_use]
#[component]
pub fn ChatHeader(
    #[prop(optional, into)] lang: Signal<Language>,
    #[prop(optional)] chat: Option<Chat>,
    /// Reactive peer avatar (direct chats). Resolved independently of the chat
    /// snapshot so it always reflects the latest `avatar_by_id`, and survives
    /// re-renders of the surrounding chat panel.
    #[prop(optional, into)] avatar: Signal<Option<String>>,
    #[prop(optional)] on_back: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_pin_toggle: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_mute_toggle: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_mark_read: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_archive: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_leave_group: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_delete_chat: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let is_group = chat.as_ref().map(|c| c.chat_type == "group").unwrap_or(false);
    let chat_name = chat.as_ref().map(|c| c.name.clone()).unwrap_or_default();
    let chat_name2 = chat_name.clone();
    let chat_avatar = chat.as_ref().and_then(|c| c.avatar_url.clone());
    let is_muted = chat.as_ref().map(|c| c.is_muted).unwrap_or(false);
    let is_pinned = chat.as_ref().map(|c| c.is_pinned).unwrap_or(false);

    let back_cb = on_back.unwrap_or_else(|| Box::new(|| {}));
    let pin_cb = on_pin_toggle.unwrap_or_else(|| Box::new(|| {}));
    let mute_cb = on_mute_toggle.unwrap_or_else(|| Box::new(|| {}));
    let mark_read_cb = on_mark_read.unwrap_or_else(|| Box::new(|| {}));
    let archive_cb = on_archive.unwrap_or_else(|| Box::new(|| {}));
    let leave_group_cb = on_leave_group.unwrap_or_else(|| Box::new(|| {}));
    let delete_chat_cb = on_delete_chat.unwrap_or_else(|| Box::new(|| {}));

    let on_pin_toggle_cb = {
        let cb = pin_cb;
        Box::new(move || cb()) as Box<dyn Fn() + Send + Sync + 'static>
    };

    let on_mute_toggle_cb = {
        let cb = mute_cb;
        Box::new(move || cb()) as Box<dyn Fn() + Send + Sync + 'static>
    };

    let on_mark_read_cb = {
        let cb = mark_read_cb;
        Box::new(move || cb()) as Box<dyn Fn() + Send + Sync + 'static>
    };

    let on_archive_cb = {
        let cb = archive_cb;
        Box::new(move || cb()) as Box<dyn Fn() + Send + Sync + 'static>
    };

    let on_leave_group_cb = {
        let cb = leave_group_cb;
        Box::new(move || cb()) as Box<dyn Fn() + Send + Sync + 'static>
    };

    let on_delete_chat_cb = {
        let cb = delete_chat_cb;
        Box::new(move || cb()) as Box<dyn Fn() + Send + Sync + 'static>
    };

    view! {
        <header class="flex items-center gap-4 border-b border-border px-4 py-3 bg-background">
            // Back button (mobile)
            <button
                class="md:hidden h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                on:click=move |_| back_cb()
            >
                <Icon name="chevron-left" class_name="h-5 w-5"/>
            </button>

            // Profile info
            <div class="flex items-center gap-3 min-w-0 flex-1">
                {
                    // Reactive: prefer the live avatar signal, fall back to the
                    // chat snapshot. Re-renders when the avatar lands or changes,
                    // independent of the parent panel's reactivity.
                    let alt = chat_name.clone();
                    let initials = crate::components::avatar::get_initials(&chat_name2);
                    move || {
                        let src = avatar.get().or_else(|| chat_avatar.clone());
                        let initials = initials.clone();
                        view! {
                            <Avatar
                                src=src
                                alt=alt.clone()
                                class="h-12 w-12 shrink-0".to_string()
                            >
                                <span class="text-sm font-semibold text-foreground">
                                    {initials}
                                </span>
                            </Avatar>
                        }
                    }
                }
                <div class="min-w-0">
                    <h2 class="text-sm font-semibold text-foreground truncate">{chat_name}</h2>
                    <p class="text-xs text-muted-foreground truncate">
                        {if is_group {
                            t(lang.get(), "chat.members")
                        } else {
                            t(lang.get(), "chat.direct")
                        }}
                    </p>
                </div>
            </div>

            // Actions
            <div class="flex items-center gap-1">
                // Pin indicator
                {if is_pinned {
                    view! {
                        <Icon name="pin" class_name="h-4 w-4 text-muted-foreground"/>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }}

                // Dropdown menu
                <DropdownMenu>
                    <DropdownMenuTrigger>
                        <button class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent">
                            <Icon name="more-vertical" class_name="h-5 w-5"/>
                        </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align=String::from("end")>
                        <DropdownMenuItem on_click=on_pin_toggle_cb>
                            <Icon name="pin" class_name="mr-2 h-4 w-4"/>
                            {if is_pinned {
                                t(lang.get(), "chat.unpinChat")
                            } else {
                                t(lang.get(), "chat.pinChat")
                            }}
                        </DropdownMenuItem>

                        <DropdownMenuItem on_click=on_mute_toggle_cb>
                            <Icon name={if is_muted { "bell-off" } else { "bell" }} class_name="mr-2 h-4 w-4"/>
                            {if is_muted {
                                t(lang.get(), "chat.unmute")
                            } else {
                                t(lang.get(), "chat.mute")
                            }}
                        </DropdownMenuItem>

                        <DropdownMenuItem on_click=on_mark_read_cb>
                            <Icon name="check-check" class_name="mr-2 h-4 w-4"/>
                            {t(lang.get(), "chat.markRead")}
                        </DropdownMenuItem>

                        <DropdownMenuSeparator/>

                        <DropdownMenuItem on_click=on_archive_cb>
                            <Icon name="archive" class_name="mr-2 h-4 w-4"/>
                            {t(lang.get(), "chat.archiveChat")}
                        </DropdownMenuItem>

                        {if is_group {
                            view! {
                                <DropdownMenuItem
                                    class="text-destructive".to_string()
                                    on_click=on_leave_group_cb
                                >
                                    <Icon name="log-out" class_name="mr-2 h-4 w-4"/>
                                    {t(lang.get(), "chat.leave")}
                                </DropdownMenuItem>
                            }.into_any()
                        } else {
                            view! {
                                <DropdownMenuItem
                                    class="text-destructive".to_string()
                                    on_click=on_delete_chat_cb
                                >
                                    <Icon name="trash" class_name="mr-2 h-4 w-4"/>
                                    {t(lang.get(), "chat.delete")}
                                </DropdownMenuItem>
                            }.into_any()
                        }}
                    </DropdownMenuContent>
                </DropdownMenu>
            </div>
        </header>
    }
}

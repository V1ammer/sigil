//! Profile sheet — slide-over panel showing user/chat profile info.
use leptos::prelude::*;
use crate::i18n::{Language, t};
use crate::mock::Chat;
use crate::icons::Icon;
use crate::components::sheet::{Sheet, SheetHeader, SheetTitle};
use crate::components::avatar::get_initials;
use crate::components::separator::Separator;
use crate::components::badge::Badge;
use crate::components::button::{Button, ButtonVariant};

#[must_use]
#[component]
pub fn ProfileSheet(
    #[prop(optional, into)] lang: Signal<Language>,
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] chat: Option<Chat>,
    #[prop(optional)] on_start_call: Option<Box<dyn Fn() + 'static>>,
    #[prop(optional)] on_block: Option<std::sync::Arc<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let chat_data = chat;
    let is_group = chat_data.as_ref().map(|c| c.chat_type == "group").unwrap_or(false);
    let participants_count = chat_data.as_ref()
        .map(|c| c.participant_ids.len() + 1)
        .unwrap_or(0);

    let close_cb = on_close.unwrap_or_else(|| Box::new(|| {}));
    let on_block = on_block.clone();

    view! {
        <Sheet
            is_open=is_open
            on_close=close_cb
            side="right".to_string()
        >
            <SheetHeader>
                <SheetTitle>{t(lang.get(), "profile.title")}</SheetTitle>
            </SheetHeader>

            {if let Some(ref chat) = chat_data {
                view! {
                    <div class="flex flex-col items-center py-6">
                        // Avatar
                        <div class="flex h-20 w-20 items-center justify-center rounded-full bg-muted text-2xl font-medium text-foreground shadow-sm mb-3">
                            {get_initials(&chat.name)}
                        </div>
                        <h3 class="text-lg font-semibold text-foreground">{chat.name.clone()}</h3>

                        {if is_group {
                            view! {
                                <Badge variant="secondary" class="mt-1">
                                    <Icon name="users" class_name="h-3 w-3 mr-1"/>
                                    {participants_count} {t(lang.get(), "profile.participants")}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                    </div>

                    <Separator class="my-2"/>

                    // Actions
                    <div class="space-y-1 px-2">
                        {if is_group {
                            view! {
                                <button class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm hover:bg-accent transition-colors">
                                    <Icon name="user-plus" class_name="h-5 w-5 text-muted-foreground"/>
                                    <span class="font-medium">{t(lang.get(), "profile.addMembers")}</span>
                                </button>
                            }.into_any()
                        } else {
                            view! {
                                <button class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm hover:bg-accent transition-colors">
                                    <Icon name="shield" class_name="h-5 w-5 text-muted-foreground"/>
                                    <div class="text-left">
                                        <p class="font-medium">{t(lang.get(), "security.safetyNumber")}</p>
                                        <p class="text-xs text-muted-foreground">{t(lang.get(), "security.compare")}</p>
                                    </div>
                                </button>
                            }.into_any()
                        }}

                        <button class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm hover:bg-accent transition-colors">
                            <Icon name="bell" class_name="h-5 w-5 text-muted-foreground"/>
                            <div class="text-left">
                                <p class="font-medium">{if chat.is_muted { t(lang.get(), "chat.unmute") } else { t(lang.get(), "chat.mute") }}</p>
                                <p class="text-xs text-muted-foreground">
                                    {if chat.is_muted { "Muted" } else { "Notifications on" }}
                                </p>
                            </div>
                        </button>

                        <button class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm hover:bg-accent transition-colors">
                            <Icon name="archive" class_name="h-5 w-5 text-muted-foreground"/>
                            <span class="font-medium">{t(lang.get(), "chat.archiveChat")}</span>
                        </button>
                    </div>

                    <Separator class="my-2"/>

                    // Security info for direct chats
                    {if is_group {
                        view! {}.into_any()
                    } else {
                        view! {
                            <div class="px-4 py-2">
                                <div class="flex items-center gap-2 text-sm text-muted-foreground">
                                    <Icon name="lock" class_name="h-4 w-4"/>
                                    <span>"E2E encrypted"</span>
                                </div>
                                {chat.device_count.map(|d| {
                                    view! {
                                        <p class="mt-1 text-xs text-muted-foreground">
                                            {d} {t(lang.get(), "security.devices")}
                                        </p>
                                    }
                                })}
                            </div>
                        }.into_any()
                    }}

                    // Danger zone
                    <div class="px-4 py-4 space-y-2">
                        {if is_group {
                            view! {
                                <Button
                                    variant=Signal::derive(move || ButtonVariant::Ghost)
                                    class="w-full text-destructive"
                                    on_click=Box::new(move |_| {})
                                >
                                    {t(lang.get(), "chat.leave")}
                                </Button>
                            }.into_any()
                        } else {
                            view! {
                                {{
                                    let block = on_block.clone();
                                    view! {
                                        <Button
                                            variant=Signal::derive(move || ButtonVariant::Ghost)
                                            class="w-full text-destructive"
                                            on_click=Box::new(move |_| {
                                                if let Some(ref f) = block { f(); }
                                            })
                                        >
                                            {t(lang.get(), "profile.block")}
                                        </Button>
                                    }.into_any()
                                }}
                            }.into_any()
                        }}
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="flex flex-col items-center justify-center py-12 text-muted-foreground">
                        <Icon name="user" class_name="h-12 w-12 mb-3 opacity-30"/>
                        <p class="text-sm">{t(lang.get(), "loading")}</p>
                    </div>
                }.into_any()
            }}
        </Sheet>
    }
}

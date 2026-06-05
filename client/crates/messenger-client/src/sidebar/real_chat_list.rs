//! Real chat list — loads data from `ChatsState` (server-backed), no mock data.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use crate::state::chats::{Chat, ChatsState};
use crate::state::session::build_api_client;
use crate::i18n::I18n;
use crate::t;

/// A lightweight sidebar that renders chats from `ChatsState`.
/// No context menus, no pin/mute — just selection and navigation.
#[must_use]
#[component]
pub fn RealChatList(
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_chat_select: Option<std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>>,
) -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let chats_state = use_context::<ChatsState>().expect("ChatsState must be provided");
    let chats = chats_state.filtered();
    let selected = chats_state.selected;
    let navigate = use_navigate();
    let go_settings = move |_| navigate("/settings", Default::default());

    // Dialog state
    let show_dialog = RwSignal::new(false);
    let username_input = RwSignal::new(String::new());
    let error_msg = RwSignal::new(String::new());
    let loading = RwSignal::new(false);

    let on_username_input = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        username_input.set(value);
        error_msg.set(String::new());
    };

    let on_new_chat_click = move |_| {
        show_dialog.set(true);
        error_msg.set(String::new());
        username_input.set(String::new());
    };

    let create_chat = move |_| {
        let username = username_input.get();
        if username.trim().is_empty() {
            error_msg.set(t!("chat.create_direct.empty_username").to_string());
            return;
        }
        loading.set(true);
        error_msg.set(String::new());
        let cs = chats_state.clone();
        spawn_local(async move {
            let api = build_api_client();
            match api {
                Some(api) => {
                    match cs.create_direct_chat(&api, username.trim()).await {
                        Ok(_group_id) => {
                            show_dialog.set(false);
                            username_input.set(String::new());
                        }
                        Err(e) => {
                            error_msg.set(e);
                        }
                    }
                }
                None => {
                    error_msg.set(t!("chat.create_direct.no_api").to_string());
                }
            }
            loading.set(false);
        });
    };

    let close_dialog = move |_| {
        show_dialog.set(false);
        username_input.set(String::new());
        error_msg.set(String::new());
        loading.set(false);
    };

    view! {
        <div class=format!("flex flex-col h-full overflow-hidden {}", class)>
            {/* Header with app name, new chat, and settings */}
            <div class="flex items-center justify-between px-4 py-3 border-b border-border">
                <h1 class="text-lg font-semibold text-foreground">"Messenger"</h1>
                <div class="flex items-center gap-1">
                    <button
                        class="inline-flex h-9 w-9 items-center justify-center rounded-md hover:bg-accent transition-colors"
                        on:click=on_new_chat_click
                        title={t!("sidebar.chatList.newChat")}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                        </svg>
                    </button>
                    <button
                        class="inline-flex h-9 w-9 items-center justify-center rounded-md hover:bg-accent transition-colors"
                        on:click=go_settings
                        title={t!("sidebar.chatList.settings")}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="12" r="3"/>
                            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                        </svg>
                    </button>
                </div>
            </div>
            <div class="flex-1 overflow-y-auto">
                {move || {
                    let list = chats();
                    if list.is_empty() {
                        view! {
                            <div class="flex flex-col items-center justify-center h-full p-8 text-center">
                                <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground mb-4">
                                    <path d="M17 6.1H3"/><path d="M21 12.1H3"/><path d="M15.1 18H3"/>
                                </svg>
                                <p class="text-muted-foreground">{t!("chat.list.empty")}</p>
                            </div>
                        }.into_any()
                    } else {
                        let on_select = on_chat_select.clone();
                        view! {
                            <For
                                each=move || list.clone()
                                key=|chat| chat.group_id
                                children=move |chat: Chat| {
                                    let cb = on_select.clone();
                                    let is_selected = move || selected.get() == Some(chat.group_id);
                                    let initials = get_initials(&chat.display_name);
                                    view! {
                                        <div
                                            class=move || {
                                                let base = "flex items-center gap-3 px-4 py-3 cursor-pointer transition-colors hover:bg-accent/50";
                                                if is_selected() {
                                                    format!("{} bg-accent", base)
                                                } else {
                                                    base.to_string()
                                                }
                                            }
                                            on:click=move |_| {
                                                selected.set(Some(chat.group_id));
                                                if let Some(ref f) = cb {
                                                    f(chat.group_id.to_string());
                                                }
                                            }
                                        >
                                            <AvatarPlaceholder initials={initials} />
                                            <div class="min-w-0 flex-1">
                                                <div class="flex items-center justify-between gap-2">
                                                    <span class="text-sm font-medium text-foreground truncate">
                                                        {chat.display_name.clone()}
                                                    </span>
                                                    {chat.last_message_at.map(|ts| {
                                                        view! {
                                                            <span class="text-xs text-muted-foreground shrink-0">
                                                                {format_timestamp(ts)}
                                                            </span>
                                                        }
                                                    })}
                                                </div>
                                                <div class="flex items-center justify-between gap-2 mt-0.5">
                                                    <p class="text-xs text-muted-foreground truncate">
                                                        {chat.last_message_preview.clone().unwrap_or_default()}
                                                    </p>
                                                    {if chat.unread_count > 0 {
                                                        view! {
                                                            <span class="inline-flex items-center justify-center rounded-full bg-primary text-primary-foreground text-[10px] font-medium min-w-[18px] h-[18px] px-1 shrink-0">
                                                                {if chat.unread_count > 99 {
                                                                    "99+".to_string()
                                                                } else {
                                                                    chat.unread_count.to_string()
                                                                }}
                                                            </span>
                                                        }.into_any()
                                                    } else {
                                                        view! {}.into_any()
                                                    }}
                                                </div>
                                            </div>
                                        </div>
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </div>
        </div>

        // Dialog overlay — use Show component to conditionally render
        {move || {
            if !show_dialog.get() {
                return view! {}.into_any();
            }
            // Clone closures for inner use
            let cc = create_chat.clone();
            let cd = close_dialog.clone();
            let oui = on_username_input.clone();
            view! {
                // Backdrop
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
                    // Inner dialog
                    <div class="bg-card rounded-lg shadow-xl border border-border w-full max-w-sm mx-4 p-6">
                        <h2 class="text-lg font-semibold text-foreground mb-4">
                            {t!("chat.create_direct.title")}
                        </h2>
                        <input
                            type="text"
                            class="w-full px-3 py-2 rounded-md border border-input bg-background text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                            placeholder={t!("chat.create_direct.placeholder")}
                            prop:value=username_input
                            on:input=move |ev| oui(ev)
                            disabled=move || loading.get()
                        />
                        {move || {
                            let err = error_msg.get();
                            if err.is_empty() {
                                return view! {}.into_any();
                            }
                            view! {
                                <p class="mt-2 text-sm text-destructive">{err}</p>
                            }.into_any()
                        }}
                        <div class="flex items-center justify-end gap-3 mt-4">
                            <button
                                class="inline-flex h-9 items-center justify-center rounded-md px-4 text-sm font-medium border border-input bg-background text-foreground hover:bg-accent transition-colors disabled:opacity-50"
                                on:click=move |ev| cd(ev)
                                disabled=move || loading.get()
                            >
                                {t!("chat.create_direct.cancel")}
                            </button>
                            <button
                                class="inline-flex h-9 items-center justify-center rounded-md px-4 text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
                                on:click=move |ev| cc(ev)
                                disabled=move || loading.get()
                            >
                                {if loading.get() {
                                    t!("chat.create_direct.creating").to_string()
                                } else {
                                    t!("chat.create_direct.create").to_string()
                                }}
                            </button>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

fn get_initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// 48×48 avatar circle with initials fallback.
#[component]
fn AvatarPlaceholder(initials: String) -> impl IntoView {
    view! {
        <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-full bg-muted text-sm font-semibold text-muted-foreground">
            {initials}
        </div>
    }
}

fn format_timestamp(ts_ms: i64) -> String {
    if ts_ms <= 0 {
        return String::new();
    }
    let now = js_sys::Date::new_0().get_time() as i64;
    let diff_ms = now - ts_ms;
    if diff_ms < 0 {
        return String::new();
    }
    let diff_mins = diff_ms / 60_000;
    let diff_hours = diff_ms / 3_600_000;
    let diff_days = diff_ms / 86_400_000;

    if diff_mins < 1 {
        t!("time.now")
    } else if diff_mins < 60 {
        format!("{}m", diff_mins)
    } else if diff_hours < 24 {
        format!("{}h", diff_hours)
    } else if diff_days < 7 {
        format!("{}d", diff_days)
    } else {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts_ms as f64));
        let month = date.get_month() + 1;
        let day = date.get_date();
        format!("{:02}/{:02}", day, month)
    }
}

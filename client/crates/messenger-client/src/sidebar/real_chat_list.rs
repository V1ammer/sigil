//! Real chat list — loads data from `ChatsState` (server-backed), no mock data.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use crate::components::sheet::{Sheet, SheetHeader, SheetTitle};
use crate::icons::Icon;
use crate::state::chats::{Chat, ChatsState};
use crate::state::messages::MessageKind;
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
    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let chats_state = use_context::<ChatsState>().expect("ChatsState must be provided");
    let msg_svc = use_context::<crate::state::message_service::MessageService>();
    let users_state = use_context::<crate::state::users::UsersState>();
    // Captured at setup (not inside the async create flow, where use_context
    // would fail after an await).
    let own_user_id = use_context::<crate::state::session::Session>()
        .and_then(|s| s.current_user_id());
    let chats = chats_state.filtered();
    let selected = chats_state.selected;
    let navigate = use_navigate();
    let go_settings = move |_| navigate("/settings", Default::default());

    // Hold a separate clone of chats_state for the action sheet — the existing
    // create_chat closure moves the original.
    let chats_state_for_sheet = chats_state.clone();

    // Dialog state
    let show_dialog = RwSignal::new(false);
    let username_input = RwSignal::new(String::new());
    let error_msg = RwSignal::new(String::new());
    let loading = RwSignal::new(false);

    // Chat action sheet — opened by long-press on a row.
    let action_sheet_chat: RwSignal<Option<Chat>> = RwSignal::new(None);
    let action_sheet_open = Signal::derive(move || action_sheet_chat.get().is_some());
    let close_action_sheet = Box::new(move || action_sheet_chat.set(None))
        as Box<dyn Fn() + Send + Sync + 'static>;

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

    let create_chat = {
        let users_state = users_state.clone();
        move |_| {
        let username = username_input.get();
        if username.trim().is_empty() {
            error_msg.set(t!("chat.create_direct.empty_username").to_string());
            return;
        }
        loading.set(true);
        error_msg.set(String::new());
        let cs = chats_state.clone();
        let svc = msg_svc.clone();
        let users_state = users_state.clone();
        // Clone the i18n handle into the async block: `t!` resolves context
        // through the leptos owner, which is gone after the first await.
        let i18n = i18n.clone();
        spawn_local(async move {
            let api = build_api_client();
            match api {
                Some(api) => {
                    match cs.create_direct_chat(&api, username.trim()).await {
                        Ok(group_id) => {
                            show_dialog.set(false);
                            let typed_username = username.trim().to_string();
                            username_input.set(String::new());
                            // Remember the peer's plaintext username (server is
                            // blind to it) so the admin user list can label the
                            // row. Resolve the peer via group members.
                            if let Some(users) = users_state.clone() {
                                if let Ok(resp) = api.get_group_members(group_id).await {
                                    if let Some(peer) = resp
                                        .members
                                        .iter()
                                        .map(|m| m.user_id)
                                        .find(|uid| Some(*uid) != own_user_id)
                                    {
                                        users.remember_username(peer, &typed_username);
                                    }
                                }
                            }
                            // Open the (new or reopened) chat right away.
                            cs.selected.set(Some(group_id));
                            // Introduce ourselves: deliver our avatar to the
                            // new chat so the peer sees it right away.
                            if let Some(svc) = svc {
                                let _ = svc.broadcast_avatar(group_id).await;
                            }
                        }
                        Err(e) => {
                            error_msg.set(humanize_create_chat_error(&e, &i18n));
                        }
                    }
                }
                None => {
                    error_msg.set(t!("chat.create_direct.no_api").to_string());
                }
            }
            loading.set(false);
        });
        }
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
                <h1 class="text-lg font-semibold text-foreground">"Sigil"</h1>
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
                    let users_for_rows = users_state.clone();
                    let list = chats();
                    if list.is_empty() {
                        view! {
                            <div class="flex flex-col items-center justify-center h-full p-8 text-center">
                                <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground mb-4">
                                    <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/>
                                </svg>
                                <p class="text-sm text-muted-foreground mb-2">{t!("chat.list.empty")}</p>
                                <p class="text-xs text-muted-foreground/70">{t!("chat.list.empty.hint")}</p>
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
                                    // Peer avatar (direct chats): reactive so it
                                    // pops in as soon as an AvatarUpdate lands.
                                    let avatar_src = {
                                        let users = users_for_rows.clone();
                                        let group_id = chat.group_id;
                                        let is_direct = chat.chat_type == crate::state::chats::ChatType::Direct;
                                        Signal::derive(move || {
                                            if !is_direct {
                                                return None;
                                            }
                                            let users = users.as_ref()?;
                                            let peer = users.peer_by_group.get().get(&group_id).copied()?;
                                            users.avatar_by_id.get().get(&peer).cloned()
                                        })
                                    };

                                    // Long-press → open the chat action sheet.
                                    let timer_id: RwSignal<Option<i32>> = RwSignal::new(None);
                                    let triggered = RwSignal::new(false);
                                    let chat_for_press = chat.clone();
                                    let clear_timer = move || {
                                        if let Some(id) = timer_id.get_untracked() {
                                            if let Some(w) = web_sys::window() { w.clear_timeout_with_handle(id); }
                                            timer_id.set(None);
                                        }
                                    };
                                    let on_pointerdown = {
                                        let chat_for_press = chat_for_press.clone();
                                        let clear_timer = clear_timer.clone();
                                        move |e: leptos::ev::PointerEvent| {
                                            if e.button() > 0 { return; }
                                            clear_timer();
                                            triggered.set(false);
                                            let chat_to_set = chat_for_press.clone();
                                            let cb = Closure::<dyn FnMut()>::new(move || {
                                                triggered.set(true);
                                                action_sheet_chat.set(Some(chat_to_set.clone()));
                                                timer_id.set(None);
                                            });
                                            if let Some(window) = web_sys::window() {
                                                if let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                                    cb.as_ref().unchecked_ref(),
                                                    400,
                                                ) { timer_id.set(Some(id)); }
                                            }
                                            cb.forget();
                                        }
                                    };
                                    let on_pointerup = {
                                        let clear_timer = clear_timer.clone();
                                        move |_: leptos::ev::PointerEvent| clear_timer()
                                    };
                                    let on_pointercancel = {
                                        let clear_timer = clear_timer.clone();
                                        move |_: leptos::ev::PointerEvent| clear_timer()
                                    };

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
                                            style="touch-action: pan-y; user-select: none"
                                            on:pointerdown=on_pointerdown
                                            on:pointerup=on_pointerup
                                            on:pointercancel=on_pointercancel
                                            on:contextmenu=move |e: leptos::ev::MouseEvent| {
                                                e.prevent_default();
                                                action_sheet_chat.set(Some(chat_for_press.clone()));
                                            }
                                            on:click=move |_| {
                                                // Suppress click that just triggered long-press.
                                                if triggered.get_untracked() {
                                                    triggered.set(false);
                                                    return;
                                                }
                                                // Already open → don't re-select; re-setting the
                                                // signal re-runs the load effect and needlessly
                                                // rebuilds the chat panel.
                                                if selected.get_untracked() == Some(chat.group_id) {
                                                    return;
                                                }
                                                let sel = selected;
                                                crate::state::back_stack::push(move || sel.set(None));
                                                selected.set(Some(chat.group_id));
                                                if let Some(ref f) = cb {
                                                    f(chat.group_id.to_string());
                                                }
                                            }
                                        >
                                            <AvatarPlaceholder initials={initials} src=avatar_src />
                                            <div class="min-w-0 flex-1">
                                                <div class="flex items-center justify-between gap-2">
                                                    <div class="flex items-center gap-1.5 min-w-0">
                                                        <span class="text-sm font-medium text-foreground truncate">
                                                            {chat.display_name.clone()}
                                                        </span>
                                                        {if chat.muted {
                                                            view! { <Icon name="bell-off" class_name="h-3 w-3 text-muted-foreground shrink-0"/> }.into_any()
                                                        } else { view! {}.into_any() }}
                                                        {if chat.pinned {
                                                            view! { <Icon name="pin" class_name="h-3 w-3 text-muted-foreground shrink-0"/> }.into_any()
                                                        } else { view! {}.into_any() }}
                                                    </div>
                                                    {chat.last_message_at.map(|ts| {
                                                        view! {
                                                            <span class="text-xs text-muted-foreground shrink-0">
                                                                {format_timestamp(ts)}
                                                            </span>
                                                        }
                                                    })}
                                                </div>
                                                <div class="flex items-center justify-between gap-2 mt-0.5">
                                                    <div class="flex items-center gap-1 min-w-0 text-xs text-muted-foreground">
                                                        {render_last_message_preview(&chat)}
                                                    </div>
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

        // Chat action sheet — opened by long-press on a row.
        {
            let cs = chats_state_for_sheet;
            let close_action_sheet = close_action_sheet;
            let close_for_pin = move || action_sheet_chat.set(None);
            let close_for_mute = close_for_pin.clone();
            let close_for_arch = close_for_pin.clone();
            view! {
                <Sheet
                    is_open=action_sheet_open
                    on_close=close_action_sheet
                    side="bottom".to_string()
                >
                    <SheetHeader>
                        <SheetTitle>
                            {move || action_sheet_chat.get().map(|c| c.display_name).unwrap_or_default()}
                        </SheetTitle>
                    </SheetHeader>
                    <div class="space-y-1">
                        {
                            let cs = cs.clone();
                            view! {
                                <button
                                    class="flex w-full items-center gap-3 rounded-md px-3 py-3 text-left text-sm hover:bg-accent"
                                    on:click=move |_| {
                                        if let Some(c) = action_sheet_chat.get_untracked() { cs.toggle_pin(c.group_id); }
                                        close_for_pin();
                                    }
                                >
                                    <Icon name="pin" class_name="h-4 w-4"/>
                                    {move || {
                                        let pinned = action_sheet_chat.get().map_or(false, |c| c.pinned);
                                        if pinned { t!("chat.unpinChat") } else { t!("chat.pinChat") }
                                    }}
                                </button>
                            }
                        }
                        {
                            let cs = cs.clone();
                            view! {
                                <button
                                    class="flex w-full items-center gap-3 rounded-md px-3 py-3 text-left text-sm hover:bg-accent"
                                    on:click=move |_| {
                                        if let Some(c) = action_sheet_chat.get_untracked() { cs.toggle_mute(c.group_id); }
                                        close_for_mute();
                                    }
                                >
                                    <Icon name="bell" class_name="h-4 w-4"/>
                                    {move || {
                                        let muted = action_sheet_chat.get().map_or(false, |c| c.muted);
                                        if muted { t!("chat.unmute") } else { t!("chat.mute") }
                                    }}
                                </button>
                            }
                        }
                        {
                            let cs = cs.clone();
                            view! {
                                <button
                                    class="flex w-full items-center gap-3 rounded-md px-3 py-3 text-left text-sm hover:bg-accent"
                                    on:click=move |_| {
                                        if let Some(c) = action_sheet_chat.get_untracked() { cs.toggle_archive(c.group_id); }
                                        close_for_arch();
                                    }
                                >
                                    <Icon name="archive" class_name="h-4 w-4"/>
                                    {t!("chat.archiveChat")}
                                </button>
                            }
                        }
                    </div>
                </Sheet>
            }
        }

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

/// Map a create-chat API failure to a message a human can act on. The raw
/// codes ("api error: 404 ERR_NOT_FOUND") must never reach the dialog.
/// Takes the i18n handle explicitly — this runs inside spawn_local where
/// the context-based `t!` macro would panic.
fn humanize_create_chat_error(e: &messenger_core::api::ApiError, i18n: &I18n) -> String {
    use messenger_core::api::ApiError;
    match e {
        ApiError::Api { status: 404, .. } => i18n.t("chat.create_direct.userNotFound"),
        ApiError::Api { status: 400, body } => {
            let reason = body
                .details
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or_default();
            if reason.contains("yourself") {
                i18n.t("chat.create_direct.self")
            } else if reason.contains("no active devices") {
                i18n.t("chat.create_direct.noDevices")
            } else {
                i18n.t("chat.create_direct.failed")
            }
        }
        ApiError::Transport(_) => i18n.t("error.network"),
        _ => i18n.t("chat.create_direct.failed"),
    }
}

/// Render the chat-list last-message snippet — a small icon for media kinds
/// followed by either the body text (truncated) or a localized label.
fn render_last_message_preview(chat: &Chat) -> AnyView {
    let preview = chat.last_message_preview.clone().unwrap_or_default();
    let (icon, label) = match chat.last_message_kind {
        Some(MessageKind::Image) => (Some("image"), t!("chat.preview.image")),
        Some(MessageKind::Video) => (Some("film"), t!("chat.preview.video")),
        Some(MessageKind::Voice) => (Some("mic"), t!("chat.preview.voice")),
        Some(MessageKind::File) => (
            Some("paperclip"),
            if preview.is_empty() { t!("chat.preview.file") } else { preview.clone() },
        ),
        _ => (None, preview.clone()),
    };
    let label = if matches!(chat.last_message_kind, Some(MessageKind::Image | MessageKind::Video | MessageKind::Voice))
        && !preview.is_empty()
    {
        preview
    } else {
        label
    };
    view! {
        {icon.map(|name| view! {
            <Icon name=name class_name="h-4 w-4 shrink-0".to_string()/>
        })}
        <span class="truncate">{label}</span>
    }.into_any()
}

fn get_initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// 48×48 avatar circle: peer image when cached, initials fallback otherwise.
#[component]
fn AvatarPlaceholder(
    initials: String,
    #[prop(optional)] src: Option<Signal<Option<String>>>,
) -> impl IntoView {
    view! {
        <div class="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted text-sm font-semibold text-muted-foreground">
            {move || match src.and_then(|s| s.get()) {
                Some(url) => view! {
                    <img class="h-full w-full object-cover" src=url alt=""/>
                }.into_any(),
                None => initials.clone().into_any(),
            }}
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

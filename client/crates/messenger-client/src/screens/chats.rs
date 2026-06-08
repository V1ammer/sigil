//! Chats screen — sidebar + main area with real message rendering.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

use uuid::Uuid;

use crate::chat::input_bar::{InputBar, InputPreview};
use crate::chat::message_bridge::display_vec_to_mock;
use crate::chat::message_list::MessageList;
use crate::i18n::I18n;
use crate::sidebar::real_chat_list::RealChatList;
use crate::state::chats::ChatsState;
use crate::state::message_service::MessageService;
use crate::state::session::{build_api_client, use_session, SessionState};
use crate::t;

fn display_name_for(chats: &[crate::state::chats::Chat], group_id: Uuid) -> String {
    chats
        .iter()
        .find(|c| c.group_id == group_id)
        .map(|c| c.display_name.clone())
        .unwrap_or_else(|| group_id.to_string())
}

#[must_use]
#[component]
pub fn ChatsScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let _session = use_session();
    let chats_state = use_context::<ChatsState>().expect("ChatsState must be provided");
    let message_service = use_context::<MessageService>().expect("MessageService must be provided");
    let messages_state = message_service.messages.clone();
    let selected = chats_state.selected;
    let chats_state_clone = chats_state.clone();
    let chats_signal = chats_state.chats;
    let loading_messages = RwSignal::new(false);

    // Load chats from server on mount
    spawn_local(async move {
        if let Some(api) = build_api_client() {
            let _ = chats_state_clone.load_from_server(&api).await;
        }
    });

    let params = use_params_map();
    let url_id = move || params.get().get("id").map(|s| s.clone()).unwrap_or_default();
    let initial_id = url_id();
    if !initial_id.is_empty() {
        if let Ok(uid) = initial_id.parse() {
            selected.set(Some(uid));
        }
    }

    // Load messages when a chat is selected
    let selected_for_load = selected.clone();
    let msg_svc = message_service.clone();
    let loading = loading_messages;
    Effect::new(move |_| {
        if let Some(group_id) = selected_for_load.get() {
            loading.set(true);
            let svc = msg_svc.clone();
            spawn_local(async move {
                svc.load_messages(group_id).await;
                loading.set(false);
            });
        }
    });

    let on_chat_select_arc = Arc::new({
        let cid = selected.clone();
        move |id: String| {
            if let Ok(uid) = id.parse() {
                cid.set(Some(uid));
            }
        }
    }) as Arc<dyn Fn(String) + Send + Sync + 'static>;

    let on_back = move |_| selected.set(None);

    // Send handler
    let on_send_handler = {
        let msg_svc = message_service.clone();
        let selected = selected.clone();
        Arc::new(move |text: String| {
            if let Some(group_id) = selected.get_untracked() {
                let svc = msg_svc.clone();
                spawn_local(async move {
                    svc.send_text(group_id, &text, None).await;
                });
            }
        }) as Arc<dyn Fn(String) + Send + Sync + 'static>
    };

    let session = use_session();
    let own_user_id = move || match session.state.get() {
        SessionState::Authenticated { ref identity, .. } => identity.user_id.to_string(),
        _ => String::new(),
    };

    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let locale = Signal::derive(move || i18n.locale.get().into());

    // Reply/Edit preview state
    let preview = RwSignal::new(InputPreview::None);
    let msg_svc = message_service.clone();
    let sel = selected.clone();

    // on_reply callback for MessageList
    let on_reply_list = Arc::new({
        let msg_svc = msg_svc.clone();
        let preview = preview.clone();
        let sel = sel.clone();
        move |msg_id: &str| {
            // Look up the message content from the store
            let group_id = sel.get_untracked();
            if let (Some(gid), Ok(id)) = (group_id, uuid::Uuid::parse_str(msg_id)) {
                let msgs = msg_svc.messages.by_group.get_untracked();
                if let Some(msg) = msgs.get(&gid).and_then(|ms| ms.iter().find(|m| m.id == id)) {
                    let sender = msg.sender_user_id.to_string();
                    let text = match &msg.body {
                        crate::state::messages::MessageBody::Text(t) => t.clone(),
                        _ => msg_id.to_string(),
                    };
                    preview.set(InputPreview::Reply {
                        sender_name: sender,
                        content: text,
                    });
                }
            }
        }
    }) as Arc<dyn Fn(&str) + Send + Sync + 'static>;

    // on_edit callback for MessageList
    let on_edit_list = Arc::new({
        let msg_svc = msg_svc.clone();
        let preview = preview.clone();
        let sel = sel.clone();
        move |msg_id: &str| {
            let group_id = sel.get_untracked();
            if let (Some(gid), Ok(id)) = (group_id, uuid::Uuid::parse_str(msg_id)) {
                let msgs = msg_svc.messages.by_group.get_untracked();
                if let Some(msg) = msgs.get(&gid).and_then(|ms| ms.iter().find(|m| m.id == id)) {
                    let text = match &msg.body {
                        crate::state::messages::MessageBody::Text(t) => t.clone(),
                        _ => String::new(),
                    };
                    preview.set(InputPreview::Edit {
                        message_id: msg_id.to_string(),
                        content: text,
                    });
                }
            }
        }
    }) as Arc<dyn Fn(&str) + Send + Sync + 'static>;

    // on_delete callback for MessageList
    let on_delete_list = Arc::new({
        let msg_svc = msg_svc.clone();
        let sel = sel.clone();
        move |msg_id: &str| {
            let group_id = sel.get_untracked();
            if let (Some(gid), Ok(id)) = (group_id, uuid::Uuid::parse_str(msg_id)) {
                let svc = msg_svc.clone();
                spawn_local(async move {
                    svc.delete_message(gid, id).await;
                });
            }
        }
    }) as Arc<dyn Fn(&str) + Send + Sync + 'static>;

    // on_reaction callback for MessageList
    let on_reaction_list = Arc::new({
        let msg_svc = msg_svc.clone();
        let sel = sel.clone();
        move |msg_id: &str, emoji: String| {
            let group_id = sel.get_untracked();
            if let (Some(gid), Ok(id)) = (group_id, uuid::Uuid::parse_str(msg_id)) {
                let svc = msg_svc.clone();
                spawn_local(async move {
                    svc.toggle_reaction(gid, id, &emoji).await;
                });
            }
        }
    }) as Arc<dyn Fn(&str, String) + Send + Sync + 'static>;

    view! {
        <div class="flex h-screen-safe bg-background overflow-hidden">
            {/* Sidebar */}
            <div class=move || {
                let base = "flex-col border-r border-border bg-sidebar overflow-hidden";
                if selected.get().is_some() {
                    format!("hidden md:flex md:w-80 lg:w-96 {}", base)
                } else {
                    format!("flex w-full md:w-80 lg:w-96 {}", base)
                }
            }>
                <RealChatList on_chat_select=on_chat_select_arc />
            </div>

            {/* Main area */}
            {move || {
                let on_reply_list = on_reply_list.clone();
                let on_edit_list = on_edit_list.clone();
                let on_delete_list = on_delete_list.clone();
                let on_reaction_list = on_reaction_list.clone();
                selected.get().map(|group_id| {
                let name = display_name_for(&chats_signal.get(), group_id);
                let msgs = messages_state.for_group(group_id);
                let is_loading = loading_messages.get();
                let on_send = on_send_handler.clone();

                view! {
                    <div class=move || {
                        if selected.get().is_some() {
                            "flex flex-1 flex-col w-full min-w-0"
                        } else {
                            "hidden md:flex flex-1 flex-col"
                        }
                    }>
                        {/* Header */}
                        <div class="flex items-center gap-3 border-b border-border px-4 py-3 shrink-0">
                            <button
                                class="md:hidden inline-flex h-9 w-9 items-center justify-center rounded-md hover:bg-accent transition-colors"
                                on:click=on_back
                                title={t!("back")}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/>
                                </svg>
                            </button>
                            <span class="text-sm font-medium text-foreground truncate">{name.clone()}</span>
                        </div>

                        {/* Messages */}
                        {move || {
                            if is_loading {
                                view! {
                                    <div class="flex-1 flex items-center justify-center">
                                        <span class="h-8 w-8 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                                    </div>
                                }.into_any()
                            } else {
                                let display_msgs = msgs();
                                if display_msgs.is_empty() {
                                    view! {
                                        <div class="flex-1 flex flex-col items-center justify-center p-8 text-center">
                                            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="text-muted-foreground mb-4">
                                                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                                            </svg>
                                            <p class="text-sm text-muted-foreground">{t!("chat.list.empty")}</p>
                                        </div>
                                    }.into_any()
                                } else {
                                    let mock_msgs = display_vec_to_mock(&display_msgs, &own_user_id());
                                    view! {
                                        <MessageList
                                            lang=locale
                                            messages=mock_msgs
                                            is_mobile=Signal::derive(|| false)
                                            on_reply=Box::new({
                                                let r = on_reply_list.clone();
                                                move |id: &str| r(id)
                                            })
                                            on_edit=Box::new({
                                                let e = on_edit_list.clone();
                                                move |id: &str| e(id)
                                            })
                                            on_delete=Box::new({
                                                let d = on_delete_list.clone();
                                                move |id: &str| d(id)
                                            })
                                            on_reaction=Box::new({
                                                let r = on_reaction_list.clone();
                                                move |id: &str, emoji: String| r(id, emoji)
                                            })
                                        />
                                    }.into_any()
                                }
                            }
                        }}

                        {/* Input bar — always visible when a chat is selected */}
                        <div class="shrink-0">
                            <InputBar
                                locale=locale
                                preview=preview.get()
                                on_send=Box::new({
                                    let os = on_send.clone();
                                    let prev = preview.clone();
                                    let svc = message_service.clone();
                                    let sel = selected.clone();
                                    move |text: String| {
                                        let gid = sel.get_untracked();
                                        if let Some(group_id) = gid {
                                            match prev.get_untracked() {
                                                InputPreview::Edit { message_id, .. } => {
                                                    if let Ok(orig_id) = uuid::Uuid::parse_str(&message_id) {
                                                        let s = svc.clone();
                                                        spawn_local(async move {
                                                            s.edit_message(group_id, orig_id, &text).await;
                                                        });
                                                        prev.set(InputPreview::None);
                                                    }
                                                }
                                                _ => {
                                                    os(text);
                                                }
                                            }
                                        }
                                    }
                                })
                                on_cancel_preview=Box::new({
                                    let prev = preview.clone();
                                    move || prev.set(InputPreview::None)
                                })
                            />
                        </div>
                    </div>
                }.into_any()
            }).unwrap_or_else(|| {
                view! {
                    <div class="hidden md:flex flex-1 flex-col items-center justify-center bg-muted/30">
                        <div class="flex h-16 w-16 items-center justify-center rounded-full bg-muted">
                            <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
                        </div>
                        <h2 class="mt-4 text-lg font-medium text-foreground">{t!("welcome.title")}</h2>
                        <p class="mt-1 text-sm text-muted-foreground">{t!("welcome.hint")}</p>
                    </div>
                }.into_any()
            })}
            } // close move || block
        </div>
    }
}

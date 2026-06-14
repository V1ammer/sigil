//! Chats screen — sidebar + main area with real message rendering.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

use uuid::Uuid;

use crate::chat::chat_header::ChatHeader;
use crate::components::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter,
    AlertDialogHeader, AlertDialogTitle,
};
use crate::chat::input_bar::{InputBar, InputPreview};
use crate::chat::message_bridge::{display_to_mock, display_vec_to_mock};
use crate::chat::message_list::MessageList;
use crate::chat::thread_panel::ThreadPanel;
use crate::components::dialog::{Dialog, DialogHeader, DialogTitle};
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::i18n::I18n;
use crate::mock;
use crate::sidebar::real_chat_list::RealChatList;
use crate::state::chats::{Chat as StateChat, ChatType, ChatsState};
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

/// Build a minimal `mock::Chat` from a `state::chats::Chat` for use by `ChatHeader`.
///
/// `ChatHeader` was authored against the mock structure during UI-first development;
/// this avoids duplicating its prop surface.
fn to_mock_chat(c: &StateChat, display_name: &str) -> mock::Chat {
    mock::Chat {
        id: c.group_id.to_string(),
        chat_type: if c.chat_type == ChatType::Direct { "direct" } else { "group" }.to_string(),
        name: display_name.to_string(),
        avatar_url: None,
        participant_ids: Vec::new(),
        last_message: None,
        unread_count: c.unread_count,
        is_pinned: c.pinned,
        is_muted: c.muted,
        muted_until: None,
        is_archived: false,
        has_security_changes: false,
        device_count: None,
    }
}

/// Placeholder chat for the header while real chat data is being fetched.
fn placeholder_mock_chat(group_id: Uuid, display_name: &str) -> mock::Chat {
    mock::Chat {
        id: group_id.to_string(),
        chat_type: "direct".to_string(),
        name: display_name.to_string(),
        avatar_url: None,
        participant_ids: Vec::new(),
        last_message: None,
        unread_count: 0,
        is_pinned: false,
        is_muted: false,
        muted_until: None,
        is_archived: false,
        has_security_changes: false,
        device_count: None,
    }
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

    // Reactive peer avatar for the open chat's header. Computed in its own
    // top-level scope (not inside the per-chat panel closure) so it always
    // tracks the latest avatar_by_id / peer_by_group and survives panel
    // re-renders — the same reliable pattern the sidebar rows use.
    let users_for_header_avatar = use_context::<crate::state::users::UsersState>();
    let typing_state = use_context::<crate::state::typing::TypingState>();
    let header_avatar: Signal<Option<String>> = Signal::derive(move || {
        let gid = selected.get()?;
        let us = users_for_header_avatar.as_ref()?;
        let peer = us.peer_by_group.with(|m| m.get(&gid).copied())?;
        us.avatar_by_id.with(|m| m.get(&peer).cloned())
    });
    // Per-chat composer drafts, created once so they outlive the re-renders
    // that rebuild the chat view (incoming messages bump the chat list).
    let drafts: RwSignal<std::collections::HashMap<Uuid, String>> =
        RwSignal::new(std::collections::HashMap::new());
    // Per-chat staged attachment (picked but not yet sent) — same rationale as
    // `drafts`: it must outlive the chat-view rebuilds incoming messages cause.
    let staged: RwSignal<std::collections::HashMap<Uuid, crate::chat::input_bar::StagedAttachment>> =
        RwSignal::new(std::collections::HashMap::new());

    // Android "Share into chat": when a file was shared into the app and the
    // user picks (or is already in) a chat, stage it into that chat's composer.
    {
        let share_state = use_context::<crate::state::share::ShareState>();
        Effect::new(move |_| {
            let Some(share) = share_state else { return };
            let selected_gid = selected.get();
            let has_pending = share.has_pending();
            if let (Some(gid), true) = (selected_gid, has_pending) {
                if let Some(payload) = share.take_one() {
                    crate::chat::input_bar::stage_into(staged, gid, payload);
                }
            }
        });
    }

    // Load chats from server on mount, then hydrate each chat's last-message
    // preview by loading messages — `GroupSummary` from the server only carries
    // metadata, so the sidebar snippets need a per-chat message fetch.
    let ms_for_hydrate = message_service.clone();
    spawn_local(async move {
        if let Some(api) = build_api_client() {
            let _ = chats_state_clone.load_from_server(&api).await;
            let group_ids: Vec<_> = chats_state_clone
                .chats
                .get_untracked()
                .iter()
                .map(|c| c.group_id)
                .collect();
            for gid in group_ids {
                ms_for_hydrate.load_messages(gid).await;
            }
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

    // Send handler — text only, no reply context (reply handled in InputBar callback below)
    let on_send_handler = {
        let msg_svc = message_service.clone();
        let selected = selected.clone();
        Arc::new(move |text: String, reply_to: Option<uuid::Uuid>| {
            if let Some(group_id) = selected.get_untracked() {
                let svc = msg_svc.clone();
                spawn_local(async move {
                    svc.send_text(group_id, &text, reply_to).await;
                });
            }
        }) as Arc<dyn Fn(String, Option<uuid::Uuid>) + Send + Sync + 'static>
    };

    let session = use_session();
    let own_user_id = move || match session.state.get() {
        SessionState::Authenticated { ref identity, .. } => identity.user_id.to_string(),
        _ => String::new(),
    };

    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let locale = Signal::derive(move || i18n.locale.get().into());

    // Mobile detection — use narrow layout on small screens (mobile/Android)
    let is_mobile = Signal::derive(move || {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.inner_width().ok())
                .and_then(|w| w.as_f64())
                .map(|w| w < 768.0)
                .unwrap_or(false)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    });

    // Reply/Edit preview state
    let preview = RwSignal::new(InputPreview::None);
    let msg_svc = message_service.clone();
    let sel = selected.clone();

    // on_reply callback for MessageList
    let users_state_for_reply = use_context::<crate::state::users::UsersState>();
    let on_reply_list = Arc::new({
        let msg_svc = msg_svc.clone();
        let preview = preview.clone();
        let sel = sel.clone();
        let users = users_state_for_reply.clone();
        move |msg_id: &str| {
            // Look up the message content from the store
            let group_id = sel.get_untracked();
            if let (Some(gid), Ok(id)) = (group_id, uuid::Uuid::parse_str(msg_id)) {
                let msgs = msg_svc.messages.by_group.get_untracked();
                if let Some(msg) = msgs.get(&gid).and_then(|ms| ms.iter().find(|m| m.id == id)) {
                    // Prefer the cached display name, fall back to a short id.
                    let sender = msg
                        .sender_display_name
                        .clone()
                        .or_else(|| users.as_ref().and_then(|u| u.get(msg.sender_user_id)))
                        .unwrap_or_else(|| {
                            msg.sender_user_id
                                .to_string()
                                .chars()
                                .take(8)
                                .collect::<String>()
                        });
                    let text = match &msg.body {
                        crate::state::messages::MessageBody::Text(t) => t.clone(),
                        _ => msg_id.to_string(),
                    };
                    preview.set(InputPreview::Reply {
                        message_id: msg_id.to_string(),
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

    // Forward: clicking "Forward" in the message menu records the source
    // (group, message) and opens a chat-picker dialog. The actual re-send runs
    // from the dialog once a target chat is chosen.
    let notifications = use_context::<NotificationsState>().expect("NotificationsState must be provided");
    let forward_source: RwSignal<Option<(Uuid, Uuid)>> = RwSignal::new(None);
    let show_forward = RwSignal::new(false);
    let on_forward_list = Arc::new({
        let sel = sel.clone();
        move |msg_id: &str| {
            let group_id = sel.get_untracked();
            if let (Some(gid), Ok(id)) = (group_id, uuid::Uuid::parse_str(msg_id)) {
                forward_source.set(Some((gid, id)));
                show_forward.set(true);
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

    // Currently open thread root (None = no thread).
    let thread_root: RwSignal<Option<Uuid>> = RwSignal::new(None);
    let on_thread_open_list = Arc::new({
        move |msg_id: &str| {
            if let Ok(id) = uuid::Uuid::parse_str(msg_id) {
                thread_root.set(Some(id));
            }
        }
    }) as Arc<dyn Fn(&str) + Send + Sync + 'static>;

    // Dedicated handles for the forward chat-picker dialog at the end of the
    // view. StoredValue is Copy, so the reactive list closure stays `Fn`.
    let forward_msg_svc = StoredValue::new(message_service.clone());
    let forward_notifications = StoredValue::new(notifications.clone());

    view! {
        <div class="flex h-full bg-background overflow-hidden">
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
            {
                let messages_state_for_main = messages_state.clone();
                let messages_state_for_thread = messages_state.clone();
                let msg_svc_for_thread = msg_svc.clone();
                let sel_for_thread = sel.clone();
                let locale_for_thread = locale;
                view! {
            <div class="contents">
            {move || {
                let on_reply_list = on_reply_list.clone();
                let on_edit_list = on_edit_list.clone();
                let on_delete_list = on_delete_list.clone();
                let on_forward_list = on_forward_list.clone();
                let on_reaction_list = on_reaction_list.clone();
                let on_thread_open_list = on_thread_open_list.clone();
                let messages_state_for_inner = messages_state_for_main.clone();
                selected.get().map(|group_id| {
                let chats_now = chats_signal.get();
                let name = display_name_for(&chats_now, group_id);
                let state_chat = chats_now.iter().find(|c| c.group_id == group_id).cloned();
                let msgs = messages_state_for_inner.for_group(group_id);
                let is_loading = loading_messages.get();
                let on_send = on_send_handler.clone();

                // Wire chat-header actions to ChatsState mutators.
                let cs = chats_state.clone();
                let on_pin_toggle = Box::new(move || cs.toggle_pin(group_id)) as Box<dyn Fn() + Send + Sync + 'static>;
                let cs2 = chats_state.clone();
                let on_mute_toggle = Box::new(move || cs2.toggle_mute(group_id)) as Box<dyn Fn() + Send + Sync + 'static>;
                let on_mark_read_cb = Box::new(|| {}) as Box<dyn Fn() + Send + Sync + 'static>;
                // Delete a chat: drop it from the local list and permanently
                // clear its conversation, then leave the chat view. The server
                // keeps the (deduped) group and has no delete endpoint, so
                // clear_conversation records a watermark — starting a chat with
                // the same person again reopens it EMPTY instead of restoring
                // the old history.
                // Deletion is destructive (server-side: messages + attachments),
                // so the header actions only OPEN a confirmation; the actual
                // delete runs from the dialog's confirm button.
                let show_delete_confirm = RwSignal::new(false);
                let do_delete: StoredValue<Arc<dyn Fn() + Send + Sync + 'static>> = StoredValue::new({
                    let cs = chats_state.clone();
                    let svc = message_service.clone();
                    Arc::new(move || {
                        cs.delete_chat(group_id);
                        svc.clear_conversation(group_id);
                        // Server-side delete: drop the group, its messages and
                        // attachments so nothing can be restored and a re-created
                        // chat with the same person starts empty.
                        spawn_local(async move {
                            if let Some(api) = build_api_client() {
                                if let Err(e) = api.delete_group(group_id).await {
                                    web_sys::console::warn_1(
                                        &format!("[delete_group] {e}").into(),
                                    );
                                }
                            }
                        });
                        selected.set(None);
                        crate::state::back_stack::pop();
                    })
                });
                let on_leave_cb = Box::new(move || show_delete_confirm.set(true))
                    as Box<dyn Fn() + Send + Sync + 'static>;
                let on_delete_cb = Box::new(move || show_delete_confirm.set(true))
                    as Box<dyn Fn() + Send + Sync + 'static>;
                let on_back_cb = Box::new(|| crate::state::back_stack::pop())
                    as Box<dyn Fn() + Send + Sync + 'static>;
                let chat_for_header = state_chat
                    .as_ref()
                    .map(|c| to_mock_chat(c, &name))
                    .unwrap_or_else(|| placeholder_mock_chat(group_id, &name));
                // The peer avatar is supplied reactively to ChatHeader via the
                // top-level `header_avatar` signal — kept out of this closure so
                // an avatar landing doesn't rebuild the whole chat panel.

                view! {
                    <div class=move || {
                        if selected.get().is_some() {
                            "flex flex-1 flex-col w-full min-w-0 min-h-0"
                        } else {
                            "hidden md:flex flex-1 flex-col"
                        }
                    }>
                        <ChatHeader
                            lang=locale
                            chat=chat_for_header
                            avatar=header_avatar
                            on_back=on_back_cb
                            on_pin_toggle=on_pin_toggle
                            on_mute_toggle=on_mute_toggle
                            on_mark_read=on_mark_read_cb
                            on_leave_group=on_leave_cb
                            on_delete_chat=on_delete_cb
                        />

                        // Confirmation before the (irreversible) chat deletion.
                        <AlertDialog
                            is_open=show_delete_confirm
                            on_close=Box::new(move || show_delete_confirm.set(false))
                        >
                            <AlertDialogHeader>
                                <AlertDialogTitle>{t!("chat.deleteConfirmTitle")}</AlertDialogTitle>
                                <AlertDialogDescription>{t!("chat.deleteConfirmDesc")}</AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                                <AlertDialogCancel on_click=Box::new(move || show_delete_confirm.set(false))>
                                    {t!("common.cancel")}
                                </AlertDialogCancel>
                                <AlertDialogAction
                                    class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                    on_click=Box::new(move || {
                                        show_delete_confirm.set(false);
                                        do_delete.get_value()();
                                    })
                                >
                                    {t!("common.delete")}
                                </AlertDialogAction>
                            </AlertDialogFooter>
                        </AlertDialog>

                        {/* Messages */}
                        {move || {
                            if is_loading {
                                view! {
                                    <div class="flex-1 flex items-center justify-center">
                                        <span class="h-8 w-8 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                                    </div>
                                }.into_any()
                            } else {
                                let display_msgs = msgs.get();
                                if display_msgs.is_empty() {
                                    view! {
                                        <div class="flex-1 flex flex-col items-center justify-center p-8 text-center">
                                            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="text-muted-foreground mb-4">
                                                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                                            </svg>
                                            <p class="text-sm text-muted-foreground">{t!("chat.messages.empty")}</p>
                                        </div>
                                    }.into_any()
                                } else {
                                    let mock_msgs = display_vec_to_mock(&display_msgs, &own_user_id());
                                    view! {
                                        <MessageList
                                            lang=locale
                                            messages=mock_msgs
                                            is_mobile=is_mobile
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
                                            on_forward=Box::new({
                                                let f = on_forward_list.clone();
                                                move |id: &str| f(id)
                                            })
                                            on_reaction=Box::new({
                                                let r = on_reaction_list.clone();
                                                move |id: &str, emoji: String| r(id, emoji)
                                            })
                                            on_thread_open=Box::new({
                                                let t = on_thread_open_list.clone();
                                                move |id: &str| t(id)
                                            })
                                            // The reply-count badge under a root
                                            // message opens the same thread panel.
                                            on_thread_click=Box::new({
                                                let t = on_thread_open_list.clone();
                                                move |id: &str| t(id)
                                            })
                                        />
                                    }.into_any()
                                }
                            }
                        }}

                        {/* Peer typing indicator — reactive to TypingState */}
                        {move || {
                            typing_state
                                .filter(|ts| ts.is_typing(group_id))
                                .map(|_| view! {
                                    <div class="shrink-0 px-4 pb-1 text-xs italic text-muted-foreground">
                                        {t!("chat.typing")}
                                    </div>
                                })
                        }}

                        {/* Input bar — always visible when a chat is selected */}
                        <div class="shrink-0">
                            <InputBar
                                locale=locale
                                group_id=group_id
                                drafts=drafts
                                staged=staged
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
                                                InputPreview::Reply { message_id, .. } => {
                                                    let reply_to = uuid::Uuid::parse_str(&message_id).ok();
                                                    os(text, reply_to);
                                                    prev.set(InputPreview::None);
                                                }
                                                InputPreview::None => {
                                                    os(text, None);
                                                }
                                            }
                                        }
                                    }
                                })
                                on_send_voice=Box::new({
                                    let svc = message_service.clone();
                                    let sel = selected.clone();
                                    move |payload: crate::chat::input_bar::VoicePayload| {
                                        if let Some(group_id) = sel.get_untracked() {
                                            let svc = svc.clone();
                                            spawn_local(async move {
                                                svc.send_voice(group_id, payload).await;
                                            });
                                        }
                                    }
                                })
                                on_cancel_preview=Box::new({
                                    let prev = preview.clone();
                                    move || prev.set(InputPreview::None)
                                })
                                on_send_attachment=Box::new({
                                    let svc = message_service.clone();
                                    let sel = selected.clone();
                                    move |payload: crate::chat::input_bar::AttachmentPayload| {
                                        if let Some(group_id) = sel.get_untracked() {
                                            let svc = svc.clone();
                                            spawn_local(async move {
                                                svc.send_attachment(group_id, payload).await;
                                            });
                                        }
                                    }
                                })
                            />
                        </div>
                    </div>
                }.into_any()
            }).unwrap_or_else(|| {
                view! {
                    <div class="hidden md:flex flex-1 flex-col items-center justify-center bg-muted/30 p-8">
                        <div class="flex h-20 w-20 items-center justify-center rounded-full bg-muted/70 mb-5">
                            <svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground"><path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/></svg>
                        </div>
                        <h2 class="text-lg font-medium text-foreground text-center">{t!("welcome.title")}</h2>
                        <p class="mt-2 text-sm text-muted-foreground text-center max-w-sm">{t!("welcome.hint")}</p>
                    </div>
                }.into_any()
            })}
            } // close move || block

            // Thread panel — slide-over with parent + replies, lives outside the
            // main split so it overlays the whole chat area.
            {
                let msg_svc = msg_svc_for_thread;
                let sel = sel_for_thread;
                let messages_state = messages_state_for_thread;
                let thread_is_open = Signal::derive(move || thread_root.get().is_some());
                let on_close_thread = Box::new(move || thread_root.set(None)) as Box<dyn Fn() + Send + Sync + 'static>;

                let messages_state_p = messages_state.clone();
                let sel_p = sel.clone();
                let parent_message = Signal::derive(move || {
                    let root = thread_root.get()?;
                    let gid = sel_p.get()?;
                    let store = messages_state_p.by_group.get();
                    store
                        .get(&gid)
                        .and_then(|ms| ms.iter().find(|m| m.id == root).cloned())
                        .map(|m| display_to_mock(&m))
                });
                let messages_state_r = messages_state.clone();
                let sel_r = sel.clone();
                let replies = Signal::derive(move || {
                    let root = match thread_root.get() { Some(r) => r, None => return vec![] };
                    let gid = match sel_r.get() { Some(g) => g, None => return vec![] };
                    let store = messages_state_r.by_group.get();
                    store
                        .get(&gid)
                        .map(|ms| {
                            ms.iter()
                                .filter(|m| m.thread_root_id == Some(root))
                                .map(display_to_mock)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                });

                let on_send_reply_arc = Arc::new(move |text: String| {
                    let svc = msg_svc.clone();
                    let gid = sel.get_untracked();
                    let root = thread_root.get_untracked();
                    if let (Some(group_id), Some(root_id)) = (gid, root) {
                        spawn_local(async move {
                            svc.send_text_in_thread(group_id, &text, Some(root_id), Some(root_id))
                                .await;
                        });
                    }
                });
                let on_close_arc = Arc::new(on_close_thread);

                // Rebuild the panel whenever the thread root / replies change —
                // passing `.get()` snapshots once froze it at "no parent".
                view! {
                    {move || {
                        let on_close = {
                            let a = on_close_arc.clone();
                            Box::new(move || a()) as Box<dyn Fn() + Send + Sync + 'static>
                        };
                        let on_send = {
                            let a = on_send_reply_arc.clone();
                            Box::new(move |t: String| a(t))
                                as Box<dyn Fn(String) + Send + Sync + 'static>
                        };
                        view! {
                            <ThreadPanel
                                lang=locale_for_thread
                                is_open=thread_is_open
                                on_close=on_close
                                parent_message=parent_message.get()
                                replies=replies.get()
                                on_send_reply=on_send
                            />
                        }
                    }}
                }
            }
            </div>
                }
            }

            // Forward chat-picker — lists every other chat; choosing one
            // re-sends the selected message into it via MessageService::forward_to.
            <Dialog
                is_open=show_forward
                on_close=Box::new(move || show_forward.set(false))
            >
                <DialogHeader>
                    <DialogTitle>{t!("chat.forwardTitle")}</DialogTitle>
                </DialogHeader>
                <div class="max-h-80 overflow-y-auto py-2 space-y-1">
                    {move || {
                        let src_group = forward_source.get().map(|(g, _)| g);
                        let chats = chats_signal.get();
                        let ok_msg = t!("chat.forwardSent");
                        let err_msg = t!("chat.forwardFailed");
                        if chats.iter().all(|c| src_group == Some(c.group_id)) {
                            return view! {
                                <p class="px-3 py-4 text-center text-sm text-muted-foreground">
                                    {t!("chat.forwardEmpty")}
                                </p>
                            }.into_any();
                        }
                        chats.iter()
                            .filter(|c| src_group.map(|sg| c.group_id != sg).unwrap_or(true))
                            .map(|c| {
                                let gid = c.group_id;
                                let name = display_name_for(&chats, gid);
                                let svc = forward_msg_svc.get_value();
                                let notifications = forward_notifications.get_value();
                                let ok_msg = ok_msg.clone();
                                let err_msg = err_msg.clone();
                                view! {
                                    <button
                                        class="flex w-full items-center rounded-md px-3 py-2 text-left text-sm text-foreground hover:bg-accent"
                                        on:click=move |_| {
                                            let ok_msg = ok_msg.clone();
                                            let err_msg = err_msg.clone();
                                            if let Some((src, mid)) = forward_source.get_untracked() {
                                                let svc = svc.clone();
                                                let notifications = notifications.clone();
                                                spawn_local(async move {
                                                    let ok = svc.forward_to(gid, src, mid).await.is_some();
                                                    notifications.push(
                                                        if ok { ToastKind::Success } else { ToastKind::Error },
                                                        if ok { ok_msg } else { err_msg },
                                                    );
                                                });
                                            }
                                            show_forward.set(false);
                                        }
                                    >
                                        {name}
                                    </button>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </div>
            </Dialog>
        </div>
    }
}

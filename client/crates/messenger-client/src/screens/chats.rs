use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use crate::i18n::{Language, t};
use crate::mock::{self, Chat, Message, ThreadInfo};
use crate::sidebar::chat_list::ChatList;
use crate::chat::chat_header::ChatHeader;
use crate::chat::message_list::MessageList;
use crate::chat::input_bar::{InputBar, InputPreview};
use crate::chat::thread_panel::ThreadPanel;
use crate::chat::profile_sheet::ProfileSheet;
use crate::chat::new_chat_dialog::NewChatDialog;
use crate::chat::media_viewer::MediaViewer;

#[must_use]
#[component]
pub fn ChatsScreen() -> impl IntoView {
    let lang = RwSignal::new(Language::Ru);
    let params = use_params_map();

    let chats = RwSignal::new(mock::mock_chats());
    let selected_chat_id = RwSignal::new(None::<String>);
    let reply_to = RwSignal::new(None::<Message>);
    let editing_message = RwSignal::new(None::<Message>);
    let thread_message_id = RwSignal::new(None::<String>);
    let show_profile = RwSignal::new(false);
    let show_new_chat = RwSignal::new(false);
    let media_viewer_url = RwSignal::new(None::<String>);

    // Read chat ID from URL params
    let chat_id_from_url = move || params.get().get("id").map(|s| s.clone()).unwrap_or_default();
    let url_id = chat_id_from_url();
    if !url_id.is_empty() {
        selected_chat_id.set(Some(url_id));
    }

    let selected_chat = move || {
        let id = selected_chat_id.get();
        id.and_then(|id| chats.get().into_iter().find(|c| c.id == id))
    };

    let chat_messages = move || {
        selected_chat_id.get().map(|id| mock::mock_messages(&id)).unwrap_or_default()
    };

    let thread_parent = move || {
        let tid = thread_message_id.get();
        tid.and_then(|tid| chat_messages().into_iter().find(|m| m.id == tid))
    };

    let on_send = move |content: String| {
        let chat_id = selected_chat_id.get();
        if content.trim().is_empty() || chat_id.is_none() { return; }
        let chat_id = chat_id.unwrap();

        // Add message to mock data
        // In real app this sends via API
        let msg = Message {
            id: format!("msg-{}", mock::now_ms() as u64),
            chat_id: chat_id.clone(),
            sender_id: "user-1".into(),
            sender_name: "Иван Иванов".into(),
            sender_avatar: None,
            msg_type: "text".into(),
            content: content.trim().to_string(),
            timestamp: mock::now_ms(),
            status: "sent".into(),
            is_own: true,
            is_edited: false,
            is_deleted: false,
            reply_to: reply_to.get().map(|r| Box::new(mock::ReplyTo {
                id: r.id.clone(),
                sender_name: r.sender_name.clone(),
                content: r.content.clone(),
            })),
            reactions: vec![],
            thread_count: None,
            duration: None,
            waveform: vec![],
            transcription: None,
            media_url: None,
            thumbnail_url: None,
            file_name: None,
            file_size: None,
            system_action: None,
        };
        mock::add_message(&chat_id, msg);
        reply_to.set(None);
        editing_message.set(None);
    };

    let on_chat_select_arc = std::sync::Arc::new({
        let cid = selected_chat_id.clone();
        move |id: String| cid.set(Some(id))
    }) as std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>;

    view! {
        <div class="flex h-screen-safe bg-background">
            {/* Sidebar */}
            <div class="hidden md:flex w-80 shrink-0 lg:w-96 flex-col border-r border-border bg-sidebar">
                <ChatList
                    on_chat_select=on_chat_select_arc
                />
            </div>

            {/* Main area */}
            {move || selected_chat().map(|chat| {
                view! {
                    <div class="flex flex-1 flex-col">
                        <ChatHeader
                            lang=lang
                            chat={chat.clone()}
                            on_back=Box::new(|| {})
                        />

                        <MessageList
                            lang=lang
                            messages={chat_messages()}
                            on_thread_click=Box::new({
                                let tid = thread_message_id.clone();
                                move |id: &str| tid.set(Some(id.to_string()))
                            })
                        />

                        <InputBar
                            lang=lang
                            preview=InputPreview::None
                            on_send=Box::new(on_send)
                            on_cancel_preview=Box::new({
                                let rt = reply_to.clone();
                                move || rt.set(None)
                            })
                        />
                    </div>
                }.into_any()
            }).unwrap_or_else(|| {
                view! {
                    <div class="hidden md:flex flex-1 flex-col items-center justify-center bg-muted/30">
                        <div class="flex h-16 w-16 items-center justify-center rounded-full bg-muted">
                            <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
                        </div>
                        <h2 class="mt-4 text-lg font-medium text-foreground">{t(lang.get(), "welcome.title")}</h2>
                        <p class="mt-1 text-sm text-muted-foreground">{t(lang.get(), "welcome.hint")}</p>
                    </div>
                }.into_any()
            })}

            {/* Thread panel */}
            <ThreadPanel
                lang=lang
                is_open=Signal::derive(move || thread_message_id.get().is_some())
                parent_message={thread_parent()}
                replies={mock::mock_thread_messages()}
                on_close=Box::new({
                    let tid = thread_message_id.clone();
                    move || tid.set(None)
                })
            />

            {/* Profile sheet */}
            {move || if show_profile.get() {
                selected_chat().map(|chat| {
                    view! {
                        <ProfileSheet
                            lang=lang
                            chat={chat.clone()}
                            on_close=Box::new({
                                let sp = show_profile.clone();
                                move || sp.set(false)
                            })
                        />
                    }
                })
            } else {
                None
            }}

            {/* New chat dialog */}
            <NewChatDialog
                lang=lang
                is_open=Signal::derive(move || show_new_chat.get())
                on_close=Box::new({
                    let nc = show_new_chat.clone();
                    move || nc.set(false)
                })
            />

            {/* Media viewer */}
            <MediaViewer
                is_open=Signal::derive(move || media_viewer_url.get().is_some())
                media_url={media_viewer_url.get()}
                on_close=Box::new({
                    let mv = media_viewer_url.clone();
                    move || mv.set(None)
                })
            />
        </div>
    }
}

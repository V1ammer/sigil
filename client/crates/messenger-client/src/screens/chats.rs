//! Chats screen — sidebar + main area with mobile-friendly navigation.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;
use uuid::Uuid;

use crate::i18n::I18n;
use crate::state::chats::ChatsState;
use crate::state::session::{build_api_client, use_session};
use crate::sidebar::real_chat_list::RealChatList;
use crate::t;

/// Helper: resolve display name for a group_id from the chats list.
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
    let selected = chats_state.selected;
    let chats_state_clone = chats_state.clone();
    let chats_signal = chats_state.chats;

    // Load chats from server on mount
    spawn_local(async move {
        let api = build_api_client();
        if let Some(api) = api {
            let _ = chats_state_clone.load_from_server(&api).await;
        }
    });

    let params = use_params_map();
    let chat_id_from_url = move || {
        params
            .get()
            .get("id")
            .map(|s| s.clone())
            .unwrap_or_default()
    };
    let url_id = chat_id_from_url();
    if !url_id.is_empty() {
        if let Ok(uid) = url_id.parse() {
            selected.set(Some(uid));
        }
    }

    let on_chat_select_arc = Arc::new({
        let cid = selected.clone();
        move |id: String| {
            if let Ok(uid) = id.parse() {
                cid.set(Some(uid));
            }
        }
    }) as Arc<dyn Fn(String) + Send + Sync + 'static>;

    // Back button handler — deselects chat on mobile
    let on_back = move |_| {
        selected.set(None);
    };

    view! {
        <div class="flex h-screen-safe bg-background overflow-hidden">
            {/* Sidebar — hidden on mobile when a chat is selected */}
            <div class=move || {
                let base = "flex-col border-r border-border bg-sidebar overflow-hidden";
                if selected.get().is_some() {
                    // On mobile: hide sidebar when chat is open
                    format!("hidden md:flex md:w-80 lg:w-96 {}", base)
                } else {
                    format!("flex w-full md:w-80 lg:w-96 {}", base)
                }
            }>
                <RealChatList on_chat_select=on_chat_select_arc />
            </div>

            {/* Main area — shown on mobile when chat is selected */}
            {move || selected.get().map(|group_id| {
                let name = display_name_for(&chats_signal.get(), group_id);
                view! {
                    <div class=move || {
                        if selected.get().is_some() {
                            "flex flex-1 flex-col w-full"
                        } else {
                            "hidden md:flex flex-1 flex-col"
                        }
                    }>
                        {/* Chat header with back button */}
                        <div class="flex items-center gap-3 border-b border-border px-4 py-3">
                            <button
                                class="md:hidden inline-flex h-9 w-9 items-center justify-center rounded-md hover:bg-accent transition-colors"
                                on:click=on_back
                                title={t!("back")}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/>
                                </svg>
                            </button>
                            <span class="text-sm font-medium text-foreground truncate">
                                {name.clone()}
                            </span>
                        </div>
                        <div class="flex-1 flex flex-col items-center justify-center p-8 text-center">
                            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground mb-4">
                                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                            </svg>
                            <h2 class="text-lg font-medium text-foreground">{t!("chat.mls.not_ready")}</h2>
                            <p class="mt-1 text-sm text-muted-foreground">{t!("chat.mls.hint")}</p>
                        </div>
                    </div>
                }.into_any()
            }).unwrap_or_else(|| {
                // Desktop placeholder when no chat is selected
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
        </div>
    }
}

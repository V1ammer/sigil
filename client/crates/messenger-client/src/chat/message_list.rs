//! Scrollable message list with date separators and grouped messages.
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use crate::i18n::{Language, t, format_date};
use crate::mock::Message;
use super::message_item::MessageItem;

/// Grouped messages — shows messages from the same sender within 5 minutes without
/// the avatar/name repeated.
const GROUP_TIME_WINDOW_MS: f64 = 5.0 * 60.0 * 1000.0;

/// Threshold (px) for "near bottom" — only auto-scroll if the user is
/// already close to the latest message, so they aren't yanked while
/// scrolled up reading history.
const AUTOSCROLL_THRESHOLD_PX: f64 = 120.0;

#[must_use]
#[component]
pub fn MessageList(
    #[prop(optional, into)] lang: Signal<Language>,
    messages: Vec<Message>,
    #[prop(optional, into)] is_mobile: Signal<bool>,
    #[prop(optional)] on_thread_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_media_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_avatar_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_reply: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_edit: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_reaction: Option<Box<dyn Fn(&str, String) + Send + Sync + 'static>>,
    #[prop(optional)] on_thread_open: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
) -> impl IntoView {
    let grouped = group_messages_with_dates(&messages, lang.get());

    let on_thread_click = on_thread_click.map(Arc::new);
    let on_thread_open = on_thread_open.map(Arc::new);
    let on_media_click = on_media_click.map(Arc::new);
    let on_avatar_click = on_avatar_click.map(Arc::new);
    let on_reply = on_reply.map(Arc::new);
    let on_edit = on_edit.map(Arc::new);
    let on_delete = on_delete.map(Arc::new);
    let on_reaction = on_reaction.map(Arc::new);

    let container_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let initial_jump_done = StoredValue::new(false);
    let messages_len = messages.len();

    // Jump straight to the latest message when the list first appears with
    // content. Subsequent updates (new incoming/outgoing) only auto-scroll if
    // the user is already near the bottom — otherwise we'd yank them out of
    // history they're reading.
    Effect::new(move |_| {
        let Some(el) = container_ref.get() else { return };
        let div: web_sys::HtmlElement = el.unchecked_into();
        let scroll_height = f64::from(div.scroll_height());
        let scroll_top = f64::from(div.scroll_top());
        let client_height = f64::from(div.client_height());
        let near_bottom = scroll_height - scroll_top - client_height < AUTOSCROLL_THRESHOLD_PX;

        let first_time = !initial_jump_done.get_value();
        if first_time && messages_len > 0 {
            initial_jump_done.set_value(true);
        }
        if first_time || near_bottom {
            div.set_scroll_top(div.scroll_height());
        }
    });

    view! {
        <div node_ref=container_ref class="flex-1 overflow-y-auto px-4 py-3 space-y-1" id="message-list">
            {grouped.into_iter().map(|group| {
                match group {
                    MessageGroup::DateSeparator(date_str) => {
                        view! {
                            <div class="flex items-center justify-center py-2">
                                <span class="rounded-full bg-muted px-3 py-1 text-xs text-muted-foreground">
                                    {date_str}
                                </span>
                            </div>
                        }.into_any()
                    }
                    MessageGroup::Messages(msg_batch) => {
                        let len = msg_batch.len();
                        let on_thread = on_thread_click.clone();
                        let on_thread_open_fn = on_thread_open.clone();
                        let on_media = on_media_click.clone();
                        let on_avatar = on_avatar_click.clone();
                        let on_reply = on_reply.clone();
                        let on_edit = on_edit.clone();
                        let on_delete = on_delete.clone();
                        let on_reaction = on_reaction.clone();
                        let items = msg_batch.into_iter().enumerate().map(move |(idx, msg)| {
                            let is_first = idx == 0;
                            let is_last = idx == len - 1;
                            let msg_id = msg.id.clone();
                            let msg_sender_id = msg.sender_id.clone();
                            view! {
                                <MessageItem
                                    lang=lang
                                    message=msg
                                    is_first_in_group=is_first
                                    is_last_in_group=is_last
                                    is_mobile=is_mobile
                                    on_thread_click={
                                        let id = msg_id.clone();
                                        let ot = on_thread.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = ot { f(&id); }
                                        })
                                    }
                                    on_media_click={
                                        let id = msg_id.clone();
                                        let om = on_media.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = om { f(&id); }
                                        })
                                    }
                                    on_avatar_click={
                                        let sender_id = msg_sender_id.clone();
                                        let oa = on_avatar.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = oa { f(&sender_id); }
                                        })
                                    }
                                    on_reply={
                                        let id = msg_id.clone();
                                        let r = on_reply.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = r { f(&id); }
                                        })
                                    }
                                    on_edit={
                                        let id = msg_id.clone();
                                        let e = on_edit.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = e { f(&id); }
                                        })
                                    }
                                    on_delete={
                                        let id = msg_id.clone();
                                        let d = on_delete.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = d { f(&id); }
                                        })
                                    }
                                    on_reaction={
                                        let id = msg_id.clone();
                                        let r = on_reaction.clone();
                                        Box::new(move |emoji: String| {
                                            if let Some(ref f) = r { f(&id, emoji); }
                                        })
                                    }
                                    on_thread_open={
                                        let id = msg_id.clone();
                                        let t = on_thread_open_fn.clone();
                                        Box::new(move || {
                                            if let Some(ref f) = t { f(&id); }
                                        })
                                    }
                                />
                            }
                        }).collect::<Vec<_>>();
                        view! { {items} }.into_any()
                    }
                }
            }).collect::<Vec<AnyView>>()}
        </div>
    }
}

enum MessageGroup {
    DateSeparator(String),
    Messages(Vec<Message>),
}

fn group_messages_with_dates(messages: &[Message], lang: Language) -> Vec<MessageGroup> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        // Date separator for the current message
        let date_str = format_date(messages[i].timestamp, lang);
        if let Some(prev) = result.last() {
            match prev {
                MessageGroup::DateSeparator(d) if d == &date_str => { /* skip duplicate */ }
                _ => {
                    result.push(MessageGroup::DateSeparator(date_str.clone()));
                }
            }
        } else {
            result.push(MessageGroup::DateSeparator(date_str));
        }

        // Collect batch — consecutive messages from same sender within time window
        let mut batch = Vec::new();
        batch.push(messages[i].clone());
        let first_sender = messages[i].sender_id.clone();
        let first_time = messages[i].timestamp;

        i += 1;
        while i < messages.len() {
            let next_date = format_date(messages[i].timestamp, lang);
            let prev_msg = &messages[i - 1];
            let next_msg = &messages[i];

            // System messages always break grouping
            if next_msg.msg_type == "system" {
                break;
            }
            // Different sender breaks grouping
            if next_msg.sender_id != first_sender {
                break;
            }
            // Time gap > 5 minutes breaks grouping
            if next_msg.timestamp - prev_msg.timestamp > GROUP_TIME_WINDOW_MS {
                break;
            }
            // Date change breaks grouping
            let current_date = format_date(next_msg.timestamp, lang);
            if current_date != next_date {
                break;
            }
            batch.push(next_msg.clone());
            i += 1;
        }
        result.push(MessageGroup::Messages(batch));
    }

    result
}

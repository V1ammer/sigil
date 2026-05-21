//! Scrollable message list with date separators and grouped messages.
use leptos::prelude::*;
use std::sync::Arc;
use crate::i18n::{Language, t, format_date};
use crate::mock::Message;
use super::message_item::MessageItem;

/// Grouped messages — shows messages from the same sender within 5 minutes without
/// the avatar/name repeated.
const GROUP_TIME_WINDOW_MS: f64 = 5.0 * 60.0 * 1000.0;

#[must_use]
#[component]
pub fn MessageList(
    #[prop(optional, into)] lang: Signal<Language>,
    messages: Vec<Message>,
    #[prop(optional, into)] is_mobile: Signal<bool>,
    #[prop(optional)] on_thread_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_media_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_avatar_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
) -> impl IntoView {
    let grouped = group_messages_with_dates(&messages);

    let on_thread_click = on_thread_click.map(Arc::new);
    let on_media_click = on_media_click.map(Arc::new);
    let on_avatar_click = on_avatar_click.map(Arc::new);

    view! {
        <div class="flex-1 overflow-y-auto px-4 py-3 space-y-1" id="message-list">
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
                        let on_media = on_media_click.clone();
                        let on_avatar = on_avatar_click.clone();
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

fn group_messages_with_dates(messages: &[Message]) -> Vec<MessageGroup> {
    let mut result = Vec::new();
    let mut i = 0;
    let lang = Language::Ru; // placeholder, used for date formatting

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

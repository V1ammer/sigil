//! Scrollable message list with date separators and grouped messages.
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use crate::i18n::{Language, format_date};
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
    #[prop(into)] messages: Signal<Vec<Message>>,
    #[prop(optional, into)] is_mobile: Signal<bool>,
    #[prop(optional)] on_thread_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_media_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_avatar_click: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_reply: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_edit: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_forward: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
    #[prop(optional)] on_reaction: Option<Box<dyn Fn(&str, String) + Send + Sync + 'static>>,
    #[prop(optional)] on_thread_open: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
) -> impl IntoView {
    let on_thread_click = on_thread_click.map(Arc::new);
    let on_thread_open = on_thread_open.map(Arc::new);
    let on_media_click = on_media_click.map(Arc::new);
    let on_avatar_click = on_avatar_click.map(Arc::new);
    let on_reply = on_reply.map(Arc::new);
    let on_edit = on_edit.map(Arc::new);
    let on_delete = on_delete.map(Arc::new);
    let on_forward = on_forward.map(Arc::new);
    let on_reaction = on_reaction.map(Arc::new);

    let container_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let initial_jump_done = StoredValue::new(false);
    // Whether the user is parked at the bottom (so we keep them pinned as
    // late-loading content — images — grows the list).
    let stick_to_bottom = StoredValue::new(true);
    let listeners_attached = StoredValue::new(false);

    // Reactive, keyed rows. The key is (id, content-version): a NEW or unrelated
    // message doesn't change an existing row's key, so its DOM — including any
    // playing <video>/<audio> — is preserved across incoming/outgoing messages.
    // Only a row whose own content/status/reactions changed gets a fresh key and
    // re-renders.
    let enriched = Signal::derive(move || enrich_messages(&messages.get(), lang.get()));

    // Jump straight to the latest message when the list first appears with
    // content. Subsequent updates (new incoming/outgoing) only auto-scroll if
    // the user is already near the bottom — otherwise we'd yank them out of
    // history they're reading.
    Effect::new(move |_| {
        // Track the message count so the effect re-runs (and keeps us pinned to
        // the bottom) as new messages arrive.
        let messages_len = messages.get().len();
        let Some(el) = container_ref.get() else { return };
        let div: web_sys::HtmlElement = el.unchecked_into();

        // Attach scroll/load listeners once per list instance.
        if !listeners_attached.get_value() {
            listeners_attached.set_value(true);

            // Keep `stick_to_bottom` in sync with the user's scroll position.
            {
                let d = div.clone();
                let on_scroll = Closure::<dyn FnMut()>::new(move || {
                    let nb = f64::from(d.scroll_height())
                        - f64::from(d.scroll_top())
                        - f64::from(d.client_height())
                        < AUTOSCROLL_THRESHOLD_PX;
                    stick_to_bottom.set_value(nb);
                });
                let _ = div.add_event_listener_with_callback(
                    "scroll",
                    on_scroll.as_ref().unchecked_ref(),
                );
                on_scroll.forget();
            }

            // When an image finishes loading it grows the list, pushing the
            // bottom further down. If the user is parked at the bottom, follow
            // it so they stay pinned instead of being left above the newest
            // message. `load` doesn't bubble, so listen in the capture phase.
            {
                let d = div.clone();
                let on_load = Closure::<dyn FnMut()>::new(move || {
                    if stick_to_bottom.get_value() {
                        d.set_scroll_top(d.scroll_height());
                    }
                });
                let _ = div.add_event_listener_with_callback_and_bool(
                    "load",
                    on_load.as_ref().unchecked_ref(),
                    true,
                );
                on_load.forget();
            }
        }

        let scroll_height = f64::from(div.scroll_height());
        let scroll_top = f64::from(div.scroll_top());
        let client_height = f64::from(div.client_height());
        let near_bottom = scroll_height - scroll_top - client_height < AUTOSCROLL_THRESHOLD_PX;

        let first_time = !initial_jump_done.get_value();
        if first_time && messages_len > 0 {
            initial_jump_done.set_value(true);
        }
        if first_time || near_bottom {
            stick_to_bottom.set_value(true);
            div.set_scroll_top(div.scroll_height());
            // Defer one more jump to after layout settles. On first open the
            // effect can fire before children have their final height, which
            // would otherwise leave us short of the bottom (near the top).
            let div2 = div.clone();
            let cb = Closure::<dyn FnMut()>::new(move || {
                div2.set_scroll_top(div2.scroll_height());
            });
            if let Some(w) = web_sys::window() {
                let _ = w.request_animation_frame(cb.as_ref().unchecked_ref());
            }
            cb.forget();
        }
    });

    view! {
        <div node_ref=container_ref class="flex-1 overflow-y-auto px-4 py-3 space-y-1" id="message-list">
            <For
                each=move || enriched.get()
                key=|row| row.key.clone()
                children=move |row| {
                    let on_thread = on_thread_click.clone();
                    let on_thread_open_fn = on_thread_open.clone();
                    let on_media = on_media_click.clone();
                    let on_avatar = on_avatar_click.clone();
                    let on_reply = on_reply.clone();
                    let on_edit = on_edit.clone();
                    let on_delete = on_delete.clone();
                    let on_forward = on_forward.clone();
                    let on_reaction = on_reaction.clone();
                    let Row { separator, is_first, is_last, msg, .. } = row;
                    let msg_id = msg.id.clone();
                    let msg_sender_id = msg.sender_id.clone();
                    view! {
                        {separator.map(|date_str| view! {
                            <div class="flex items-center justify-center py-2">
                                <span class="rounded-full bg-muted px-3 py-1 text-xs text-muted-foreground">
                                    {date_str}
                                </span>
                            </div>
                        })}
                        <MessageItem
                            lang=lang
                            message=msg
                            is_first_in_group=is_first
                            is_last_in_group=is_last
                            is_mobile=is_mobile
                            on_thread_click={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_thread { f(&id); } })
                            }
                            on_media_click={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_media { f(&id); } })
                            }
                            on_avatar_click={
                                let sid = msg_sender_id.clone();
                                Box::new(move || { if let Some(ref f) = on_avatar { f(&sid); } })
                            }
                            on_reply={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_reply { f(&id); } })
                            }
                            on_edit={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_edit { f(&id); } })
                            }
                            on_delete={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_delete { f(&id); } })
                            }
                            on_forward={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_forward { f(&id); } })
                            }
                            on_reaction={
                                let id = msg_id.clone();
                                Box::new(move |emoji: String| { if let Some(ref f) = on_reaction { f(&id, emoji); } })
                            }
                            on_thread_open={
                                let id = msg_id.clone();
                                Box::new(move || { if let Some(ref f) = on_thread_open_fn { f(&id); } })
                            }
                        />
                    }
                }
            />
        </div>
    }
}

/// One renderable row: an optional date separator followed by a message, plus
/// grouping flags. `key` ties the row's DOM identity to its content so playing
/// media survives unrelated list changes (see `MessageList`).
#[derive(Clone)]
struct Row {
    key: String,
    separator: Option<String>,
    is_first: bool,
    is_last: bool,
    msg: Message,
}

/// Hash of the fields whose change should re-render a row. Deliberately excludes
/// grouping flags (is_first/is_last) and the attachment id/key, so a new
/// neighbour or an unrelated message never re-creates this row's media element.
fn msg_version(m: &Message) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    m.content.hash(&mut h);
    m.status.hash(&mut h);
    m.is_edited.hash(&mut h);
    m.is_deleted.hash(&mut h);
    m.thread_count.hash(&mut h);
    for r in &m.reactions {
        r.emoji.hash(&mut h);
        r.count.hash(&mut h);
        r.has_own.hash(&mut h);
    }
    h.finish()
}

/// Flatten messages into keyed rows: one date separator per day, with
/// per-message group-boundary flags computed from neighbours.
fn enrich_messages(messages: &[Message], lang: Language) -> Vec<Row> {
    let mut rows = Vec::with_capacity(messages.len());
    let mut last_date: Option<String> = None;
    for i in 0..messages.len() {
        let m = &messages[i];
        let date = format_date(m.timestamp, lang);
        let separator = if last_date.as_deref() != Some(date.as_str()) {
            last_date = Some(date.clone());
            Some(date.clone())
        } else {
            None
        };
        let is_first = separator.is_some()
            || m.msg_type == "system"
            || match messages.get(i.wrapping_sub(1)).filter(|_| i > 0) {
                None => true,
                Some(p) => {
                    p.msg_type == "system"
                        || p.sender_id != m.sender_id
                        || (m.timestamp - p.timestamp > GROUP_TIME_WINDOW_MS)
                }
            };
        let is_last = match messages.get(i + 1) {
            None => true,
            Some(n) => {
                n.msg_type == "system"
                    || n.sender_id != m.sender_id
                    || (n.timestamp - m.timestamp > GROUP_TIME_WINDOW_MS)
                    || format_date(n.timestamp, lang) != date
            }
        };
        rows.push(Row {
            key: format!("{}-{:x}", m.id, msg_version(m)),
            separator,
            is_first,
            is_last,
            msg: m.clone(),
        });
    }
    rows
}

//! Thread panel — shows replies to a specific message in a slide-over panel.
use std::sync::Arc;
use leptos::prelude::*;
use crate::i18n::{Language, t, format_time};
use crate::mock::Message;
use crate::icons::Icon;
use crate::components::sheet::{Sheet, SheetHeader, SheetTitle};
use crate::components::scroll_area::ScrollArea;
use crate::components::avatar::get_initials;
use super::input_bar::{InputBar, InputPreview};
use super::message_item::MessageItem;

#[must_use]
#[component]
pub fn ThreadPanel(
    #[prop(optional, into)] lang: Signal<Language>,
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    parent_message: Option<Message>,
    #[prop(optional, into)] replies: Vec<Message>,
    #[prop(optional)] on_send_reply: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_media_click: Option<Box<dyn Fn(String) + 'static>>,
) -> impl IntoView {
    let close_cb = on_close.unwrap_or_else(|| Box::new(|| {}));

    let msg = parent_message;

    // Wrap in Arc so the FnOnce closure can clone it
    let on_send_reply = Arc::new(on_send_reply);

    // Store replies in a signal so closures can be Fn (not FnOnce)
    let replies_signal = RwSignal::new(replies);

    view! {
        <Sheet
            is_open=is_open
            on_close=close_cb
            side="right".to_string()
            class="flex flex-col"
        >
            <SheetHeader>
                <SheetTitle>
                    <span class="flex items-center gap-2">
                        <Icon name="message-square" class_name="h-5 w-5"/>
                        "Thread"
                    </span>
                </SheetTitle>
            </SheetHeader>

            // Parent message preview
            {if let Some(ref parent) = msg {
                view! {
                    <div class="px-4 py-3 border-b border-border bg-muted/20">
                        <div class="flex items-start gap-3">
                            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-foreground">
                                {get_initials(&parent.sender_name)}
                            </div>
                            <div class="min-w-0 flex-1">
                                <div class="flex items-center gap-2">
                                    <span class="text-sm font-medium">{parent.sender_name.clone()}</span>
                                    <span class="text-[11px] text-muted-foreground">{format_time(parent.timestamp, lang.get())}</span>
                                </div>
                                <p class="mt-0.5 text-sm text-foreground/90">{parent.content.clone()}</p>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}

            // Replies list
            <ScrollArea class="flex-1 px-4 py-3">
                <div class="space-y-3">
                    {move || {
                        let replies = replies_signal.get();
                        if replies.is_empty() {
                            view! {
                                <p class="text-sm text-muted-foreground text-center py-8">
                                    {t(lang.get(), "chats.empty")}
                                </p>
                            }.into_any()
                        } else {
                            view! {
                                <For
                                    each=move || replies_signal.get()
                                    key=|msg| msg.id.clone()
                                    children=move |msg| {
                                        view! {
                                            <MessageItem
                                                lang=lang
                                                message=msg
                                                is_first_in_group=true
                                                is_last_in_group=true
                                                is_mobile=Signal::derive(move || false)
                                            />
                                        }
                                    }
                                />
                            }.into_any()
                        }
                    }}
                </div>
            </ScrollArea>

            // Thread input bar
            <div class="border-t border-border">
                <InputBar
                    locale=lang
                    preview=InputPreview::None
                    on_send=Box::new({
                        let osr = on_send_reply.clone();
                        move |text| {
                            if let Some(ref f) = *osr { f(text); }
                        }
                    })
                />
            </div>
        </Sheet>
    }
}

//! Thread panel — shows replies to a specific message in a slide-over panel.
use std::sync::Arc;
use leptos::prelude::*;
use crate::i18n::{Language, t, format_time};
use crate::mock::Message;
use crate::icons::Icon;
use crate::components::sheet::{Sheet, SheetHeader, SheetTitle};
use crate::components::scroll_area::ScrollArea;
use crate::components::avatar::get_initials;
use super::input_bar::{AttachmentPayload, InputBar, InputPreview, VoicePayload};
use super::message_item::MessageItem;

#[must_use]
#[component]
pub fn ThreadPanel(
    #[prop(optional, into)] lang: Signal<Language>,
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    // Reactive so the panel (and its composer) is NOT rebuilt every time a reply
    // arrives or the periodic sync re-renders — which used to wipe the input.
    #[prop(optional, into)] parent_message: Signal<Option<Message>>,
    #[prop(optional, into)] replies: Signal<Vec<Message>>,
    #[prop(optional)] on_send_reply: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_send_attachment: Option<Box<dyn Fn(AttachmentPayload) + Send + Sync + 'static>>,
    #[prop(optional)] on_send_voice: Option<Box<dyn Fn(VoicePayload) + Send + Sync + 'static>>,
) -> impl IntoView {
    let close_cb = on_close.unwrap_or_else(|| Box::new(|| {}));

    let on_send_reply = Arc::new(on_send_reply);
    let on_send_attachment = Arc::new(on_send_attachment);
    let on_send_voice = Arc::new(on_send_voice);

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

            // Parent message preview (reactive).
            {move || parent_message.get().map(|parent| view! {
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
            })}

            // Replies list (reactive — updates without rebuilding the composer).
            <ScrollArea class="flex-1 px-4 py-3">
                <div class="space-y-3">
                    {move || {
                        if replies.get().is_empty() {
                            view! {
                                <p class="text-sm text-muted-foreground text-center py-8">
                                    {t(lang.get(), "chats.empty")}
                                </p>
                            }.into_any()
                        } else {
                            view! {
                                <For
                                    each=move || replies.get()
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

            // Thread composer — same capabilities as the main chat: text, photo/
            // file attachments and voice.
            <div class="border-t border-border">
                <InputBar
                    locale=lang
                    preview=InputPreview::None
                    on_send=Box::new({
                        let osr = on_send_reply.clone();
                        move |text| { if let Some(ref f) = *osr { f(text); } }
                    })
                    on_send_attachment=Box::new({
                        let osa = on_send_attachment.clone();
                        move |payload| { if let Some(ref f) = *osa { f(payload); } }
                    })
                    on_send_voice=Box::new({
                        let osv = on_send_voice.clone();
                        move |payload| { if let Some(ref f) = *osv { f(payload); } }
                    })
                />
            </div>
        </Sheet>
    }
}

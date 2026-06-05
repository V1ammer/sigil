//! Full-screen media viewer — shows images/videos in an overlay with zoom, pan, and
//! navigation controls.
use leptos::prelude::*;
use crate::i18n::Language;
use crate::icons::Icon;
use crate::components::button::{Button, ButtonVariant, ButtonSize};

#[must_use]
#[component]
pub fn MediaViewer(
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    media_url: Option<String>,
    #[prop(optional, into)] media_type: String,
    #[prop(optional, into)] caption: Option<String>,
    #[prop(optional)] on_download: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_next: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional)] on_prev: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let is_zoomed = RwSignal::new(false);
    let is_image = media_type == "image";

    let toggle_zoom = move |_| {
        is_zoomed.set(!is_zoomed.get());
    };

    // Wrap callbacks in Arc for sharing across closures
    let on_close = std::sync::Arc::new(on_close);
    let on_download = std::sync::Arc::new(on_download);
    let on_prev = std::sync::Arc::new(on_prev);
    let on_next = std::sync::Arc::new(on_next);

    view! {
        <Show when=move || is_open.get()>
            <div class="fixed inset-0 z-50 flex flex-col bg-black/95">
                // Top bar
                <div class="flex items-center justify-between px-4 py-3 shrink-0">
                    <button
                        class="flex h-10 w-10 items-center justify-center rounded-full text-white/80 hover:bg-white/10 transition-colors"
                        on:click={
                        let oc = on_close.clone();
                        move |_| { if let Some(f) = oc.as_ref() { f(); } }
                    }
                    >
                        <Icon name="x" class_name="h-6 w-6"/>
                    </button>

                    <div class="flex items-center gap-2">
                        {if is_image {
                            view! {
                                <button
                                    class="flex h-10 w-10 items-center justify-center rounded-full text-white/80 hover:bg-white/10 transition-colors"
                                    on:click=toggle_zoom
                                >
                                    <Icon name="search" class_name="h-5 w-5"/>
                                </button>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                        <button
                            class="flex h-10 w-10 items-center justify-center rounded-full text-white/80 hover:bg-white/10 transition-colors"
                            on:click={
                                let od = on_download.clone();
                                move |_| { if let Some(f) = od.as_ref() { f(); } }
                            }
                        >
                            <Icon name="download" class_name="h-5 w-5"/>
                        </button>
                    </div>
                </div>

                // Media content area
                <div class="flex-1 flex items-center justify-center relative overflow-hidden">
                    // Previous button
                    {if on_prev.is_some() {
                        view! {
                            <button
                                class="absolute left-4 z-10 flex h-12 w-12 items-center justify-center rounded-full bg-black/30 text-white/80 hover:bg-black/50 transition-colors"
                                on:click={
                                    let op = on_prev.clone();
                                    move |_| { if let Some(f) = op.as_ref() { f(); } }
                                }
                            >
                                <Icon name="chevron-left" class_name="h-6 w-6"/>
                            </button>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }}

                    // Media display
                    <div class="flex items-center justify-center w-full h-full px-16">
                        {if is_image {
                            view! {
                                <img
                                    src=media_url.clone().unwrap_or_default()
                                    alt="Media"
                                    class=move || format!(
                                        "max-h-full max-w-full object-contain transition-transform duration-200 ease-in-out {}",
                                        if is_zoomed.get() { "scale-150 cursor-zoom-out" } else { "cursor-zoom-in" }
                                    )
                                    on:click=toggle_zoom
                                />
                            }.into_any()
                        } else {
                            view! {
                                {if let Some(ref url) = media_url {
                                    view! {
                                        <video
                                            src=url
                                            class="max-h-[80vh] max-w-full rounded-lg"
                                            controls=true
                                            autoplay=false
                                        />
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="flex flex-col items-center gap-2 text-white/60">
                                            <Icon name="film" class_name="h-16 w-16"/>
                                            <p class="text-sm">"Video"</p>
                                        </div>
                                    }.into_any()
                                }}
                            }.into_any()
                        }}
                    </div>

                    // Next button
                    {if on_next.is_some() {
                        view! {
                            <button
                                class="absolute right-4 z-10 flex h-12 w-12 items-center justify-center rounded-full bg-black/30 text-white/80 hover:bg-black/50 transition-colors"
                                on:click={
                                    let on = on_next.clone();
                                    move |_| { if let Some(f) = on.as_ref() { f(); } }
                                }
                            >
                                <Icon name="chevron-right" class_name="h-6 w-6"/>
                            </button>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }}
                </div>

                // Bottom caption bar
                {if let Some(ref caption_text) = caption {
                    view! {
                        <div class="shrink-0 px-6 py-4 text-center">
                            <p class="text-sm text-white/70">{caption_text.clone()}</p>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }}
            </div>
        </Show>
    }
}

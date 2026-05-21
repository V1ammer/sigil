//! Voice message player — waveform visualization, play/pause, duration, transcription.
use leptos::prelude::*;
use crate::i18n::format_duration;
use crate::icons::Icon;
use crate::components::slider::Slider;

#[must_use]
#[component]
pub fn VoiceMessage(
    #[prop(optional, into)] duration: u32,
    #[prop(optional, into)] waveform: Vec<f64>,
    #[prop(optional)] transcription: Option<String>,
    #[prop(optional, into)] is_own: bool,
) -> impl IntoView {
    let is_playing = RwSignal::new(false);
    let current_position = RwSignal::new(0u32);
    let show_transcription = RwSignal::new(false);

    let playback_text = move || {
        format!("{} / {}", format_duration(current_position.get()), format_duration(duration))
    };

    let progress_pct = move || {
        if duration == 0 {
            0.0
        } else {
            current_position.get() as f64 / duration as f64 * 100.0
        }
    };

    let toggle_play = move |_| {
        is_playing.set(!is_playing.get());
    };

    let accent = if is_own { "bg-primary" } else { "bg-foreground" };
    let muted_accent = if is_own { "bg-primary/30" } else { "bg-foreground/30" };

    view! {
        <div class="flex flex-col gap-1 min-w-[200px] max-w-full">
            <div class="flex items-center gap-2">
                // Play/Pause button
                <button
                    class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full hover:bg-accent/50 transition-colors"
                    on:click=toggle_play
                >
                    {move || if is_playing.get() {
                        view! { <Icon name="pause" class_name="h-4 w-4"/> }.into_any()
                    } else {
                        view! { <Icon name="play" class_name="h-4 w-4"/> }.into_any()
                    }}
                </button>

                // Waveform visualization
                <div class="flex-1 flex items-center h-8 gap-px">
                    {if waveform.is_empty() {
                        // Render flat waveform placeholder
                        (0..30).map(|_| {
                            view! {
                                <div class="flex-1 h-2 self-center rounded-full bg-muted-foreground/20"/>
                            }.into_any()
                        }).collect::<Vec<AnyView>>()
                    } else {
                        waveform.iter().map(|bar| {
                            let height_pct = (bar * 100.0).clamp(5.0, 100.0);
                            let style = format!("height:{}%", height_pct);
                            // Played portion gets accent color
                            let bar_class = move || {
                                if is_own {
                                    if is_playing.get() { "flex-1 rounded-full transition-colors bg-primary" }
                                    else { "flex-1 rounded-full transition-colors bg-primary/30" }
                                } else {
                                    if is_playing.get() { "flex-1 rounded-full transition-colors bg-foreground" }
                                    else { "flex-1 rounded-full transition-colors bg-foreground/30" }
                                }
                            };
                            view! {
                                <div
                                    class=bar_class
                                    style=style
                                />
                            }.into_any()
                        }).collect::<Vec<AnyView>>()
                    }}
                </div>

                // Duration
                <span class="text-[11px] tabular-nums shrink-0 opacity-70 min-w-[3rem] text-right">
                    {playback_text}
                </span>
            </div>

            // Playback slider
            <Slider
                value=current_position
                on_change=Box::new(move |v: u32| current_position.set(v))
                max=duration
            />

            // Transcription toggle
            {if let Some(text) = transcription.clone() {
                view! {
                    <div class="mt-1">
                        <button
                            class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                            on:click=move |_| show_transcription.set(!show_transcription.get())
                        >
                            <Icon name="file-text" class_name="h-3 w-3"/>
                            {move || if show_transcription.get() { "Hide" } else { "Transcription" }}
                        </button>
                        <Show when=move || show_transcription.get()>
                            <p class="mt-1 text-xs text-muted-foreground/80 leading-relaxed whitespace-pre-wrap">
                                {text.clone()}
                            </p>
                        </Show>
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
        </div>
    }
}

//! Voice message player — waveform visualization, play/pause, duration, transcription.
//!
//! On first play, downloads the encrypted blob from the server, decrypts in-memory,
//! and feeds it to an `<audio>` element via an object URL.
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use crate::i18n::format_duration;
use crate::icons::Icon;
use crate::state::session::build_api_client;

#[must_use]
#[component]
pub fn VoiceMessage(
    #[prop(optional, into)] duration_ms: u32,
    #[prop(optional, into)] waveform: Vec<f64>,
    #[prop(optional)] transcription: Option<String>,
    #[prop(optional, into)] is_own: bool,
    /// Server attachment ID — required to actually play audio.
    #[prop(optional, into)] attachment_id: Option<String>,
    /// Base64-encoded 32-byte decryption key.
    #[prop(optional, into)] decryption_key: Option<String>,
    /// MIME type of the encoded audio (e.g. `audio/webm;codecs=opus`).
    #[prop(optional, into)] mime: Option<String>,
) -> impl IntoView {
    let is_playing = RwSignal::new(false);
    let current_position_ms = RwSignal::new(0u32);
    let show_transcription = RwSignal::new(false);
    let blob_url: RwSignal<Option<String>> = RwSignal::new(None);
    let loading = RwSignal::new(false);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    let audio_ref: NodeRef<leptos::html::Audio> = NodeRef::new();
    let duration_total = duration_ms.max(1);

    let playback_text = move || {
        format!(
            "{} / {}",
            format_duration(current_position_ms.get() / 1000),
            format_duration(duration_total / 1000),
        )
    };

    let attachment_id_for_load = attachment_id.clone();
    let decryption_key_for_load = decryption_key.clone();
    let mime_for_load = mime.clone();

    // Lazy-load: fetch + decrypt on first play. Returns Some(url) when ready.
    let load_audio = move || {
        if blob_url.get_untracked().is_some() {
            return;
        }
        let aid = match attachment_id_for_load.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };
        let key_b64 = match decryption_key_for_load.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };
        let mime = mime_for_load.clone().unwrap_or_else(|| "audio/webm".to_string());
        loading.set(true);
        spawn_local(async move {
            use base64::Engine as _;
            let api = match build_api_client() {
                Some(a) => a,
                None => {
                    error.set(Some("no api client".into()));
                    loading.set(false);
                    return;
                }
            };
            let attachment_id = match aid.parse::<uuid::Uuid>() {
                Ok(u) => u,
                Err(_) => {
                    error.set(Some("bad attachment id".into()));
                    loading.set(false);
                    return;
                }
            };
            let ct = match api.download_attachment(attachment_id, None).await {
                Ok(b) => b,
                Err(e) => {
                    error.set(Some(format!("download: {e}")));
                    loading.set(false);
                    return;
                }
            };
            let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&key_b64) {
                Ok(b) if b.len() == 32 => b,
                _ => {
                    error.set(Some("bad key".into()));
                    loading.set(false);
                    return;
                }
            };
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_bytes);
            let plain = match messenger_core::attachment_crypto::decrypt_attachment(&key, &ct) {
                Ok(p) => p,
                Err(e) => {
                    error.set(Some(format!("decrypt: {e:?}")));
                    loading.set(false);
                    return;
                }
            };
            // Wrap in Blob → object URL.
            let u8a = js_sys::Uint8Array::from(plain.as_slice());
            let arr = js_sys::Array::new();
            arr.push(&u8a.into());
            let mut bag = web_sys::BlobPropertyBag::new();
            bag.type_(&mime);
            let blob = match web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &bag) {
                Ok(b) => b,
                Err(_) => {
                    error.set(Some("blob create failed".into()));
                    loading.set(false);
                    return;
                }
            };
            let url = match web_sys::Url::create_object_url_with_blob(&blob) {
                Ok(u) => u,
                Err(_) => {
                    error.set(Some("object url failed".into()));
                    loading.set(false);
                    return;
                }
            };
            blob_url.set(Some(url));
            loading.set(false);
        });
    };

    let toggle_play = move |_| {
        if blob_url.get_untracked().is_none() {
            load_audio();
            // Auto-play once the URL is set via the Effect below.
            is_playing.set(true);
            return;
        }
        if let Some(el) = audio_ref.get_untracked() {
            let audio: &web_sys::HtmlAudioElement = el.unchecked_ref();
            if is_playing.get_untracked() {
                let _ = audio.pause();
                is_playing.set(false);
            } else {
                let _ = audio.play();
                is_playing.set(true);
            }
        }
    };

    // When blob_url becomes Some after we already wanted to play, start playback.
    Effect::new(move |_| {
        if blob_url.get().is_some() && is_playing.get_untracked() {
            if let Some(el) = audio_ref.get_untracked() {
                let audio: &web_sys::HtmlAudioElement = el.unchecked_ref();
                let _ = audio.play();
            }
        }
    });

    // Drive the position counter from the audio element's timeupdate event.
    let on_time_update = move |ev: leptos::ev::Event| {
        if let Some(target) = ev.target() {
            let audio: web_sys::HtmlAudioElement = target.unchecked_into();
            current_position_ms.set((audio.current_time() * 1000.0) as u32);
        }
    };
    let on_ended = move |_: leptos::ev::Event| {
        is_playing.set(false);
        current_position_ms.set(0);
    };

    view! {
        <div class="flex flex-col gap-1 min-w-[220px] max-w-full">
            <div class="flex items-center gap-2">
                <button
                    class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full hover:bg-accent/50 active:bg-accent/70 transition-colors disabled:opacity-50"
                    on:click=toggle_play
                    disabled=move || loading.get()
                >
                    {move || if loading.get() {
                        view! { <span class="block h-4 w-4 rounded-full border-2 border-current border-t-transparent animate-spin"/> }.into_any()
                    } else if is_playing.get() {
                        view! { <Icon name="pause" class_name="h-4 w-4"/> }.into_any()
                    } else {
                        view! { <Icon name="play" class_name="h-4 w-4"/> }.into_any()
                    }}
                </button>

                // Waveform — bars become accented as playback progresses.
                <div class="flex-1 flex items-center h-9 gap-px">
                    {
                        let bars: Vec<f64> = if waveform.is_empty() {
                            (0..30).map(|i| 0.3 + 0.3 * (i as f64 % 4.0 / 4.0)).collect()
                        } else {
                            waveform.clone()
                        };
                        let n = bars.len() as f64;
                        bars.into_iter().enumerate().map(|(i, bar)| {
                            let height_pct = (bar * 100.0).clamp(8.0, 100.0);
                            let progress_threshold = (i as f64 + 1.0) / n;
                            let cls = move || {
                                let played = (current_position_ms.get() as f64 / duration_total as f64) >= progress_threshold;
                                match (is_own, played) {
                                    (true, true)  => "flex-1 rounded-full bg-primary-foreground",
                                    (true, false) => "flex-1 rounded-full bg-primary-foreground/40",
                                    (false, true) => "flex-1 rounded-full bg-foreground",
                                    (false, false) => "flex-1 rounded-full bg-foreground/40",
                                }
                            };
                            view! { <div class=cls style=format!("height:{height_pct}%")/> }
                        }).collect::<Vec<_>>()
                    }
                </div>

                <span class="text-[11px] tabular-nums shrink-0 opacity-70 min-w-[3rem] text-right">
                    {playback_text}
                </span>
            </div>

            // Hidden audio element — wired only after blob URL is available.
            {move || {
                blob_url.get().map(|url| {
                    view! {
                        <audio
                            node_ref=audio_ref
                            src=url
                            preload="auto"
                            on:timeupdate=on_time_update
                            on:ended=on_ended
                        />
                    }
                })
            }}

            {move || {
                error.get().map(|e| view! {
                    <span class="text-[10px] text-destructive">{e}</span>
                })
            }}

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

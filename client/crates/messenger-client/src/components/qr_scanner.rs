//! QR Scanner component.
//!
//! ## Pragmatic approach (per C08 spec)
//!
//! 1. **Manual input** (always works) — a text input for pasting QR content.
//! 2. **BarcodeDetector** (optional, Chromium-based browsers) — uses the
//!    built-in `BarcodeDetector` API via `web-sys` when available.
//!
//! The BarcodeDetector path requires `wasm-bindgen-futures` at runtime
//! (already available transitively via Leptos in CSR mode).

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::components::button::{Button, ButtonVariant};
use crate::i18n::I18n;
use crate::t;

/// Check whether BarcodeDetector is available in this browser.
#[must_use]
pub fn check_barcode_detector_support() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Reflect::get(
            &js_sys::global(),
            &wasm_bindgen::JsValue::from_str("BarcodeDetector"),
        )
        .ok()
        .map(|v| !v.is_undefined())
        .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// QR Scanner component.
///
/// Shows a text input for manual entry on all platforms.
/// On browsers with BarcodeDetector + camera, also shows a scan button.
#[must_use]
#[component]
pub fn QrScanner(
    /// Called when a QR payload string is decoded or pasted.
    on_decode: Box<dyn Fn(String) + Send + Sync + 'static>,
) -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let manual_input = RwSignal::new(String::new());
    let supports_barcode = check_barcode_detector_support();
    let camera_error = RwSignal::new(Option::<String>::None);

    // Store on_decode in Arc so we can share it
    let decode_cb = Arc::new(on_decode);

    let on_manual_submit = {
        let cb = decode_cb.clone();
        move || {
            let val = manual_input.get_untracked().trim().to_string();
            if !val.is_empty() {
                (cb)(val);
            }
        }
    };

    view! {
        <div class="space-y-4">
            // Scanner section
            {move || {
                if supports_barcode {
                    view! {
                        <div class="space-y-2">
                            <p class="text-sm font-medium text-foreground">{t!("settings.devices.scan")}</p>
                            <div class="relative rounded-lg overflow-hidden bg-black aspect-video">
                                <video id="qr-scanner-video" autoplay playsinline class="w-full h-full object-cover"/>
                            </div>
                            {move || camera_error.get().map(|e| {
                                view! {
                                    <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                        <p class="text-sm">{e}</p>
                                    </div>
                                }
                            })}
                            <Button
                                variant=Signal::derive(move || ButtonVariant::Default)
                                on_click={
                                    let cb = decode_cb.clone();
                                    let err = camera_error;
                                    Box::new(move |_| {
                                        err.set(None);
                                        let cb = cb.clone();
                                        spawn_local(async move {
                                            if let Err(e) = start_barcode_scan(cb).await {
                                                err.set(Some(e));
                                            }
                                        });
                                    })
                                }
                            >
                                {t!("scan.confirm")}
                            </Button>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="relative w-full rounded-lg border border-muted p-4 bg-background text-sm text-muted-foreground">
                            {t!("scan.unavailable")}
                        </div>
                    }.into_any()
                }
            }}

            // Manual input (always available)
            <div class="space-y-2">
                <p class="text-sm font-medium text-foreground">{t!("scan.manual")}</p>
                <input
                    type="text"
                    class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                    placeholder=t!("scan.placeholder").to_string()
                    prop:value=manual_input
                    on:input=move |ev| manual_input.set(event_target_value(&ev))
                />
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    on_click=Box::new(move |_| on_manual_submit())
                    disabled=Signal::derive(move || manual_input.get().trim().is_empty())
                >
                    {t!("scan.apply")}
                </Button>
            </div>
        </div>
    }
}

/// Start BarcodeDetector scan using `getUserMedia` + `BarcodeDetector`.
///
/// Only works on WASM targets (Chromium-based browsers).
async fn start_barcode_scan(
    on_decode: Arc<Box<dyn Fn(String) + Send + Sync + 'static>>,
) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = on_decode;
        Err("BarcodeDetector not available on this platform".to_string())
    }

    #[cfg(target_arch = "wasm32")]
    {
        start_barcode_wasm(on_decode).await
    }
}

/// WASM implementation of BarcodeDetector scan.
#[cfg(target_arch = "wasm32")]
async fn start_barcode_wasm(
    on_decode: Arc<Box<dyn Fn(String) + Send + Sync + 'static>>,
) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use web_sys::HtmlVideoElement;

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    // Get or create the video element
    let video: HtmlVideoElement = document
        .get_element_by_id("qr-scanner-video")
        .ok_or("no video element found")?
        .dyn_into()
        .map_err(|_| "invalid video element")?;

    // Get camera stream (rear-facing)
    let mut constraints = web_sys::MediaStreamConstraints::new();
    let video_obj = js_sys::Object::new();
    js_sys::Reflect::set(&video_obj, &"facingMode".into(), &"environment".into())
        .map_err(|_| "failed to set facingMode")?;
    constraints.video(&video_obj);

    let media_devices = window
        .navigator()
        .media_devices()
        .ok_or("no media devices")?;

    let promise = media_devices.get_user_media_with_constraints(&constraints);
    let stream: web_sys::MediaStream = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|_| "camera access denied")?
        .dyn_into()
        .map_err(|_| "invalid MediaStream")?;

    video.set_src_object(Some(&stream));
    let _ = video.play();

    // Create BarcodeDetector instance
    let barcode_ctor = js_sys::Reflect::get(&js_sys::global(), &"BarcodeDetector".into())
        .map_err(|_| "BarcodeDetector API not available")?;
    let detector = js_sys::Reflect::construct(&barcode_ctor, &js_sys::Array::new())
        .map_err(|_| "failed to create BarcodeDetector")?;

    let max_attempts = 300u32; // ~30 seconds at 100ms/frame
    for _ in 0..max_attempts {
        gloo_timers::future::TimeoutFuture::new(100).await;

        let detect_fn = js_sys::Reflect::get(&detector, &"detect".into())
            .map_err(|_| "no detect method")?;
        let promise = js_sys::Reflect::apply(&detect_fn, &detector, &js_sys::Array::of1(&video))
            .map_err(|_| "detect failed")?;

        let barcodes: js_sys::Array = wasm_bindgen_futures::JsFuture::from(
            promise.dyn_into::<js_sys::Promise>().map_err(|_| "not a promise")?,
        )
        .await
        .map_err(|_| "detect rejected")?
        .dyn_into()
        .map_err(|_| "not an array")?;

        if barcodes.length() > 0 {
            if let Some(first) = barcodes.get(0).dyn_ref::<js_sys::Object>() {
                if let Ok(raw) = js_sys::Reflect::get(first, &"rawValue".into()) {
                    if let Some(text) = raw.as_string() {
                        stop_tracks(&stream);
                        on_decode(text);
                        return Ok(());
                    }
                }
            }
        }
    }

    stop_tracks(&stream);
    Err("QR scan timeout".to_string())
}

/// Stop all tracks on a media stream.
#[cfg(target_arch = "wasm32")]
fn stop_tracks(stream: &web_sys::MediaStream) {
    let tracks = stream.get_tracks();
    for i in 0..tracks.length() {
        if let Some(track) = tracks.get(i).dyn_ref::<web_sys::MediaStreamTrack>() {
            track.stop();
        }
    }
}

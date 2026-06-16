//! Client-side media normalization for the "send as Media" path.
//!
//! Thin Rust bindings over the `window.__sigil*` helpers defined in
//! `index.html`. Video is transcoded to a streamable fragmented H.264/AAC mp4
//! (via vendored ffmpeg.wasm running in its own worker), images are downscaled
//! and re-encoded to JPEG via a canvas. Everything runs locally so end-to-end
//! encryption is preserved — the plaintext never leaves the device.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

fn win_fn(name: &str) -> Option<js_sys::Function> {
    let win = web_sys::window()?;
    js_sys::Reflect::get(&win, &JsValue::from_str(name))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()
}

fn win_string(name: &str) -> Option<String> {
    let win = web_sys::window()?;
    js_sys::Reflect::get(&win, &JsValue::from_str(name))
        .ok()?
        .as_string()
}

/// Transcode any input video into a streamable fragmented MP4 (H.264 Main\@4.0 +
/// AAC). `on_progress` receives a 0..1 fraction. Returns `(bytes, mime)` where
/// `mime` carries the `codecs=…` parameter so MediaSource can play it. The
/// 32&nbsp;MB ffmpeg core is fetched lazily on the first call.
///
/// Web/desktop only (ffmpeg.wasm). On Android this path is replaced by the
/// native MediaCodec transcoder; callers should select per platform.
pub async fn transcode_video<F>(input: &[u8], on_progress: F) -> Result<(Vec<u8>, String), String>
where
    F: Fn(f64) + 'static,
{
    let f = win_fn("__sigilTranscodeVideo").ok_or("transcode helper missing")?;
    let arr = js_sys::Uint8Array::from(input);

    let cb = Closure::wrap(Box::new(move |p: JsValue| {
        on_progress(p.as_f64().unwrap_or(0.0));
    }) as Box<dyn FnMut(JsValue)>);

    let promise = f
        .call2(&JsValue::NULL, &arr, cb.as_ref().unchecked_ref())
        .map_err(|e| format!("transcode call failed: {e:?}"))?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "transcode did not return a promise".to_string())?;

    let out = JsFuture::from(promise)
        .await
        .map_err(|e| format!("transcode failed: {e:?}"))?;
    drop(cb); // keep the progress closure alive until the promise settles

    let bytes = out
        .dyn_into::<js_sys::Uint8Array>()
        .map_err(|_| "transcode output was not a Uint8Array".to_string())?
        .to_vec();
    let mime =
        win_string("__sigilVideoMime").unwrap_or_else(|| "video/mp4; codecs=\"avc1.4D4028, mp4a.40.2\"".into());
    Ok((bytes, mime))
}

/// Hardware-accelerated transcode via WebCodecs (in a worker). Fast path for
/// large/4K/HEVC input. Returns `(bytes, mime)` of a fragmented H.264/AAC mp4,
/// or `Err` when the browser lacks WebCodecs or can't decode the input codec —
/// the caller then falls back to [`transcode_video`] (ffmpeg.wasm).
pub async fn transcode_video_hw<F>(input: &[u8], on_progress: F) -> Result<(Vec<u8>, String), String>
where
    F: Fn(f64) + 'static,
{
    let f = win_fn("__sigilTranscodeVideoHW").ok_or("hw transcode helper missing")?;
    let arr = js_sys::Uint8Array::from(input);

    let cb = Closure::wrap(Box::new(move |p: JsValue| {
        on_progress(p.as_f64().unwrap_or(0.0));
    }) as Box<dyn FnMut(JsValue)>);

    let promise = f
        .call2(&JsValue::NULL, &arr, cb.as_ref().unchecked_ref())
        .map_err(|e| format!("hw transcode call failed: {e:?}"))?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "hw transcode did not return a promise".to_string())?;

    let out = JsFuture::from(promise)
        .await
        .map_err(|e| format!("hw transcode failed: {e:?}"))?;
    drop(cb);

    let bytes = js_sys::Reflect::get(&out, &JsValue::from_str("bytes"))
        .map_err(|_| "hw result missing bytes".to_string())?
        .dyn_into::<js_sys::Uint8Array>()
        .map_err(|_| "hw bytes not a Uint8Array".to_string())?
        .to_vec();
    let mime = js_sys::Reflect::get(&out, &JsValue::from_str("mime"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "video/mp4; codecs=\"avc1.4d0028, mp4a.40.2\"".into());
    Ok((bytes, mime))
}

/// Downscale + re-encode an image to JPEG (long side capped, EXIF stripped).
/// Returns `(bytes, "image/jpeg")`.
pub async fn compress_image(input: &[u8], mime: &str) -> Result<(Vec<u8>, String), String> {
    let f = win_fn("__sigilCompressImage").ok_or("image helper missing")?;
    let arr = js_sys::Uint8Array::from(input);

    let promise = f
        .call2(&JsValue::NULL, &arr, &JsValue::from_str(mime))
        .map_err(|e| format!("compress call failed: {e:?}"))?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "compress did not return a promise".to_string())?;

    let out = JsFuture::from(promise)
        .await
        .map_err(|e| format!("image compress failed: {e:?}"))?;

    let bytes = js_sys::Reflect::get(&out, &JsValue::from_str("bytes"))
        .map_err(|_| "compress result missing bytes".to_string())?
        .dyn_into::<js_sys::Uint8Array>()
        .map_err(|_| "compress bytes not a Uint8Array".to_string())?
        .to_vec();
    let out_mime = js_sys::Reflect::get(&out, &JsValue::from_str("mime"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "image/jpeg".into());
    Ok((bytes, out_mime))
}

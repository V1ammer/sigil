//! Streaming playback of chunked-encrypted media into an `<audio>`/`<video>`.
//!
//! The crypto stays in Rust: we `Range`-fetch each encrypted chunk, decrypt it,
//! and hand the plaintext to the MediaSource driver in `index.html`
//! (`window.__sigilStream*`). Playback starts on the first chunk instead of
//! waiting for the whole blob. If the blob isn't chunked, MediaSource can't play
//! the mime, or anything fails, we fall back to a whole-blob download + decrypt
//! and set the element `src` directly — identical to the pre-streaming path.

use base64::Engine as _;
use messenger_core::attachment_crypto::{decrypt_chunk, ChunkedHeader, CHUNK_HEADER_LEN};
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::i18n::{t, Language};
use crate::state::session::build_api_client;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn win_fn(name: &str) -> Option<js_sys::Function> {
    let win = web_sys::window()?;
    js_sys::Reflect::get(&win, &JsValue::from_str(name))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()
}

/// Whether MediaSource can play `mime` (codecs included).
fn mse_supported(mime: &str) -> bool {
    win_fn("__sigilStreamSupported")
        .and_then(|f| f.call1(&JsValue::NULL, &JsValue::from_str(mime)).ok())
        .is_some_and(|v| v.is_truthy())
}

fn decode_key(key_b64: &str) -> Option<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(key_b64).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Some(key)
}

/// Try to stream a chunked attachment into `el` via MediaSource. Returns `Err`
/// when the blob isn't chunked / the codec is unsupported / a step fails, so the
/// caller can fall back to a whole-blob download.
async fn stream_into(
    el: &web_sys::HtmlMediaElement,
    attachment_id: Uuid,
    key: &[u8; 32],
    mime: &str,
) -> Result<(), ()> {
    if !mse_supported(mime) {
        return Err(());
    }
    let api = build_api_client().ok_or(())?;

    // Header first — tells us the chunk layout. A short read or missing magic
    // means this isn't a chunked blob; fall back.
    let header_bytes = api
        .download_attachment(attachment_id, Some((0, (CHUNK_HEADER_LEN - 1) as u64)))
        .await
        .map_err(|_| ())?;
    let header = ChunkedHeader::parse(&header_bytes).ok_or(())?;

    let start = win_fn("__sigilStreamStart").ok_or(())?;
    let push = win_fn("__sigilStreamPush").ok_or(())?;
    let end = win_fn("__sigilStreamEnd").ok_or(())?;

    let handle = start
        .call2(&JsValue::NULL, el.as_ref(), &JsValue::from_str(mime))
        .map_err(|_| ())?;

    for i in 0..header.num_chunks() {
        let (s, e) = header.chunk_byte_range(i);
        let ct = api
            .download_attachment(attachment_id, Some((s, e)))
            .await
            .map_err(|_| {
                let _ = end.call1(&JsValue::NULL, &handle);
            })?;
        let plain = decrypt_chunk(key, &header.base_nonce, i, &ct).map_err(|_| {
            let _ = end.call1(&JsValue::NULL, &handle);
        })?;
        let u8a = js_sys::Uint8Array::from(plain.as_slice());
        // push() resolves on the SourceBuffer's `updateend`, so we append one
        // chunk at a time (backpressure) rather than flooding the buffer.
        let promise = push
            .call2(&JsValue::NULL, &handle, &u8a.into())
            .ok()
            .and_then(|p| p.dyn_into::<js_sys::Promise>().ok())
            .ok_or(())?;
        if JsFuture::from(promise).await.is_err() {
            let _ = end.call1(&JsValue::NULL, &handle);
            return Err(());
        }
    }
    let _ = end.call1(&JsValue::NULL, &handle);
    Ok(())
}

/// Whole-blob fallback: download (with a few retries for the sender's
/// upload→finalize race), decrypt either format, and point `el` at a blob URL.
async fn fallback_whole_blob(
    el: &web_sys::HtmlMediaElement,
    attachment_id: Uuid,
    key: &[u8; 32],
    mime: &str,
    err: RwSignal<Option<String>>,
    l: Language,
) {
    let Some(api) = build_api_client() else {
        err.set(Some("no api".into()));
        return;
    };
    let mut ct: Option<Vec<u8>> = None;
    for delay in [0u32, 250, 500, 1000, 2000] {
        if delay > 0 {
            gloo_timers::future::TimeoutFuture::new(delay).await;
        }
        match api.download_attachment(attachment_id, None).await {
            Ok(b) => {
                ct = Some(b);
                break;
            }
            Err(messenger_core::api::ApiError::Api { status: 404, .. }) => break,
            Err(_) => {}
        }
    }
    let Some(ct) = ct else {
        err.set(Some(t(l, "message.imageUnavailable").to_string()));
        return;
    };
    let Ok(plain) = messenger_core::attachment_crypto::decrypt_attachment_auto(key, &ct) else {
        err.set(Some(t(l, "message.imageUnavailable").to_string()));
        return;
    };
    let u8a = js_sys::Uint8Array::from(plain.as_slice());
    let arr = js_sys::Array::new();
    arr.push(&u8a.into());
    let mut bag = web_sys::BlobPropertyBag::new();
    bag.type_(mime);
    if let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &bag) {
        if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
            el.set_src(&url);
        }
    }
}

/// Play a media attachment into `el`: stream it chunk-by-chunk if possible,
/// otherwise fall back to a whole-blob download. `autoplay` issues `el.play()`
/// once setup begins (custom players that manage play/pause pass `false`).
pub fn play(
    el: web_sys::HtmlMediaElement,
    attachment_id: String,
    key_b64: String,
    mime: String,
    autoplay: bool,
    err: RwSignal<Option<String>>,
    l: Language,
) {
    let Ok(id) = attachment_id.parse::<Uuid>() else {
        err.set(Some("bad id".into()));
        return;
    };
    if id.is_nil() {
        return;
    }
    let Some(key) = decode_key(&key_b64) else {
        err.set(Some("bad key".into()));
        return;
    };
    spawn_local(async move {
        if stream_into(&el, id, &key, &mime).await.is_err() {
            fallback_whole_blob(&el, id, &key, &mime, err, l).await;
        }
        if autoplay {
            let _ = el.play();
        }
    });
}

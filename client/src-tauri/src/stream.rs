//! Native range-decrypt streaming proxy for E2E-encrypted media (video).
//!
//! The WebView can't Range-fetch our encrypted attachments directly (the bytes
//! are AES-GCM ciphertext), and MediaSource can't take a non-fragmented MP4. So
//! we expose a local custom-scheme URL — `stream://localhost/v/<id>` (served as
//! `http(s)://stream.localhost/v/<id>` on Android) — that the `<video>` element
//! points at. This handler answers each HTTP Range request by fetching just the
//! encrypted chunk(s) covering the requested plaintext range from the server
//! (reusing the signed `download_attachment`), decrypting them in Rust (reusing
//! `attachment_crypto`), and returning the exact plaintext slice as `206`. The
//! native demuxer then handles progressive playback and seeking of any format.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use messenger_core::api::client::{ApiClient, AuthCredentials};
use messenger_core::attachment_crypto::{
    decrypt_attachment_auto, decrypt_chunk, ChunkedHeader, CHUNK_HEADER_LEN, CHUNK_TAG_LEN,
};
use tauri::http;
use tauri::{AppHandle, Manager, Runtime, State, UriSchemeContext, UriSchemeResponder};
use uuid::Uuid;

/// One response window — how much plaintext we serve per Range request. The
/// player asks for more as it plays, so this bounds memory/latency per request.
const WINDOW: u64 = 1024 * 1024;

#[derive(Clone)]
struct Session {
    server_url: String,
    creds: AuthCredentials,
}

#[derive(Clone)]
struct Media {
    key: [u8; 32],
    mime: String,
}

/// Cached per-attachment layout so we don't re-probe the header (or re-download
/// a whole non-chunked blob) on every Range request.
#[derive(Clone)]
enum Layout {
    Chunked {
        chunk_size: u32,
        total_len: u64,
        base_nonce: [u8; 8],
    },
    /// Non-chunked (legacy) attachment, fully decrypted once and kept in memory.
    Whole(Arc<Vec<u8>>),
}

#[derive(Default)]
pub struct StreamState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    session: Option<Session>,
    media: HashMap<Uuid, Media>,
    layout: HashMap<Uuid, Layout>,
}

fn decode_32(b64: &str) -> Option<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

/// Register the session + a media key so the `stream` protocol can serve this
/// attachment. Called by the frontend right before it points a `<video>` at the
/// proxy URL.
#[tauri::command]
pub fn stream_prepare(
    state: State<'_, StreamState>,
    server_url: String,
    device_id: String,
    secret_b64: String,
    attachment_id: String,
    key_b64: String,
    mime: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&attachment_id).map_err(|_| "bad attachment id".to_string())?;
    let device_id = Uuid::parse_str(&device_id).map_err(|_| "bad device id".to_string())?;
    let secret = decode_32(&secret_b64).ok_or_else(|| "bad device secret".to_string())?;
    let key = decode_32(&key_b64).ok_or_else(|| "bad media key".to_string())?;

    let mut inner = state.inner.lock().map_err(|_| "lock".to_string())?;
    inner.session = Some(Session {
        server_url,
        creds: AuthCredentials {
            device_id,
            device_signing_secret: secret,
        },
    });
    inner.media.insert(id, Media { key, mime });
    Ok(())
}

/// The custom-scheme protocol handler. Runs the async fetch/decrypt on a
/// dedicated current-thread runtime (the API transport futures are `!Send`).
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let id = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .and_then(|s| Uuid::parse_str(s).ok());
    let range = request
        .headers()
        .get(http::header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let (session, media, layout) = {
        let state = app.state::<StreamState>();
        let inner = match state.inner.lock() {
            Ok(g) => g,
            Err(_) => {
                responder.respond(resp_status(500));
                return;
            }
        };
        let id_ref = id.as_ref();
        (
            inner.session.clone(),
            id_ref.and_then(|i| inner.media.get(i).cloned()),
            id_ref.and_then(|i| inner.layout.get(i).cloned()),
        )
    };

    let (Some(id), Some(session), Some(media)) = (id, session, media) else {
        responder.respond(resp_status(404));
        return;
    };

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => {
                responder.respond(resp_status(500));
                return;
            }
        };
        let resp = rt.block_on(build_response(&app, id, session, media, layout, range));
        responder.respond(resp);
    });
}

async fn build_response<R: Runtime>(
    app: &AppHandle<R>,
    id: Uuid,
    session: Session,
    media: Media,
    cached: Option<Layout>,
    range: Option<String>,
) -> http::Response<Vec<u8>> {
    let client = ApiClient::new(session.server_url).with_auth(session.creds);

    // Resolve the layout (header for chunked blobs; whole decrypt for legacy).
    let layout = match cached {
        Some(l) => l,
        None => {
            let header_bytes = match client
                .download_attachment(id, Some((0, (CHUNK_HEADER_LEN as u64) - 1)))
                .await
            {
                Ok(b) => b,
                Err(_) => return resp_status(502),
            };
            let layout = if let Some(h) = ChunkedHeader::parse(&header_bytes) {
                Layout::Chunked {
                    chunk_size: h.chunk_size,
                    total_len: h.total_len,
                    base_nonce: h.base_nonce,
                }
            } else {
                // Legacy whole-blob encryption — decrypt once, cache in memory.
                let ct = match client.download_attachment(id, None).await {
                    Ok(b) => b,
                    Err(_) => return resp_status(502),
                };
                match decrypt_attachment_auto(&media.key, &ct) {
                    Ok(plain) => Layout::Whole(Arc::new(plain)),
                    Err(_) => return resp_status(502),
                }
            };
            if let Ok(mut inner) = app.state::<StreamState>().inner.lock() {
                inner.layout.insert(id, layout.clone());
            }
            layout
        }
    };

    let total = match &layout {
        Layout::Chunked { total_len, .. } => *total_len,
        Layout::Whole(p) => p.len() as u64,
    };
    if total == 0 {
        return resp_status(502);
    }

    let (start, req_end) = parse_range(range.as_deref(), total);
    if start >= total {
        return resp_range_not_satisfiable(total);
    }
    // Cap the response to one window; the player will request the rest.
    let end = req_end.min(start + WINDOW - 1).min(total - 1);

    let body = match &layout {
        Layout::Whole(p) => p[start as usize..=end as usize].to_vec(),
        Layout::Chunked {
            chunk_size,
            total_len,
            base_nonce,
        } => {
            let header = ChunkedHeader {
                chunk_size: *chunk_size,
                total_len: *total_len,
                base_nonce: *base_nonce,
            };
            match fetch_chunked_range(&client, id, &media.key, &header, start, end).await {
                Ok(b) => b,
                Err(_) => return resp_status(502),
            }
        }
    };

    http::Response::builder()
        .status(http::StatusCode::PARTIAL_CONTENT)
        .header(http::header::CONTENT_TYPE, media.mime)
        .header(http::header::ACCEPT_RANGES, "bytes")
        .header(
            http::header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        )
        .header(http::header::CONTENT_LENGTH, (end - start + 1).to_string())
        .header(http::header::CACHE_CONTROL, "no-store")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| resp_status(500))
}

/// Fetch + decrypt the chunk(s) covering plaintext `[start, end]`, then slice to
/// exactly that range. One ranged GET spans all the needed encrypted chunks.
async fn fetch_chunked_range(
    client: &ApiClient,
    id: Uuid,
    key: &[u8; 32],
    header: &ChunkedHeader,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, ()> {
    let cs = u64::from(header.chunk_size);
    let first = (start / cs) as u32;
    let last = (end / cs) as u32;

    let (enc_start, _) = header.chunk_byte_range(first);
    let (_, enc_end) = header.chunk_byte_range(last);
    let ct = client
        .download_attachment(id, Some((enc_start, enc_end)))
        .await
        .map_err(|_| ())?;

    decode_window(header, key, first, last, &ct, start, end)
}

/// Decrypt the ciphertext window covering chunks `[first, last]` and return
/// exactly plaintext `[start, end]`. Pure (no network) so it can be tested.
fn decode_window(
    header: &ChunkedHeader,
    key: &[u8; 32],
    first: u32,
    last: u32,
    ct_window: &[u8],
    start: u64,
    end: u64,
) -> Result<Vec<u8>, ()> {
    let cs = u64::from(header.chunk_size);
    let mut plain = Vec::new();
    let mut off = 0usize;
    for idx in first..=last {
        let clen = (header.chunk_plaintext_len(idx) + CHUNK_TAG_LEN) as usize;
        let chunk_ct = ct_window.get(off..off + clen).ok_or(())?;
        let dec = decrypt_chunk(key, &header.base_nonce, idx, chunk_ct).map_err(|_| ())?;
        plain.extend_from_slice(&dec);
        off += clen;
    }

    let base = u64::from(first) * cs;
    let s = (start - base) as usize;
    let e = (end - base) as usize;
    plain.get(s..=e).map(<[u8]>::to_vec).ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use messenger_core::attachment_crypto::{encrypt_attachment_chunked, generate_encryption_key};

    /// End-to-end check of the chunk-index math: encrypt a blob, then for a set
    /// of plaintext ranges slice exactly the encrypted chunks the proxy would
    /// fetch, decode them, and assert the result equals the plaintext slice.
    #[test]
    fn decode_window_returns_exact_plaintext_range() {
        let key = generate_encryption_key();
        // Plaintext spanning several chunks plus a partial last chunk.
        let plain: Vec<u8> = (0..(256 * 1024 * 3 + 12345)).map(|i| (i % 251) as u8).collect();
        let chunk_size = 256 * 1024u32;
        let blob = encrypt_attachment_chunked(&key, &plain, chunk_size).unwrap();
        let header = ChunkedHeader::parse(&blob).unwrap();
        let cs = u64::from(chunk_size);

        let total = plain.len() as u64;
        let cases = [
            (0u64, 0u64),
            (0, cs - 1),
            (0, cs), // crosses a chunk boundary
            (cs - 5, cs + 5),
            (cs + 1, 2 * cs + 100),
            (total - 1, total - 1),
            (total - 9000, total - 1), // into the partial last chunk
            (5, total - 1),            // whole thing
        ];
        for (start, end) in cases {
            let first = (start / cs) as u32;
            let last = (end / cs) as u32;
            let (enc_start, _) = header.chunk_byte_range(first);
            let (_, enc_end) = header.chunk_byte_range(last);
            let window = &blob[enc_start as usize..=enc_end as usize];
            let got = decode_window(&header, &key, first, last, window, start, end).unwrap();
            assert_eq!(
                got,
                &plain[start as usize..=end as usize],
                "range {start}..={end}"
            );
        }
    }
}

/// Parse a `Range: bytes=start-end` header. Missing/last value defaults to EOF.
fn parse_range(range: Option<&str>, total: u64) -> (u64, u64) {
    let default = (0u64, total.saturating_sub(1));
    let Some(r) = range else { return default };
    let Some(rest) = r.trim().strip_prefix("bytes=") else {
        return default;
    };
    let Some((s, e)) = rest.split_once('-') else {
        return default;
    };
    let start = s.trim().parse::<u64>().unwrap_or(0);
    let end = e
        .trim()
        .parse::<u64>()
        .unwrap_or(total.saturating_sub(1))
        .min(total.saturating_sub(1));
    (start, end)
}

fn resp_status(code: u16) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(code)
        .header("Access-Control-Allow-Origin", "*")
        .body(Vec::new())
        .expect("static response builds")
}

fn resp_range_not_satisfiable(total: u64) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(http::StatusCode::RANGE_NOT_SATISFIABLE)
        .header(http::header::CONTENT_RANGE, format!("bytes */{total}"))
        .body(Vec::new())
        .expect("static response builds")
}

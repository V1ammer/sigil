//! Web HTTP transport using `gloo-net` with timeout.

use async_trait::async_trait;
use futures_util::future::Either;
use gloo_net::http::Request;
use std::pin::pin;

use super::{ApiError, HttpResponse, HttpTransport};

/// Base timeout in milliseconds for HTTP requests.
const REQUEST_TIMEOUT_MS: u32 = 30_000;

/// Extra timeout budget per k**i**byte of request body, assuming a slow
/// ~50 KB/s uplink floor (20 ms/KiB). Without this, a large attachment upload
/// (e.g. a video) would be aborted at the 30 s base and fail silently.
const PER_KIB_TIMEOUT_MS: u32 = 20;

/// Timeout for a request whose body is `body_len` bytes.
fn timeout_for_body(body_len: usize) -> u32 {
    let kib = u32::try_from(body_len / 1024).unwrap_or(u32::MAX);
    REQUEST_TIMEOUT_MS.saturating_add(kib.saturating_mul(PER_KIB_TIMEOUT_MS))
}

/// HTTP methods that must not have a body per the Fetch spec.
fn method_forbids_body(method: &str) -> bool {
    matches!(method, "GET" | "HEAD" | "DELETE")
}

/// Gloo-net-based HTTP transport for WASM targets.
pub struct GlooNetTransport;

#[async_trait(?Send)]
impl HttpTransport for GlooNetTransport {
    async fn request(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, ApiError> {
        let mut req = match method {
            "GET" => Request::get(url),
            "POST" => Request::post(url),
            "PUT" => Request::put(url),
            "DELETE" => Request::delete(url),
            "PATCH" => Request::patch(url),
            _ => return Err(ApiError::Transport("bad method".into())),
        };
        for (k, v) in &headers {
            req = req.header(k, v);
        }

        // Build the request future
        let send_fut = if method_forbids_body(method) || body.is_empty() {
            Either::Left(req.send())
        } else {
            let req_with_body = req
                .body(js_sys::Uint8Array::from(body.as_slice()))
                .map_err(|e| ApiError::Transport(format!("{method} {url} — body failed: {e}")))?;
            Either::Right(req_with_body.send())
        };

        // Race request against a body-size-aware timeout.
        let timeout_ms = timeout_for_body(body.len());
        let timeout_fut = gloo_timers::future::TimeoutFuture::new(timeout_ms);
        let pinned_send = pin!(send_fut);
        let pinned_timeout = pin!(timeout_fut);

        let resp = match futures_util::future::select(pinned_send, pinned_timeout).await {
            Either::Left((result, _)) => {
                match result {
                    Ok(r) => r,
                    Err(e) => return Err(ApiError::Transport(format!("{method} {url} — {e}"))),
                }
            }
            Either::Right(((), _)) => {
                return Err(ApiError::Transport(format!("{method} {url} — request timed out after {timeout_ms}ms")));
            }
        };

        let status = resp.status();
        let resp_body = resp.binary().await
            .map_err(|e| ApiError::Transport(format!("{method} {url} — read body failed: {e}")))?;
        #[cfg(target_arch = "wasm32")]
        if resp_body.len() > 0 {
            let first_byte_str = format!("[transport] {method} {url} — status={status}, body_len={}, first_byte={}",
                resp_body.len(), resp_body[0]);
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&first_byte_str));
        }
        Ok(HttpResponse {
            status,
            body: resp_body,
            headers: vec![],
        })
    }
}

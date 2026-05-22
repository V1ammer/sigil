//! Web HTTP transport using `gloo-net`.

use async_trait::async_trait;
use gloo_net::http::Request;

use super::{ApiError, HttpResponse, HttpTransport};

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
        let resp = req
            .body(js_sys::Uint8Array::from(body.as_slice()))
            .map_err(|e| ApiError::Transport(e.to_string()))?
            .send()
            .await
            .map_err(|e| ApiError::Transport(e.to_string()))?;

        let status = resp.status();
        let body = resp.binary().await.map_err(|e| ApiError::Transport(e.to_string()))?;
        Ok(HttpResponse {
            status,
            body,
            headers: vec![],
        })
    }
}

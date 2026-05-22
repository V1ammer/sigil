//! Native HTTP transport using `reqwest`.

use async_trait::async_trait;
use reqwest;

use super::{ApiError, HttpResponse, HttpTransport};

/// Reqwest-based HTTP transport.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Create a new transport with a 30-second timeout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client builds with default config; this is infallible"),
        }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl HttpTransport for ReqwestTransport {
    async fn request(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, ApiError> {
        let m = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| ApiError::Transport("bad method".into()))?;
        let mut req = self.client.request(m, url).body(body);
        for (k, v) in headers {
            req = req.header(&k, &v);
        }
        let resp = req.send().await.map_err(|e| ApiError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp.bytes().await.map_err(|e| ApiError::Transport(e.to_string()))?.to_vec();
        Ok(HttpResponse {
            status,
            body,
            headers: vec![],
        })
    }
}

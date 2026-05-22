//! HTTP API client with automatic request signing.

use super::{signing, transport, ApiError, HttpResponse};
use crate::ed25519::Ed25519Pair;
use uuid::Uuid;

/// Credentials used to sign outgoing requests.
#[derive(Clone)]
pub struct AuthCredentials {
    pub device_id: Uuid,
    pub device_signing_secret: [u8; 32],
}

/// High-level HTTP API client.
pub struct ApiClient {
    pub(crate) base_url: String,
    pub(crate) transport: Box<dyn transport::HttpTransport>,
    auth: Option<AuthCredentials>,
}

impl ApiClient {
    /// Create a new unauthenticated client.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            transport: transport::default_transport(),
            auth: None,
        }
    }

    /// Set authentication credentials (builder style).
    #[must_use]
    pub fn with_auth(mut self, creds: AuthCredentials) -> Self {
        self.auth = Some(creds);
        self
    }

    /// Set or clear authentication credentials.
    pub fn set_auth(&mut self, creds: Option<AuthCredentials>) {
        self.auth = creds;
    }

    /// Send an HTTP request with optional body and parse the response.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network, serialization, or API errors.
    pub async fn send<Req, Resp>(
        &self,
        method: &str,
        path: &str,
        body: Option<&Req>,
    ) -> Result<Resp, ApiError>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let body_bytes = if let Some(b) = body {
            rmp_serde::to_vec_named(b)?
        } else {
            Vec::new()
        };

        let mut headers = vec![
            ("Content-Type".to_string(), "application/msgpack".to_string()),
            ("Accept".to_string(), "application/msgpack".to_string()),
        ];

        if let Some(creds) = &self.auth {
            let ts = signing::now_secs();
            let mut nonce = [0u8; 16];
            getrandom::getrandom(&mut nonce).map_err(|e| ApiError::Crypto(e.to_string()))?;
            let canonical = signing::build_signed_message(method, path, ts, &nonce, &body_bytes);
            let pair = Ed25519Pair::from_secret_bytes(&creds.device_signing_secret);
            let sig = pair.sign(&canonical);
            let auth_header = format!(
                "{}:{}:{}:{}",
                hex::encode(creds.device_id.as_bytes()),
                ts,
                hex::encode(&nonce),
                hex::encode(&sig)
            );
            headers.push(("X-Auth-Signature".to_string(), auth_header));
        }

        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .transport
            .request(method, &url, headers, body_bytes)
            .await?;

        parse_response(resp)
    }

    /// Send a raw byte request (e.g. attachment upload) with auth headers.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or API errors.
    pub async fn send_raw(
        &self,
        method: &str,
        path: &str,
        extra_headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, ApiError> {
        let mut headers = vec![
            ("Content-Type".to_string(), "application/octet-stream".to_string()),
        ];
        headers.extend(extra_headers);

        if let Some(creds) = &self.auth {
            let ts = signing::now_secs();
            let mut nonce = [0u8; 16];
            getrandom::getrandom(&mut nonce).map_err(|e| ApiError::Crypto(e.to_string()))?;
            let canonical = signing::build_signed_message(method, path, ts, &nonce, &body);
            let pair = Ed25519Pair::from_secret_bytes(&creds.device_signing_secret);
            let sig = pair.sign(&canonical);
            let auth_header = format!(
                "{}:{}:{}:{}",
                hex::encode(creds.device_id.as_bytes()),
                ts,
                hex::encode(&nonce),
                hex::encode(&sig)
            );
            headers.push(("X-Auth-Signature".to_string(), auth_header));
        }

        let url = format!("{}{}", self.base_url, path);
        self.transport.request(method, &url, headers, body).await
    }
}

pub(crate) fn parse_response<Resp>(resp: HttpResponse) -> Result<Resp, ApiError>
where
    Resp: serde::de::DeserializeOwned,
{
    if resp.status >= 400 {
        let err: messenger_proto::error::ApiErrorBody = rmp_serde::from_slice(&resp.body)
            .unwrap_or_else(|_| messenger_proto::error::ApiErrorBody {
                code: format!("HTTP_{}", resp.status),
                details: None,
            });
        return Err(ApiError::Api {
            status: resp.status,
            body: err,
        });
    }

    if resp.body.is_empty() {
        // For 204 No Content — try to deserialize empty map.
        // Callers that expect () will succeed; others will get a parse error.
        return rmp_serde::from_slice(&[0x80]).map_err(ApiError::Deserialize);
    }

    rmp_serde::from_slice(&resp.body).map_err(ApiError::Deserialize)
}

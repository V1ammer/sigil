//! HTTP API client and WebSocket client.

pub mod client;
pub mod endpoints;
pub mod signing;
pub mod transport;
pub mod ws;

use thiserror::Error;

/// Unified API error type.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("serialize error: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),
    #[error("deserialize error: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
    #[error("api error: {status} {0}", body.code)]
    Api {
        status: u16,
        body: messenger_proto::error::ApiErrorBody,
    },
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("not authenticated")]
    NotAuthenticated,
}

impl ApiError {
    /// Returns `true` if this is an authentication error (HTTP 401).
    #[must_use]
    pub const fn is_auth_error(&self) -> bool {
        matches!(self, Self::Api { status: 401, .. })
    }

    /// Returns the server error code if this is an API error.
    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        match self {
            Self::Api { body, .. } => Some(&body.code),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::signing::build_signed_message;

    #[test]
    fn test_signed_request_format() {
        let canonical = build_signed_message("POST", "/v1/groups", 1_234_567_890, &[1, 2, 3, 4], b"body");
        let text = String::from_utf8(canonical).unwrap();
        assert!(text.starts_with("POST\n/v1/groups\n1234567890\n01020304\n"));
        // blake3 hash of "body" follows after the last newline
        let expected_hash = blake3::hash(b"body").to_hex().to_string();
        assert!(text.ends_with(&expected_hash));
    }

    #[test]
    fn test_msgpack_round_trip() {
        let req = messenger_proto::invites::CreateInviteRequest {
            role_to_grant: "user".into(),
            max_uses: 1,
            ttl_seconds: 3600,
        };
        let bytes = rmp_serde::to_vec_named(&req).unwrap();
        let back: messenger_proto::invites::CreateInviteRequest =
            rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(back.role_to_grant, "user");
        assert_eq!(back.max_uses, 1);
        assert_eq!(back.ttl_seconds, 3600);
    }
}

/// HTTP response returned by a transport.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

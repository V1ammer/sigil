//! WebSocket client with reconnect and auth handshake.

pub mod client;
pub mod transport;

use thiserror::Error;

/// WebSocket-specific errors.
#[derive(Debug, Error)]
pub enum WsError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("serialize error: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),
    #[error("deserialize error: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Frames sent by the client over the WebSocket.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    Auth {
        device_id: Uuid,
        timestamp: i64,
        #[serde(with = "serde_bytes")]
        nonce: Vec<u8>,
        #[serde(with = "serde_bytes")]
        signature: Vec<u8>,
    },
    Ping,
    Typing {
        group_id: Uuid,
        started: bool,
    },
}

/// Frames sent by the server over the WebSocket.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    AuthOk {
        user_id: Uuid,
    },
    AuthError {
        code: String,
    },
    Pong,
    NewMessage {
        group_id: Uuid,
        message_id: Uuid,
        epoch: i64,
    },
    NewWelcome {
        welcome_id: Uuid,
        group_id: Uuid,
    },
    KeyChange {
        user_id: Uuid,
        device_id: Uuid,
        event: String,
    },
    Typing {
        group_id: Uuid,
        user_id: Uuid,
        started: bool,
    },
    Error {
        code: String,
        message: Option<String>,
    },
}

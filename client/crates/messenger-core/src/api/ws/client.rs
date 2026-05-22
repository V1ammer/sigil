//! WebSocket client with automatic reconnect, auth handshake, and heartbeat.

use std::sync::{Arc, RwLock};

use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures_util::{FutureExt, StreamExt};
use messenger_proto::ws::{ClientFrame, ServerFrame};
use rand::Rng;
use uuid::Uuid;

use super::transport;
use super::transport::WsTransport;
use super::WsError;
use crate::api::client::AuthCredentials;
use crate::api::signing;
use crate::ed25519::Ed25519Pair;

/// Current connection state.
#[derive(Debug, Clone)]
pub enum WsState {
    Disconnected,
    Connecting,
    Connected {
        user_id: Uuid,
    },
    Failed(String),
}

/// WebSocket client handle.
pub struct WsClient {
    tx_outgoing: UnboundedSender<ClientFrame>,
    rx_incoming: UnboundedReceiver<ServerFrame>,
    state: Arc<RwLock<WsState>>,
}

impl WsClient {
    /// Connect to the WebSocket endpoint and start the background reconnect loop.
    ///
    /// # Errors
    ///
    /// Returns `WsError` if the initial connection or auth handshake fails.
    pub async fn connect(base_url: &str, auth: AuthCredentials) -> Result<Self, WsError> {
        let (tx_incoming, rx_incoming) = unbounded::<ServerFrame>();
        let (tx_outgoing, rx_outgoing) = unbounded::<ClientFrame>();
        let state = Arc::new(RwLock::new(WsState::Connecting));

        let ws_url = base_url_to_ws(base_url);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let state_clone = Arc::clone(&state);
            tokio::task::spawn_local(async move {
                reconnect_loop(ws_url, auth, tx_incoming, rx_outgoing, state_clone).await;
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let state_clone = Arc::clone(&state);
            wasm_bindgen_futures::spawn_local(async move {
                reconnect_loop(ws_url, auth, tx_incoming, rx_outgoing, state_clone).await;
            });
        }

        Ok(Self {
            tx_outgoing,
            rx_incoming,
            state,
        })
    }

    /// Send a frame to the server.
    ///
    /// # Errors
    ///
    /// Returns `ApiError::Transport` if the outgoing channel is closed.
    pub fn send(&self, frame: ClientFrame) -> Result<(), crate::api::ApiError> {
        self.tx_outgoing
            .unbounded_send(frame)
            .map_err(|_| crate::api::ApiError::Transport("ws closed".into()))
    }

    /// Receive the next incoming server frame.
    pub async fn next_event(&mut self) -> Option<ServerFrame> {
        self.rx_incoming.next().await
    }

    /// Get a snapshot of the current connection state.
    #[must_use]
    pub fn state(&self) -> WsState {
        self.state.read().unwrap().clone()
    }
}

fn base_url_to_ws(url: &str) -> String {
    url.replace("http://", "ws://")
        .replace("https://", "wss://")
}

async fn reconnect_loop(
    base_url: String,
    auth: AuthCredentials,
    tx_to_caller: UnboundedSender<ServerFrame>,
    rx_from_caller: UnboundedReceiver<ClientFrame>,
    state: Arc<RwLock<WsState>>,
) {
    let mut backoff_ms = 500u64;
    let mut rx_from_caller = rx_from_caller;

    loop {
        *state.write().unwrap() = WsState::Connecting;

        let result = try_run(
            &base_url,
            &auth,
            &tx_to_caller,
            &mut rx_from_caller,
            &state,
        )
        .await;

        match result {
            Ok(()) => {
                backoff_ms = 500;
            }
            Err(e) => {
                tracing::warn!(error = %e, "ws disconnected");
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        }

        *state.write().unwrap() = WsState::Disconnected;

        let jitter = rand::thread_rng().gen_range(0..250);
        sleep_ms(backoff_ms + jitter).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn try_run(
    base_url: &str,
    auth: &AuthCredentials,
    tx_to_caller: &UnboundedSender<ServerFrame>,
    rx_from_caller: &mut UnboundedReceiver<ClientFrame>,
    state: &Arc<RwLock<WsState>>,
) -> Result<(), WsError> {
    use transport::native::TungsteniteTransport;

    let url = format!("{}/v1/ws", base_url);
    let socket = TungsteniteTransport::connect(&url).await?;

    // Auth handshake
    do_auth_handshake(&socket, auth, state).await?;

    let socket = Arc::new(socket);

    let mut heartbeat = Box::pin(async {
        loop {
            sleep_ms(30_000).await;
            if socket.send_msgpack(&ClientFrame::Ping).await.is_err() {
                break;
            }
        }
    }
    .fuse());

    let tx_to_caller = tx_to_caller.clone();
    let socket_rd = Arc::clone(&socket);
    let mut reader = Box::pin(async {
        loop {
            match socket_rd.recv_msgpack::<ServerFrame>().await {
                Ok(frame) => {
                    if tx_to_caller.unbounded_send(frame).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }
    .fuse());

    let socket_wr = Arc::clone(&socket);
    loop {
        futures_util::select! {
            _ = heartbeat => break,
            frame = rx_from_caller.next() => {
                match frame {
                    Some(frame) => {
                        if socket_wr.send_msgpack(&frame).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = reader => break,
        }
    }

    let _ = socket.close().await;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn try_run(
    base_url: &str,
    auth: &AuthCredentials,
    tx_to_caller: &UnboundedSender<ServerFrame>,
    rx_from_caller: &mut UnboundedReceiver<ClientFrame>,
    state: &Arc<RwLock<WsState>>,
) -> Result<(), WsError> {
    use transport::web::GlooWsTransport;

    let url = format!("{}/v1/ws", base_url);
    let socket = GlooWsTransport::connect(&url).await?;

    // Auth handshake
    do_auth_handshake(&socket, auth, state).await?;

    let socket = Arc::new(socket);

    let mut heartbeat = Box::pin(async {
        loop {
            sleep_ms(30_000).await;
            if socket.send_msgpack(&ClientFrame::Ping).await.is_err() {
                break;
            }
        }
    }
    .fuse());

    let tx_to_caller = tx_to_caller.clone();
    let socket_rd = Arc::clone(&socket);
    let mut reader = Box::pin(async {
        loop {
            match socket_rd.recv_msgpack::<ServerFrame>().await {
                Ok(frame) => {
                    if tx_to_caller.unbounded_send(frame).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }
    .fuse());

    let socket_wr = Arc::clone(&socket);
    loop {
        futures_util::select! {
            _ = heartbeat => break,
            frame = rx_from_caller.next() => {
                match frame {
                    Some(frame) => {
                        if socket_wr.send_msgpack(&frame).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = reader => break,
        }
    }

    let _ = socket.close().await;
    Ok(())
}

async fn do_auth_handshake<S: WsTransport>(
    socket: &S,
    auth: &AuthCredentials,
    state: &Arc<RwLock<WsState>>,
) -> Result<(), WsError> {
    let ts = signing::now_secs();
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|e| WsError::Transport(e.to_string()))?;
    let canonical = signing::build_signed_message("GET", "/v1/ws", ts, &nonce, &[]);
    let pair = Ed25519Pair::from_secret_bytes(&auth.device_signing_secret);
    let sig = pair.sign(&canonical);

    let auth_frame = ClientFrame::Auth {
        device_id: auth.device_id,
        timestamp: ts,
        nonce: nonce.to_vec(),
        signature: sig.to_vec(),
    };

    socket.send_msgpack(&auth_frame).await?;

    let first: ServerFrame = socket.recv_msgpack().await?;
    match first {
        ServerFrame::AuthOk { user_id } => {
            *state.write().unwrap() = WsState::Connected { user_id };
        }
        ServerFrame::AuthError { code } => {
            return Err(WsError::Auth(code));
        }
        _ => return Err(WsError::Protocol("expected AuthOk".into())),
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u64) {
    gloo_timers::future::sleep(std::time::Duration::from_millis(ms)).await;
}

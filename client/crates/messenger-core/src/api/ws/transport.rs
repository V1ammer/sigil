//! Platform-specific WebSocket transport.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use super::WsError;

/// Platform-agnostic WebSocket transport.
#[async_trait(?Send)]
pub trait WsTransport {
    /// Send a MessagePack-encoded frame.
    async fn send_msgpack<T: Serialize + Sync>(&self, frame: &T) -> Result<(), WsError>;
    /// Receive and decode a MessagePack frame.
    async fn recv_msgpack<T: DeserializeOwned>(&self) -> Result<T, WsError>;
    /// Close the connection.
    async fn close(&self) -> Result<(), WsError>;
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpStream;
    use tokio::sync::Mutex;
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

    /// Native WebSocket transport using `tokio-tungstenite`.
    pub struct TungsteniteTransport {
        sink: Mutex<
            futures_util::stream::SplitSink<
                WebSocketStream<MaybeTlsStream<TcpStream>>,
                Message,
            >,
        >,
        stream: Mutex<
            futures_util::stream::SplitStream<
                WebSocketStream<MaybeTlsStream<TcpStream>>,
            >,
        >,
    }

    impl TungsteniteTransport {
        /// Connect to a WebSocket URL.
        ///
        /// # Errors
        ///
        /// Returns `WsError::Transport` on connection failure.
        pub async fn connect(url: &str) -> Result<Self, WsError> {
            let (ws, _) = connect_async(url)
                .await
                .map_err(|e| WsError::Transport(e.to_string()))?;
            let (sink, stream) = ws.split();
            Ok(Self {
                sink: Mutex::new(sink),
                stream: Mutex::new(stream),
            })
        }
    }

    #[async_trait(?Send)]
    impl WsTransport for TungsteniteTransport {
        async fn send_msgpack<T: Serialize + Sync>(&self, frame: &T) -> Result<(), WsError> {
            let bytes = rmp_serde::to_vec_named(frame)?;
            let mut sink = self.sink.lock().await;
            sink.send(Message::Binary(bytes))
                .await
                .map_err(|e| WsError::Transport(e.to_string()))?;
            Ok(())
        }

        async fn recv_msgpack<T: DeserializeOwned>(&self) -> Result<T, WsError> {
            let mut stream = self.stream.lock().await;
            loop {
                match stream.next().await {
                    Some(Ok(Message::Binary(bytes))) => {
                        return rmp_serde::from_slice(&bytes).map_err(Into::into);
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(WsError::Transport("connection closed".into()));
                    }
                    Some(Err(e)) => return Err(WsError::Transport(e.to_string())),
                    _ => continue,
                }
            }
        }

        async fn close(&self) -> Result<(), WsError> {
            let mut sink = self.sink.lock().await;
            sink.close()
                .await
                .map_err(|e| WsError::Transport(e.to_string()))?;
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use futures_util::lock::Mutex;
    use gloo_net::websocket::futures::WebSocket;
    use gloo_net::websocket::Message;

    /// Web WebSocket transport using `gloo-net`.
    pub struct GlooWsTransport {
        sink: Mutex<futures_util::stream::SplitSink<WebSocket, Message>>,
        stream: Mutex<futures_util::stream::SplitStream<WebSocket>>,
    }

    impl GlooWsTransport {
        /// Connect to a WebSocket URL.
        ///
        /// # Errors
        ///
        /// Returns `WsError::Transport` on connection failure.
        pub async fn connect(url: &str) -> Result<Self, WsError> {
            let ws = WebSocket::open(url).map_err(|e| WsError::Transport(e.to_string()))?;
            let (sink, stream) = ws.split();
            Ok(Self {
                sink: Mutex::new(sink),
                stream: Mutex::new(stream),
            })
        }
    }

    #[async_trait(?Send)]
    impl WsTransport for GlooWsTransport {
        async fn send_msgpack<T: Serialize + Sync>(&self, frame: &T) -> Result<(), WsError> {
            let bytes = rmp_serde::to_vec_named(frame)?;
            let mut sink = self.sink.lock().await;
            sink.send(Message::Bytes(bytes))
                .await
                .map_err(|e| WsError::Transport(e.to_string()))?;
            Ok(())
        }

        async fn recv_msgpack<T: DeserializeOwned>(&self) -> Result<T, WsError> {
            let mut stream = self.stream.lock().await;
            loop {
                match stream.next().await {
                    Some(Ok(Message::Bytes(bytes))) => {
                        return rmp_serde::from_slice(&bytes).map_err(Into::into);
                    }
                    Some(Err(e)) => return Err(WsError::Transport(e.to_string())),
                    _ => continue,
                }
            }
        }

        async fn close(&self) -> Result<(), WsError> {
            let mut sink = self.sink.lock().await;
            sink.close()
                .await
                .map_err(|e| WsError::Transport(e.to_string()))?;
            Ok(())
        }
    }
}

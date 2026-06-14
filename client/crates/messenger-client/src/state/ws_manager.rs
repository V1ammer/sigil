//! WebSocket connection manager — connects after auth and dispatches real-time events.
//!
//! Wraps `WsClient` in a reactive shell.  After authentication, call `connect()` to
//! start the background event loop.  Events are dispatched to the relevant Leptos
//! stores (chats, messages, connectivity).

use std::sync::Arc;
use std::sync::Mutex;

use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_core::api::client::AuthCredentials;
use messenger_core::api::ws::client::WsClient;
use messenger_proto::ws::{ClientFrame, ServerFrame};
use uuid::Uuid;

use super::connectivity::{ConnectivityState, WsConnectivity};

/// Whether typing indicators are enabled (privacy setting, default on). Gates
/// both sending our own typing and surfacing peers' — reciprocal, like read
/// receipts.
fn typing_indicators_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("ms_settings_typing_indicators").ok().flatten())
        .map_or(true, |v| v != "false")
}

/// Reactive WebSocket handle — Send + Sync for Leptos context.
#[derive(Clone)]
pub struct WsManager {
    /// Queue of outgoing frames.  The event loop drains this periodically.
    outgoing: Arc<Mutex<Vec<ClientFrame>>>,
    /// Connectivity state shared with the UI.
    pub connectivity: ConnectivityState,
    /// Whether we have an active event loop running.
    running: Arc<Mutex<bool>>,
}

impl WsManager {
    /// Create a new disconnected manager.
    pub fn new() -> Self {
        Self {
            outgoing: Arc::new(Mutex::new(Vec::new())),
            connectivity: ConnectivityState::new(),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the WebSocket connection and background event loop.
    ///
    /// Call once after successful authentication. The event loop reconnects
    /// automatically with exponential backoff (1s, 2s, 4s, 8s, capped at 30s)
    /// after the WebSocket drops.
    pub fn connect(&self, base_url: &str, auth: AuthCredentials) {
        let url = base_url.to_string();
        let outgoing = self.outgoing.clone();
        let connectivity = self.connectivity.clone();

        {
            let mut running = self.running.lock().unwrap();
            if *running {
                tracing::warn!("ws already running");
                return;
            }
            *running = true;
        }

        connectivity.ws_state.set(WsConnectivity::Connecting);

        spawn_local(async move {
            let mut backoff_ms: u32 = 1_000;
            loop {
                connectivity.ws_state.set(WsConnectivity::Connecting);
                match WsClient::connect(&url, auth.clone()).await {
                    Ok(mut client) => {
                        connectivity.ws_state.set(WsConnectivity::Connected);
                        connectivity.api_reachable.set(true);
                        backoff_ms = 1_000;
                        tracing::debug!("ws connected");

                        loop {
                            let frames = {
                                let mut q = outgoing.lock().unwrap();
                                q.drain(..).collect::<Vec<_>>()
                            };
                            for frame in &frames {
                                let _ = client.send(frame.clone());
                            }

                            match client.next_event().await {
                                Some(frame) => Self::handle_frame(frame, &connectivity),
                                None => {
                                    tracing::debug!("ws event loop ended");
                                    break;
                                }
                            }
                        }

                        connectivity.ws_state.set(WsConnectivity::Disconnected);
                        connectivity.api_reachable.set(false);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "ws connect failed, will retry");
                        connectivity.ws_state.set(WsConnectivity::Disconnected);
                    }
                }

                gloo_timers::future::TimeoutFuture::new(backoff_ms).await;
                backoff_ms = (backoff_ms.saturating_mul(2)).min(30_000);
            }
        });
    }

    /// Dispatch a single server frame to reactive stores.
    fn handle_frame(frame: ServerFrame, connectivity: &ConnectivityState) {
        match frame {
            ServerFrame::AuthOk { user_id } => {
                tracing::debug!(%user_id, "ws auth ok");
                connectivity.ws_state.set(WsConnectivity::Connected);
                connectivity.api_reachable.set(true);
            }
            ServerFrame::Pong => {}
            ServerFrame::NewMessage { group_id, message_id, .. } => {
                tracing::debug!(%group_id, "ws new message");
                // A delivered message means the peer is no longer typing.
                if let Some(typing) = crate::state::message_service::typing_handle() {
                    typing.clear(group_id);
                }
                // Thread-local handles: this runs in the WS event loop where
                // the leptos owner (and use_context) is unavailable.
                // NB: don't bump last_message_at here. It fired unconditionally
                // on every frame (including avatar side-channel messages, which
                // carry no timeline content) with a wall-clock time, churning
                // the open chat panel. The real last-message time/preview is set
                // from message content by load_messages / refresh_incoming below.
                let chats = crate::state::message_service::chats_handle();
                // If this chat is open and the message isn't ours (our own
                // sends are already echoed locally with the server id), pull
                // the new content right away — that is what makes incoming
                // messages (and read receipts turning our checkmarks blue)
                // appear without reopening the chat.
                let is_open = chats
                    .as_ref()
                    .map(|c| c.selected.get_untracked() == Some(group_id))
                    .unwrap_or(false);
                if let Some(svc) = crate::state::message_service::service_handle() {
                    let already_known = svc.messages.by_group.with_untracked(|map| {
                        map.get(&group_id)
                            .is_some_and(|list| list.iter().any(|m| m.id == message_id))
                    });
                    // Skip our own server echo (already in the buffer with the
                    // server id). Otherwise pull the content: the open chat
                    // refreshes inline; a background chat updates its sidebar
                    // preview + unread badge.
                    if !already_known {
                        // A new message resurrects a "deleted" (hidden) chat so
                        // the user doesn't silently miss it.
                        if let Some(ref cs) = chats {
                            cs.unhide(group_id);
                        }
                        spawn_local(async move {
                            if is_open {
                                svc.load_messages(group_id).await;
                            } else {
                                svc.refresh_incoming(group_id).await;
                            }
                            // Notification sound — only for a genuine incoming
                            // content message from SOMEONE ELSE. Control/side-channel
                            // messages (read receipts, avatar updates, edits) are
                            // consumed during the pull and never land in the buffer.
                            // The sender check matters on reload: the buffer is empty
                            // so `already_known` can't suppress our own echoes (e.g.
                            // the read-receipt/avatar re-announce posted during the
                            // initial sync), which otherwise dinged on every refresh.
                            let me = crate::state::message_service::current_user_id();
                            let is_incoming_content = svc.messages.by_group.with_untracked(|map| {
                                map.get(&group_id)
                                    .and_then(|list| list.iter().find(|m| m.id == message_id))
                                    .is_some_and(|m| {
                                        !m.sender_user_id.is_nil() && Some(m.sender_user_id) != me
                                    })
                            });
                            if is_incoming_content {
                                crate::sound::play_message_sound();
                                crate::sound::vibrate_message();
                            }
                        });
                    }
                }
            }
            ServerFrame::NewWelcome { welcome_id, group_id } => {
                tracing::debug!(%welcome_id, %group_id, "ws new welcome");
            }
            ServerFrame::KeyChange { .. } => {}
            ServerFrame::Typing { group_id, user_id, started } => {
                // Reciprocal privacy: only surface peers' typing if the user
                // has the indicator enabled (same toggle that gates sending).
                if typing_indicators_enabled() {
                    if let Some(typing) = crate::state::message_service::typing_handle() {
                        typing.set(group_id, user_id, started);
                    }
                }
            }
            ServerFrame::Error { code, message } => {
                tracing::warn!(%code, message = %message.as_deref().unwrap_or(""), "ws server error");
            }
            ServerFrame::AuthError { .. } => {
                tracing::warn!("ws auth error");
                connectivity.ws_state.set(WsConnectivity::Disconnected);
            }
        }
    }

    /// Queue a frame to be sent.  The event loop will pick it up.
    pub fn send(&self, frame: ClientFrame) {
        self.outgoing.lock().unwrap().push(frame);
    }

    /// Convenience: send a Typing indicator.
    pub fn send_typing(&self, group_id: Uuid, started: bool) {
        self.send(ClientFrame::Typing { group_id, started });
    }

    /// Whether the WebSocket is currently connected.
    pub fn is_connected(&self) -> bool {
        self.connectivity.ws_state.get() == WsConnectivity::Connected
    }
}

impl Default for WsManager {
    fn default() -> Self {
        Self::new()
    }
}

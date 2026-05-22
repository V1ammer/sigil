//! Network connectivity state — online / offline / reconnecting.

use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WsConnectivity {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Tracks API reachability and WebSocket connection state.
/// Populated in C09+ once real network calls are wired.
#[derive(Clone)]
pub struct ConnectivityState {
    pub api_reachable: RwSignal<bool>,
    pub ws_state: RwSignal<WsConnectivity>,
}

impl ConnectivityState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            api_reachable: RwSignal::new(false),
            ws_state: RwSignal::new(WsConnectivity::Disconnected),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.api_reachable.get() && self.ws_state.get() == WsConnectivity::Connected
    }

    /// Helper: set both to connected state.
    pub fn mark_connected(&self) {
        self.api_reachable.set(true);
        self.ws_state.set(WsConnectivity::Connected);
    }

    /// Helper: set both to disconnected state.
    pub fn mark_disconnected(&self) {
        self.api_reachable.set(false);
        self.ws_state.set(WsConnectivity::Disconnected);
    }
}

impl Default for ConnectivityState {
    fn default() -> Self {
        Self::new()
    }
}

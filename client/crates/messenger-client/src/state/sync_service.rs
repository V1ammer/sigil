//! Background sync service — periodic polling for welcomes, messages, and group updates.
//!
//! Runs a lightweight event loop after session restore, complementing the WebSocket
//! real-time channel.  Falls back to polling when WebSocket is disconnected.

use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::state::session::build_api_client;

/// Sync loop interval (seconds).
const SYNC_INTERVAL_SECS: u64 = 30;

/// Number of initial sync passes that run at the fast interval after login,
/// before settling into `SYNC_INTERVAL_SECS`. ~10 passes × 3s ≈ 30s of fast
/// polling — long enough to cover MLS init + the owner's heal-welcome.
const INITIAL_FAST_PASSES: u32 = 10;

/// Interval (seconds) between the initial fast-ramp passes.
const INITIAL_FAST_INTERVAL_SECS: u64 = 3;

/// Background sync service handle.
#[derive(Clone)]
pub struct SyncService {
    running: Arc<AtomicBool>,
}

impl SyncService {
    /// Create a new stopped sync service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the background sync loop.
    ///
    /// Spawns a `spawn_local` loop that runs every `SYNC_INTERVAL_SECS` seconds.
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            tracing::debug!("sync service already running");
            return;
        }

        let running = self.running.clone();
        spawn_local(async move {
            tracing::debug!("sync service started");

            // Fast initial ramp: right after login the MLS runtime may still be
            // initializing and the owner's heal-welcome may land a few seconds
            // later, so a single immediate pass can miss both — and the old code
            // then slept a full 30s, which is exactly the lag the user saw
            // (group names blank, can't send/receive for ~30s). Poll welcomes +
            // chat list every few seconds for the first stretch, then settle
            // into the normal interval.
            let mut passes: u32 = 0;

            while running.load(Ordering::SeqCst) {
                // 1. Sync welcomes
                process_pending_welcomes().await;

                // 1b. Keep the KeyPackage pool topped up (and ensure a
                // last-resort exists) so peers can always start a chat with us.
                if let Some(api) = crate::state::session::build_api_client() {
                    crate::state::message_service::ensure_keypackages(&api).await;
                    // 1c. Pull any member device that's missing from a group we
                    // own into its MLS tree (e.g. a device added to the account
                    // after the chat was created).
                    crate::state::message_service::heal_owned_groups(&api).await;
                }

                // 2. Refresh chat list
                Self::sync_chats().await;

                // 2b. Refresh sidebar previews + unread badges for every chat.
                // The WS keeps these live, but this catches messages that
                // arrived while the app was closed or the socket was down.
                Self::sync_previews().await;

                // 3. Deliver the own avatar to any group that hasn't seen
                // the current one (covers chats created after the avatar
                // was set and welcome joins that bypassed the MLS hook).
                match crate::state::message_service::service_handle() {
                    Some(svc) => svc.ensure_avatar_broadcasts().await,
                    None => web_sys::console::log_1(
                        &"[avatar] ensure skipped: no MessageService handle".into(),
                    ),
                }

                // 4. Sleep for the interval — short for the first few passes
                // (fast ramp), then the steady-state interval.
                passes = passes.saturating_add(1);
                let delay_secs = if passes < INITIAL_FAST_PASSES {
                    INITIAL_FAST_INTERVAL_SECS
                } else {
                    SYNC_INTERVAL_SECS
                };
                gloo_timers::future::TimeoutFuture::new((delay_secs * 1000) as u32).await;
            }

            tracing::debug!("sync service stopped");
        });
    }

    /// Stop the background sync loop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Refresh the chat list from the server.
    async fn sync_chats() {
        let chats = crate::state::message_service::chats_handle();
        if let Some(ref c) = chats {
            if let Some(api) = build_api_client() {
                let _ = c.load_from_server(&api).await;
            }
        }
    }

    /// Refresh sidebar previews and unread badges for all known chats.
    async fn sync_previews() {
        let (Some(chats), Some(svc)) = (
            crate::state::message_service::chats_handle(),
            crate::state::message_service::service_handle(),
        ) else {
            return;
        };
        // Never refresh the open chat here: it's kept live by the WS, and
        // re-inserting its messages would rebuild the whole list and yank the
        // scroll position. The safety net is only for background chats.
        let open = chats.selected.get_untracked();
        let group_ids: Vec<_> = chats
            .chats
            .get_untracked()
            .into_iter()
            .map(|c| c.group_id)
            .filter(|gid| Some(*gid) != open)
            .collect();
        for group_id in group_ids {
            svc.refresh_incoming(group_id).await;
        }
    }
}

impl Default for SyncService {
    fn default() -> Self {
        Self::new()
    }
}

/// Pull pending welcomes from the server, join each group, and ack on success.
///
/// Module-level (not tied to a `SyncService` instance) so the WebSocket
/// `NewWelcome` push can join immediately instead of waiting for the next
/// 30s poll — that wait was why a freshly QR-provisioned device (or a fresh
/// web login) saw group names and could send/receive only ~30s after login:
/// joining is the gate for the GroupName application message and for owning
/// MLS state.
pub async fn process_pending_welcomes() {
    let api = match build_api_client() {
        Some(c) => c,
        None => return,
    };

    // Thread-local handles, not use_context — this runs in a detached
    // spawn_local loop where the leptos owner may be gone.
    let msg_svc = crate::state::message_service::service_handle();
    let chats = crate::state::message_service::chats_handle();

    match api.list_welcomes(None).await {
        Ok(resp) if !resp.welcomes.is_empty() => {
            tracing::debug!(count = resp.welcomes.len(), "processing welcomes");

            let mls_ready = msg_svc
                .as_ref()
                .map_or(false, |_| crate::state::message_service::is_mls_initialized());

            for welcome in &resp.welcomes {
                // Only ack (drop) a welcome once we've ACTUALLY joined. If MLS
                // isn't ready yet, or the join fails, leave it on the server so
                // the next sync retries — acking a failed join silently loses
                // the group with no way to recover.
                let mut joined = false;
                if mls_ready {
                    if let Some(ref svc) = msg_svc {
                        // The MlsRuntime is thread-local — we join via the cached runtime
                        joined = crate::state::message_service::join_welcome(
                            welcome.id,
                            &welcome.welcome_ciphertext,
                        )
                        .await;
                        if joined {
                            tracing::debug!(welcome_id = ?welcome.id, "joined via welcome");
                            // Introduce ourselves: deliver our avatar to
                            // the freshly joined group.
                            let _ = svc.broadcast_avatar(welcome.group_id).await;
                        } else {
                            tracing::warn!(welcome_id = ?welcome.id, "failed to join via welcome");
                        }
                    }
                }

                if joined {
                    let _ = api.ack_welcome(welcome.id).await;
                }
            }

            // Refresh chat list after processing welcomes
            if let Some(ref c) = chats {
                if let Some(api) = build_api_client() {
                    let _ = c.load_from_server(&api).await;
                }
            }
        }
        Ok(_) => {} // no welcomes
        Err(e) => {
            tracing::warn!(error = %e, "failed to list welcomes");
        }
    }
}

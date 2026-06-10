//! Invite deep-link screen.
//!
//! Handles URLs of the form `/invite?token=...&server=...`. The `server`
//! parameter is optional — when absent we fall back to the page origin,
//! which is the common case for the web build where the API and the
//! static frontend live behind the same host. On success we persist the
//! server URL just like `ConnectScreen` and forward to
//! `/register?token=...` so the existing redeem flow takes over.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_query};
use leptos_router::params::Params;
use leptos_router::NavigateOptions;
use messenger_core::api::client::ApiClient;

use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::session::{persist_server_url, use_session, SessionState};
use crate::t;

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct InviteQuery {
    pub token: Option<String>,
    pub server: Option<String>,
}

impl Params for InviteQuery {
    fn from_map(
        map: &leptos_router::params::ParamsMap,
    ) -> Result<Self, leptos_router::params::ParamsError> {
        Ok(Self {
            token: map.get("token"),
            server: map.get("server"),
        })
    }
}

/// Default server URL when no `?server=` query param is supplied —
/// the origin the SPA was loaded from.
fn current_origin() -> Option<String> {
    let origin = web_sys::window()?.location().origin().ok()?;
    if origin.is_empty() || origin == "null" {
        None
    } else {
        Some(origin)
    }
}

#[must_use]
#[component]
pub fn InviteScreen() -> impl IntoView {
    let session = use_session();
    let navigate = use_navigate();
    let notifications =
        use_context::<NotificationsState>().expect("NotificationsState must be provided");

    let query = use_query::<InviteQuery>();
    let error = RwSignal::new(Option::<String>::None);

    // Already authenticated → straight to chats.
    let nav_auth = navigate.clone();
    let sess_auth = session.clone();
    Effect::new(move |_| {
        if sess_auth.is_authenticated() {
            nav_auth(
                "/chats",
                NavigateOptions {
                    replace: true,
                    ..Default::default()
                },
            );
        }
    });

    // Kick off the auto-configure flow once on mount.
    let nav = navigate.clone();
    let sess = session.clone();
    let notif = notifications.clone();
    Effect::new(move |prev: Option<()>| {
        if prev.is_some() {
            return;
        }
        let (token, server_override) = query.with(|q| {
            q.as_ref()
                .ok()
                .map(|q| (q.token.clone(), q.server.clone()))
                .unwrap_or((None, None))
        });

        let token = match token {
            Some(t) if !t.is_empty() => t,
            _ => {
                error.set(Some(t!("token.error.invalid").to_string()));
                return;
            }
        };

        let url = server_override
            .filter(|s| !s.is_empty())
            .or_else(current_origin)
            .unwrap_or_default();

        if url.is_empty()
            || !(url.starts_with("http://") || url.starts_with("https://"))
        {
            error.set(Some(t!("connect.error.invalid").to_string()));
            return;
        }

        let nav = nav.clone();
        let sess = sess.clone();
        let notif = notif.clone();
        spawn_local(async move {
            let client = ApiClient::new(url.clone());
            match client.server_info().await {
                Ok(info) => {
                    persist_server_url(&url);
                    if let Ok(local) = messenger_storage::init_storage("default").await {
                        let _ = local
                            .set_setting(
                                "server_pubkey_hex",
                                &hex::encode(&info.server_identity_public_key),
                            )
                            .await;
                        let _ = local
                            .set_setting(
                                "mls_ciphersuite",
                                &info.mls_ciphersuite.to_string(),
                            )
                            .await;
                    }
                    sess.state
                        .set(SessionState::ServerConfigured { url: url.clone() });

                    let target = format!("/register?token={token}");
                    nav(
                        &target,
                        NavigateOptions {
                            replace: true,
                            ..Default::default()
                        },
                    );
                }
                Err(e) => {
                    let msg = format!("{}: {e}", t!("error.network"));
                    notif.push(ToastKind::Error, &msg);
                    error.set(Some(msg));
                }
            }
        });
    });

    view! {
        <div class="flex h-screen-safe flex-col items-center justify-center bg-background p-4 overflow-hidden">
            <div class="w-full max-w-md space-y-6 text-center">
                {move || match error.get() {
                    Some(e) => view! {
                        <div class="space-y-4">
                            <h1 class="text-2xl font-semibold tracking-tight text-foreground">
                                {t!("invite.failed")}
                            </h1>
                            <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                <p class="text-sm">{e}</p>
                            </div>
                            <a
                                href="/"
                                class="inline-flex h-10 items-center justify-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                            >
                                {t!("invite.back")}
                            </a>
                        </div>
                    }.into_any(),
                    None => view! {
                        <div class="space-y-4">
                            <div class="mx-auto h-10 w-10 rounded-full border-4 border-muted border-t-primary animate-spin"/>
                            <p class="text-sm text-muted-foreground">{t!("invite.connecting")}</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use messenger_core::api::client::ApiClient;
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::session::{use_session, persist_server_url, SessionState};
use crate::t;

#[must_use]
#[component]
pub fn ConnectScreen() -> impl IntoView {
    let session = use_session();
    let navigate = use_navigate();
    let notifications = use_context::<NotificationsState>()
        .expect("NotificationsState must be provided");

    let server_address = RwSignal::new(String::new());
    let is_loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let show_help = RwSignal::new(false);

    // If already authenticated (session restored before mount), redirect to chats.
    let nav = navigate.clone();
    let session_for_effect = session.clone();
    Effect::new(move |_| {
        if session_for_effect.is_authenticated() {
            nav("/chats", NavigateOptions { replace: true, ..Default::default() });
        }
    });

    // Pre-fill from local storage
    spawn_local(async move {
        if let Ok(local) = messenger_storage::init_storage("default").await {
            if let Ok(Some(saved_url)) = local.get_setting("server_url").await {
                server_address.set(saved_url);
            }
        }
    });

    let on_connect = move || {
        let url = server_address.get().trim().to_string();

        // Validate URL
        if url.is_empty() {
            error.set(Some(t!("connect.error.invalid")));
            return;
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            error.set(Some(t!("connect.error.invalid")));
            return;
        }

        is_loading.set(true);
        error.set(None);

        let nav = navigate.clone();
        let sess = session.clone();
        let notif = notifications.clone();
        spawn_local(async move {
            let client = ApiClient::new(url.clone());
            match client.server_info().await {
                Ok(info) => {
                    // Save server config to local store
                    if let Ok(local) = messenger_storage::init_storage("default").await {
                        let _ = local.set_setting("server_url", &url).await;
                        let _ = local
                            .set_setting(
                                "server_pubkey_hex",
                                &hex::encode(&info.server_identity_public_key),
                            )
                            .await;
                        let _ = local
                            .set_setting("mls_ciphersuite", &info.mls_ciphersuite.to_string())
                            .await;
                    }

                    sess.state.set(SessionState::ServerConfigured { url: url.clone() });
                    persist_server_url(&url);

                    nav("/login", Default::default());
                }
                Err(e) => {
                    notif.push(ToastKind::Error, format!("{}: {e}", t!("error.network")));
                    error.set(Some(format!("{e}")));
                    is_loading.set(false);
                }
            }
        });
    };

    let is_disabled = move || is_loading.get() || server_address.get().trim().is_empty();

    let on_connect_clone = on_connect.clone();
    let on_connect_handler = move |_| on_connect_clone();

    view! {
        <div class="flex h-screen-safe flex-col items-center justify-center bg-background p-4 overflow-hidden">
            <div class="w-full max-w-md space-y-8">
                <div class="flex flex-col items-center space-y-4 text-center">
                    <div class="flex h-16 w-16 items-center justify-center rounded-2xl bg-primary">
                        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary-foreground"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                    </div>
                    <div class="space-y-2">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">{t!("app.name")}</h1>
                        <p class="text-sm text-muted-foreground">{t!("app.description")}</p>
                    </div>
                </div>

                <div class="space-y-4">
                    <div class="space-y-2">
                        <label class="text-sm font-medium text-foreground">{t!("connect.title")}</label>
                        <input
                            type="url"
                            placeholder={t!("connect.placeholder")}
                            class="flex h-12 w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                            disabled=is_loading
                            on:input=move |ev| server_address.set(event_target_value(&ev))
                            prop:value=server_address
                            on:keydown=move |ev| {
                                if ev.key() == "Enter" && !is_loading.get() { on_connect(); }
                            }
                        />
                    </div>

                    {move || error.get().map(|e| {
                        view! {
                            <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                <p class="text-sm">{e}</p>
                            </div>
                        }
                    })}

                    <button
                        class="inline-flex h-12 w-full items-center justify-center rounded-md bg-primary text-sm font-medium text-primary-foreground ring-offset-background transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
                        disabled=is_disabled
                        on:click=on_connect_handler
                    >
                        {move || if is_loading.get() {
                            t!("loading")
                        } else {
                            t!("connect.button")
                        }}
                    </button>
                </div>

                <div class="flex justify-center">
                    <button
                        class="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
                        on:click=move |_| show_help.set(true)
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
                        {t!("connect.whatsThis")}
                    </button>
                </div>
            </div>

            {move || if show_help.get() {
                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center">
                        <div class="fixed inset-0 bg-black/50" on:click=move |_| show_help.set(false)/>
                        <div class="relative z-50 w-full max-w-md rounded-lg border bg-background p-6 shadow-lg">
                            <h2 class="text-lg font-semibold">{t!("connect.help.title")}</h2>
                            <p class="text-sm text-muted-foreground mt-2">{t!("connect.help.description")}</p>
                            <div class="flex justify-end pt-4">
                                <button
                                    class="inline-flex items-center justify-center rounded-md bg-primary h-10 px-4 py-2 text-sm font-medium text-primary-foreground"
                                    on:click=move |_| show_help.set(false)
                                >{t!("close")}</button>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
        </div>
    }
}

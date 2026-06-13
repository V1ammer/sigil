use std::sync::Arc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use crate::components::switch::Switch;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::components::button::{Button, ButtonVariant};
use crate::components::alert_dialog::{AlertDialog, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction};
use crate::state::settings::SettingsState;
use crate::state::session::use_session;
use crate::session::restore::clear_persisted_session;
use crate::t;

/// Privacy settings — read receipts, typing indicators, history, block list, cache.
#[must_use]
#[component]
pub fn PrivacySettings() -> impl IntoView {
    let settings = use_context::<SettingsState>()
        .expect("SettingsState must be provided via provide_context()");
    let session = use_session();
    let navigate = use_navigate();

    // Show clear-cache confirmation dialog
    let show_clear_cache = RwSignal::new(false);

    let on_clear_cache_confirm = {
        let navigate = navigate.clone();
        move || {
            show_clear_cache.set(false);

            // a. Clear persisted session (server_url etc.)
            clear_persisted_session();

            // b. Wipe all SettingsState persistent keys
            SettingsState::wipe_all();

            // c. Clear individual localStorage keys
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
            {
                let _ = storage.remove_item("messenger_device_id");
                let _ = storage.remove_item("messenger_device_signing_secret");
                let _ = storage.remove_item("messenger_user_id");
                let _ = storage.remove_item("messenger_identity");
                let _ = storage.remove_item("messenger_user_role");
                let _ = storage.remove_item("messenger_server_url");
            }

            // d. Optional: try to clear IndexedDB (web) — best-effort
            spawn_local(async move {
                if let Ok(store) = messenger_storage::init_storage("default").await {
                    // Attempt to close/drop the store by releasing it;
                    // a full IndexedDB delete would need a separate web API call.
                    // The store handle is dropped at end of scope.
                    drop(store);
                }
                // On the web platform we can also delete the entire IndexedDB database.
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(window) = web_sys::window() {
                        if let Ok(Some(idb_factory)) = window.indexed_db() {
                            let _ = idb_factory.delete_database("messenger");
                        }
                    }
                }
            });

            // e. Set session state to Disconnected
            session.state.set(crate::state::session::SessionState::Disconnected);

            // f. Navigate to root
            _ = navigate("/", Default::default());
        }
    };

    let on_clear_cache_confirm = Arc::new(on_clear_cache_confirm);

    // Wrap non-Copy captures in RwSignal so the outer view! closure can capture them by Copy.
    let clear_cache_signal: RwSignal<Option<Arc<dyn Fn() + Send + Sync + 'static>>> =
        RwSignal::new(Some(on_clear_cache_confirm));
    let settings_signal = RwSignal::new(settings);

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.privacy.title")}</h3>
                <p class="text-sm text-muted-foreground">{t!("settings.privacy.description")}</p>
            </div>

            <Separator />

            <div class="space-y-4">
                // Read receipts toggle
                <div class="flex items-center justify-between">
                    <div class="space-y-0.5">
                        <Label class="text-foreground">{t!("settings.privacy.readReceipts")}</Label>
                        <p class="text-xs text-muted-foreground">{t!("settings.privacy.readReceiptsDesc")}</p>
                    </div>
                    <Switch
                        checked=Signal::derive(move || settings_signal.get().read_receipts.get())
                        on_change=Box::new(move |v| settings_signal.get().read_receipts.set(v))
                    />
                </div>

                // Typing indicators toggle
                <div class="flex items-center justify-between">
                    <div class="space-y-0.5">
                        <Label class="text-foreground">{t!("settings.privacy.typingIndicators")}</Label>
                        <p class="text-xs text-muted-foreground">{t!("settings.privacy.typingIndicatorsDesc")}</p>
                    </div>
                    <Switch
                        checked=Signal::derive(move || settings_signal.get().typing_indicators.get())
                        on_change=Box::new(move |v| settings_signal.get().typing_indicators.set(v))
                    />
                </div>

                <Separator />

                // History retention
                <div class="space-y-2">
                    <Label class="text-foreground">{t!("settings.privacy.historyRetention")}</Label>
                    <Select
                        value=Signal::derive(move || settings_signal.get().history_retention.get())
                        on_change=Box::new({
                            let s = settings_signal.get().history_retention;
                            move |v: String| s.set(v)
                        })
                        class="w-full max-w-xs"
                    >
                        <SelectOption value=String::from("forever")>{t!("settings.privacy.historyForever")}</SelectOption>
                        <SelectOption value=String::from("year")>{t!("settings.privacy.historyYear")}</SelectOption>
                        <SelectOption value=String::from("month")>{t!("settings.privacy.historyMonth")}</SelectOption>
                        <SelectOption value=String::from("week")>{t!("settings.privacy.historyWeek")}</SelectOption>
                    </Select>
                    <p class="text-xs text-muted-foreground">{t!("settings.privacy.historyHint")}</p>
                </div>

                // Auto-delete messages
                <div class="space-y-2">
                    <Label class="text-foreground">{t!("settings.privacy.autoDelete")}</Label>
                    <Select
                        value=Signal::derive(move || settings_signal.get().auto_delete.get())
                        on_change=Box::new({
                            let s = settings_signal.get().auto_delete;
                            move |v: String| s.set(v)
                        })
                        class="w-full max-w-xs"
                    >
                        <SelectOption value=String::from("off")>{t!("settings.privacy.autoDeleteOff")}</SelectOption>
                        <SelectOption value=String::from("24h")>{t!("settings.privacy.autoDelete24h")}</SelectOption>
                        <SelectOption value=String::from("7d")>{t!("settings.privacy.autoDelete7d")}</SelectOption>
                        <SelectOption value=String::from("30d")>{t!("settings.privacy.autoDelete30d")}</SelectOption>
                        <SelectOption value=String::from("90d")>{t!("settings.privacy.autoDelete90d")}</SelectOption>
                    </Select>
                    <p class="text-xs text-muted-foreground">{t!("settings.privacy.autoDeleteHint")}</p>
                </div>

                <Separator />

                // Auto-download files toggle + size threshold
                <div class="flex items-center justify-between">
                    <div class="space-y-0.5">
                        <Label class="text-foreground">{t!("settings.privacy.autoDownloadFiles")}</Label>
                        <p class="text-xs text-muted-foreground">{t!("settings.privacy.autoDownloadFilesDesc")}</p>
                    </div>
                    <Switch
                        checked=Signal::derive(move || settings_signal.get().auto_download_files.get())
                        on_change=Box::new(move |v| settings_signal.get().auto_download_files.set(v))
                    />
                </div>

                {move || if settings_signal.get().auto_download_files.get() {
                    view! {
                        <div class="space-y-2">
                            <Label class="text-foreground">{t!("settings.privacy.autoDownloadMaxSize")}</Label>
                            <Select
                                value=Signal::derive(move || settings_signal.get().auto_download_max_mb.get())
                                on_change=Box::new({
                                    let s = settings_signal.get().auto_download_max_mb;
                                    move |v: String| s.set(v)
                                })
                                class="w-full max-w-xs"
                            >
                                <SelectOption value=String::from("1")>{"1 MB"}</SelectOption>
                                <SelectOption value=String::from("5")>{"5 MB"}</SelectOption>
                                <SelectOption value=String::from("10")>{"10 MB"}</SelectOption>
                                <SelectOption value=String::from("50")>{"50 MB"}</SelectOption>
                                <SelectOption value=String::from("100")>{"100 MB"}</SelectOption>
                            </Select>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }}

                <Separator />

                // Clear local cache — destructive button with confirmation
                <div class="space-y-3">
                    <h4 class="text-sm font-medium text-foreground">{t!("settings.privacy.clearCache")}</h4>
                    <p class="text-xs text-muted-foreground">
                        {t!("settings.privacy.clearCacheWarning")}
                    </p>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Destructive)
                        on_click=Box::new(move |_| show_clear_cache.set(true))
                    >
                        {t!("settings.privacy.clearCache")}
                    </Button>
                </div>
            </div>
        </div>

        // Clear cache confirmation dialog
        <AlertDialog
            is_open=show_clear_cache
            on_close=Box::new(move || show_clear_cache.set(false))
        >
            <AlertDialogHeader>
                <AlertDialogTitle>{t!("settings.privacy.clearCache")}</AlertDialogTitle>
                <AlertDialogDescription>
                    {t!("settings.privacy.clearCacheWarning")}
                </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
                <AlertDialogCancel on_click=Box::new(move || show_clear_cache.set(false))>
                    {t!("common.cancel")}
                </AlertDialogCancel>
                <AlertDialogAction
                    class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                    on_click=Box::new({
                        let sig = clear_cache_signal;
                        move || {
                            if let Some(ref cb) = sig.get_untracked() {
                                cb();
                            }
                        }
                    })
                >
                    {t!("settings.privacy.clearCacheAction")}
                </AlertDialogAction>
            </AlertDialogFooter>
        </AlertDialog>
    }
}

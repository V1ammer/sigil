//! Voice transcription settings — list of Whisper models with download / delete
//! and a single "active" choice. The actual model files live on disk (managed
//! by the Tauri side); this UI just orchestrates the commands.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::button::{Button, ButtonVariant};
use crate::state::NotificationsState;
use crate::state::notifications::ToastKind;
use crate::t;
use crate::tauri_bridge::{
    is_tauri_context, transcription_delete_model, transcription_download_model,
    transcription_get_active, transcription_list_downloaded, transcription_list_models,
    transcription_set_active, WhisperModelInfo,
};

#[must_use]
#[component]
pub fn VoiceSettings() -> impl IntoView {
    let notifications = use_context::<NotificationsState>();

    let models: RwSignal<Vec<WhisperModelInfo>> = RwSignal::new(Vec::new());
    let downloaded: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let active: RwSignal<Option<String>> = RwSignal::new(None);
    let downloading: RwSignal<Option<String>> = RwSignal::new(None);
    let in_tauri = is_tauri_context();

    if in_tauri {
        spawn_local(async move {
            if let Ok(list) = transcription_list_models().await {
                models.set(list);
            }
            if let Ok(list) = transcription_list_downloaded().await {
                downloaded.set(list);
            }
            if let Ok(a) = transcription_get_active().await {
                active.set(a);
            }
        });
    }

    let refresh = move || {
        spawn_local(async move {
            if let Ok(list) = transcription_list_downloaded().await {
                downloaded.set(list);
            }
            if let Ok(a) = transcription_get_active().await {
                active.set(a);
            }
        });
    };

    if !in_tauri {
        return view! {
            <div class="flex flex-col items-center justify-center py-16 text-center">
                <h3 class="text-lg font-medium text-foreground">{t!("settings.voice.title")}</h3>
                <p class="text-sm text-muted-foreground mt-2 max-w-sm">
                    {t!("settings.voice.unavailableWeb")}
                </p>
            </div>
        }.into_any();
    }

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.voice.title")}</h3>
                <p class="text-sm text-muted-foreground mt-1">
                    {t!("settings.voice.description")}
                </p>
            </div>

            <div class="rounded-md border border-border bg-muted/30 p-3 text-xs text-muted-foreground">
                {t!("settings.voice.privacyNote")}
            </div>

            <div class="space-y-2">
                {move || {
                    let downloaded_set = downloaded.get();
                    let active_id = active.get();
                    let downloading_id = downloading.get();
                    models.get().into_iter().map(|m| {
                        let id = m.id.clone();
                        let id_for_dl = id.clone();
                        let id_for_select = id.clone();
                        let id_for_delete = id.clone();
                        let id_for_state = id.clone();
                        let is_downloaded = downloaded_set.iter().any(|d| d == &m.id);
                        let is_active = active_id.as_deref() == Some(&m.id);
                        let is_downloading = downloading_id.as_deref() == Some(&m.id);
                        let notifications_dl = notifications.clone();
                        let notifications_sel = notifications.clone();
                        let on_download = move |_| {
                            let mid = id_for_dl.clone();
                            downloading.set(Some(mid.clone()));
                            let notif = notifications_dl.clone();
                            spawn_local(async move {
                                let res = transcription_download_model(&mid).await;
                                downloading.set(None);
                                match res {
                                    Ok(_) => {
                                        if let Ok(list) = transcription_list_downloaded().await {
                                            downloaded.set(list);
                                        }
                                        if let Some(n) = notif.as_ref() {
                                            n.push(ToastKind::Success, t!("settings.voice.downloaded"));
                                        }
                                    }
                                    Err(e) => {
                                        if let Some(n) = notif.as_ref() {
                                            n.push(ToastKind::Error, format!("{}: {e}", t!("settings.voice.downloadFailed")));
                                        }
                                    }
                                }
                            });
                        };
                        let on_select = move |_| {
                            let mid = id_for_select.clone();
                            let notif = notifications_sel.clone();
                            spawn_local(async move {
                                if let Err(e) = transcription_set_active(&mid).await {
                                    if let Some(n) = notif.as_ref() {
                                        n.push(ToastKind::Error, e);
                                    }
                                } else if let Ok(a) = transcription_get_active().await {
                                    active.set(a);
                                }
                            });
                        };
                        let on_delete = move |_| {
                            let mid = id_for_delete.clone();
                            spawn_local(async move {
                                let _ = transcription_delete_model(&mid).await;
                                if let Ok(list) = transcription_list_downloaded().await {
                                    downloaded.set(list);
                                }
                                if let Ok(a) = transcription_get_active().await {
                                    active.set(a);
                                }
                            });
                        };
                        let _ = id_for_state;
                        view! {
                            <div class="flex items-center justify-between gap-3 rounded-lg border border-border p-3">
                                <div class="min-w-0 flex-1">
                                    <p class="text-sm font-medium text-foreground">{m.display_name.clone()}</p>
                                    <p class="text-xs text-muted-foreground">
                                        {format!("{} MB · RAM ≈ {} MB · {}",
                                            m.size_mb,
                                            m.ram_mb,
                                            if m.multilingual { t!("settings.voice.multilingual") } else { t!("settings.voice.englishOnly") }
                                        )}
                                    </p>
                                </div>
                                <div class="flex items-center gap-2 shrink-0">
                                    {if is_downloading {
                                        view! {
                                            <span class="text-xs text-muted-foreground">{t!("settings.voice.downloading")}</span>
                                        }.into_any()
                                    } else if is_downloaded {
                                        let label = if is_active {
                                            t!("settings.voice.activeBadge")
                                        } else {
                                            t!("settings.voice.select")
                                        };
                                        view! {
                                            <Button
                                                variant=Signal::derive(move || if is_active { ButtonVariant::Secondary } else { ButtonVariant::Default })
                                                disabled=Signal::derive(move || is_active)
                                                on_click=Box::new(on_select)
                                            >
                                                {label}
                                            </Button>
                                            <Button
                                                variant=Signal::derive(move || ButtonVariant::Outline)
                                                on_click=Box::new(on_delete)
                                            >
                                                {t!("settings.voice.delete")}
                                            </Button>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <Button
                                                variant=Signal::derive(move || ButtonVariant::Default)
                                                on_click=Box::new(on_download)
                                            >
                                                {t!("settings.voice.download")}
                                            </Button>
                                        }.into_any()
                                    }}
                                </div>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
        </div>
    }.into_any()
}

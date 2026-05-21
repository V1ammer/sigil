use leptos::prelude::*;
use crate::components::button::{Button, ButtonVariant};
use crate::components::badge::Badge;
use crate::components::separator::Separator;
use crate::i18n::{Language, t, format_date};
use crate::mock::{mock_devices, Device};

/// Device icon helper.
fn device_icon(device_type: &str) -> &'static str {
    match device_type {
        "smartphone" => "smartphone",
        "tablet" => "tablet",
        _ => "monitor",
    }
}

/// Single device row component.
#[must_use]
#[component]
fn DeviceRow(
    device: Device,
    lang: RwSignal<Language>,
) -> impl IntoView {
    let is_current = device.is_current;
    view! {
        <div class="flex items-center justify-between rounded-lg border p-4">
            <div class="flex items-center gap-3">
                <div class="flex h-10 w-10 items-center justify-center rounded-full bg-muted">
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
                        {move || match device.device_type.as_str() {
                            "smartphone" => view! {
                                <rect x="5" y="2" width="14" height="20" rx="2" ry="2" />
                                <line x1="12" y1="18" x2="12.01" y2="18" />
                            }.into_any(),
                            "tablet" => view! {
                                <rect x="4" y="2" width="16" height="20" rx="2" ry="2" />
                                <line x1="12" y1="18" x2="12.01" y2="18" />
                            }.into_any(),
                            _ => view! {
                                <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                                <line x1="8" y1="21" x2="16" y2="21" />
                                <line x1="12" y1="17" x2="12" y2="21" />
                            }.into_any(),
                        }}
                    </svg>
                </div>
                <div class="space-y-0.5">
                    <div class="flex items-center gap-2">
                        <span class="text-sm font-medium text-foreground">{device.name.clone()}</span>
                        {move || if is_current {
                            view! {
                                <Badge variant=String::from("secondary")>
                                    {t(lang.get(), "settings.devices.currentDevice")}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                        {move || if device.is_new {
                            view! {
                                <Badge variant=String::from("default")>
                                    {t(lang.get(), "settings.devices.newDevice")}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                    </div>
                    <p class="text-xs text-muted-foreground">
                        {t(lang.get(), "settings.devices.lastActive")}
                        {format_date(device.last_active, lang.get())}
                    </p>
                    <p class="text-xs text-muted-foreground">
                        {t(lang.get(), "settings.devices.added")}
                        {format_date(device.added_at, lang.get())}
                    </p>
                </div>
            </div>
            {move || if is_current {
                view! {}.into_any()
            } else {
                view! {
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Ghost)
                        size=Signal::derive(move || crate::components::button::ButtonSize::Sm)
                        class="text-destructive hover:text-destructive"
                    >
                        {t(lang.get(), "settings.devices.revoke")}
                    </Button>
                }.into_any()
            }}
        </div>
    }
}

/// Devices settings — list of devices with current device badge.
#[must_use]
#[component]
pub fn DevicesSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let devices = mock_devices();

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.devices.title")}</h3>
                    <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.devices.description")}</p>
                </div>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Outline)
                    size=Signal::derive(move || crate::components::button::ButtonSize::Sm)
                >
                    {t(lang.get(), "settings.devices.addDevice")}
                </Button>
            </div>

            <Separator />

            <div class="space-y-3">
                {devices.into_iter().map(|d| {
                    let device = d;
                    view! {
                        <DeviceRow device=device lang=lang />
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

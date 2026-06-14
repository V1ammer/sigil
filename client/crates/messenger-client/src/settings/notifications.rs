use leptos::prelude::*;
use crate::components::switch::Switch;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::state::settings::SettingsState;
use crate::t;

/// Notification setting row component.
#[must_use]
#[component]
fn NotificationRow(
    label: String,
    description: String,
    checked: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between">
            <div class="space-y-0.5">
                <Label class="text-foreground">{label}</Label>
                <p class="text-xs text-muted-foreground">{description}</p>
            </div>
            <Switch
                checked=Signal::derive(move || checked.get())
                on_change=Box::new(move |v| checked.set(v))
            />
        </div>
    }
}

/// Notifications settings — enable, sound, vibration, preview, filter, quiet hours.
#[must_use]
#[component]
pub fn NotificationsSettings() -> impl IntoView {
    let settings = use_context::<SettingsState>().expect("SettingsState must be provided");

    let enable_notifications = settings.notifications_enabled;
    let sound_enabled = settings.notification_sound;
    let vibration = settings.notification_vibration;
    let preview_enabled = settings.message_preview;
    let notification_filter = settings.notification_filter;
    let quiet_hours_enabled = settings.quiet_hours_enabled;

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.notifications.title")}</h3>
                <p class="text-sm text-muted-foreground">{t!("settings.notifications.description")}</p>
            </div>

            <Separator />

            <div class="space-y-4">
                <NotificationRow
                    label={t!("settings.notifications.enable")}
                    description={t!("settings.notifications.enableDesc")}
                    checked=enable_notifications
                />
                <NotificationRow
                    label={t!("settings.notifications.sound")}
                    description={t!("settings.notifications.soundDesc")}
                    checked=sound_enabled
                />
                <NotificationRow
                    label={t!("settings.notifications.vibration")}
                    description={t!("settings.notifications.vibrationDesc")}
                    checked=vibration
                />
                <NotificationRow
                    label={t!("settings.notifications.preview")}
                    description={t!("settings.notifications.previewDesc")}
                    checked=preview_enabled
                />
            </div>

            <Separator />

            // Notification filter
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.notifications.filter")}</Label>
                <Select
                    value=Signal::derive(move || notification_filter.get())
                    on_change=Box::new(move |v| notification_filter.set(v))
                    class="w-full max-w-xs"
                >
                    <SelectOption value=String::from("all")>{t!("settings.notifications.filterAll")}</SelectOption>
                    <SelectOption value=String::from("mentions")>{t!("settings.notifications.filterMentions")}</SelectOption>
                    <SelectOption value=String::from("none")>{t!("settings.notifications.filterNone")}</SelectOption>
                </Select>
            </div>

            <Separator />

            // Do Not Disturb — immediate toggle: while on, no notification sounds.
            <div class="flex items-center justify-between gap-4">
                <div class="space-y-0.5">
                    <Label class="text-foreground">{t!("settings.notifications.quietHours")}</Label>
                    <p class="text-xs text-muted-foreground">{t!("settings.notifications.quietHoursDesc")}</p>
                </div>
                <Switch
                    checked=Signal::derive(move || quiet_hours_enabled.get())
                    on_change=Box::new(move |v| quiet_hours_enabled.set(v))
                />
            </div>
        </div>
    }
}

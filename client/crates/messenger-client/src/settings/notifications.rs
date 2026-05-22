use leptos::prelude::*;
use crate::components::switch::Switch;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::i18n::{Language, t};

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

/// Notifications settings — enable, sound, preview, read receipts.
#[must_use]
#[component]
pub fn NotificationsSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let enable_notifications = RwSignal::new(true);
    let sound_enabled = RwSignal::new(true);
    let preview_enabled = RwSignal::new(true);
    let read_receipts = RwSignal::new(true);

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.notifications.title")}</h3>
                <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.notifications.description")}</p>
            </div>

            <Separator />

            <div class="space-y-4">
                <NotificationRow
                    label={t(lang.get(), "settings.notifications.enable")}
                    description={t(lang.get(), "settings.notifications.enableDesc")}
                    checked=enable_notifications
                />
                <NotificationRow
                    label={t(lang.get(), "settings.notifications.sound")}
                    description={t(lang.get(), "settings.notifications.soundDesc")}
                    checked=sound_enabled
                />
                <NotificationRow
                    label={t(lang.get(), "settings.notifications.preview")}
                    description={t(lang.get(), "settings.notifications.previewDesc")}
                    checked=preview_enabled
                />
                <NotificationRow
                    label={t(lang.get(), "settings.notifications.readReceipts")}
                    description={t(lang.get(), "settings.notifications.readReceiptsDesc")}
                    checked=read_receipts
                />
            </div>
        </div>
    }
}

use leptos::prelude::*;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::i18n::{Language, t};

/// Privacy settings — history retention, auto-delete.
#[must_use]
#[component]
pub fn PrivacySettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let history_retention = RwSignal::new("forever".to_string());
    let auto_delete = RwSignal::new("off".to_string());

    let on_history_change = Box::new(move |v: String| history_retention.set(v));
    let on_auto_delete_change = Box::new(move |v: String| auto_delete.set(v));

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.privacy.title")}</h3>
                <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.privacy.description")}</p>
            </div>

            <Separator />

            <div class="space-y-4">
                // History retention
                <div class="space-y-2">
                    <Label class="text-foreground">{t(lang.get(), "settings.privacy.historyRetention")}</Label>
                    <Select
                        on_change=on_history_change
                        class="w-full max-w-xs"
                    >
                        <SelectOption value=String::from("forever")>{t(lang.get(), "settings.privacy.historyForever")}</SelectOption>
                        <SelectOption value=String::from("year")>{t(lang.get(), "settings.privacy.historyYear")}</SelectOption>
                        <SelectOption value=String::from("month")>{t(lang.get(), "settings.privacy.historyMonth")}</SelectOption>
                        <SelectOption value=String::from("week")>{t(lang.get(), "settings.privacy.historyWeek")}</SelectOption>
                    </Select>
                    <p class="text-xs text-muted-foreground">{t(lang.get(), "settings.privacy.historyHint")}</p>
                </div>

                // Auto-delete messages
                <div class="space-y-2">
                    <Label class="text-foreground">{t(lang.get(), "settings.privacy.autoDelete")}</Label>
                    <Select
                        on_change=on_auto_delete_change
                        class="w-full max-w-xs"
                    >
                        <SelectOption value=String::from("off")>{t(lang.get(), "settings.privacy.autoDeleteOff")}</SelectOption>
                        <SelectOption value=String::from("24h")>{t(lang.get(), "settings.privacy.autoDelete24h")}</SelectOption>
                        <SelectOption value=String::from("7d")>{t(lang.get(), "settings.privacy.autoDelete7d")}</SelectOption>
                        <SelectOption value=String::from("30d")>{t(lang.get(), "settings.privacy.autoDelete30d")}</SelectOption>
                        <SelectOption value=String::from("90d")>{t(lang.get(), "settings.privacy.autoDelete90d")}</SelectOption>
                    </Select>
                    <p class="text-xs text-muted-foreground">{t(lang.get(), "settings.privacy.autoDeleteHint")}</p>
                </div>
            </div>
        </div>
    }
}

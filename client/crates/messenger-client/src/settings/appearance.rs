use leptos::prelude::*;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::components::radio::{RadioGroup, RadioItem};
use crate::components::select::{Select, SelectOption};
use crate::i18n::{I18n, Locale};
use crate::theme::{Theme, apply_font_size, persist_locale};
use crate::t;

/// Appearance settings — theme, language, font size.
#[must_use]
#[component]
pub fn AppearanceSettings() -> impl IntoView {
    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let theme = use_context::<RwSignal<Theme>>().expect("Theme must be provided");
    let font_size = RwSignal::new("medium".to_string());

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.appearance.title")}</h3>
                <p class="text-sm text-muted-foreground">{t!("settings.appearance.description")}</p>
            </div>

            <Separator />

            // Theme
            <div class="space-y-3">
                <Label class="text-foreground">{t!("settings.appearance.theme")}</Label>
                <RadioGroup>
                    <RadioItem
                        value=String::from("system")
                        label=t!("appearance.theme.system")
                        active=Signal::derive(move || matches!(theme.get(), Theme::System))
                        on_select=Box::new(move || theme.set(Theme::System))
                    />
                    <RadioItem
                        value=String::from("light")
                        label=t!("appearance.theme.light")
                        active=Signal::derive(move || matches!(theme.get(), Theme::Light))
                        on_select=Box::new(move || theme.set(Theme::Light))
                    />
                    <RadioItem
                        value=String::from("dark")
                        label=t!("appearance.theme.dark")
                        active=Signal::derive(move || matches!(theme.get(), Theme::Dark))
                        on_select=Box::new(move || theme.set(Theme::Dark))
                    />
                </RadioGroup>
            </div>

            <Separator />

            // Language
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.appearance.language")}</Label>
                <Select
                    on_change=Box::new(move |v| {
                        let loc = match v.as_str() {
                            "ru" => Locale::Ru,
                            "en" => Locale::En,
                            _ => Locale::System,
                        };
                        i18n.locale.set(loc);
                    })
                    class="w-full max-w-xs"
                >
                    <SelectOption value=String::from("ru")>{t!("settings.appearance.langRu")}</SelectOption>
                    <SelectOption value=String::from("en")>{t!("settings.appearance.langEn")}</SelectOption>
                </Select>
            </div>

            <Separator />

            // Font size
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.appearance.fontSize")}</Label>
                <Select
                    on_change=Box::new(move |v| {
                        font_size.set(v.clone());
                        apply_font_size(&v);
                    })
                    class="w-full max-w-xs"
                >
                    <SelectOption value=String::from("small")>{t!("settings.appearance.fontSmall")}</SelectOption>
                    <SelectOption value=String::from("medium")>{t!("settings.appearance.fontMedium")}</SelectOption>
                    <SelectOption value=String::from("large")>{t!("settings.appearance.fontLarge")}</SelectOption>
                </Select>
            </div>
        </div>
    }
}

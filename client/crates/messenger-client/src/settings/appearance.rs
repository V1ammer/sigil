use leptos::prelude::*;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::components::radio::{RadioGroup, RadioItem};
use crate::components::select::{Select, SelectOption};
use crate::i18n::{Language, t};
use crate::theme::{Theme, apply_font_size};

/// Appearance settings — theme, language, font size.
#[must_use]
#[component]
pub fn AppearanceSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let theme = use_context::<RwSignal<Theme>>().unwrap_or_default();
    let app_lang = RwSignal::new(lang.get().as_str().to_string());
    let font_size = RwSignal::new("medium".to_string());

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.appearance.title")}</h3>
                <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.appearance.description")}</p>
            </div>

            <Separator />

            // Theme
            <div class="space-y-3">
                <Label class="text-foreground">{t(lang.get(), "settings.appearance.theme")}</Label>
                <RadioGroup>
                    <RadioItem
                        value=String::from("system")
                        label=t(lang.get(), "..." ).to_string()
                        active=Signal::derive(move || matches!(theme.get(), Theme::System))
                        on_select=Box::new(move || theme.set(Theme::System))
                    />
                    <RadioItem
                        value=String::from("light")
                        label=t(lang.get(), "..." ).to_string()
                        active=Signal::derive(move || matches!(theme.get(), Theme::Light))
                        on_select=Box::new(move || theme.set(Theme::Light))
                    />
                    <RadioItem
                        value=String::from("dark")
                        label=t(lang.get(), "..." ).to_string()
                        active=Signal::derive(move || matches!(theme.get(), Theme::Dark))
                        on_select=Box::new(move || theme.set(Theme::Dark))
                    />
                </RadioGroup>
            </div>

            <Separator />

            // Language
            <div class="space-y-2">
                <Label class="text-foreground">{t(lang.get(), "settings.appearance.language")}</Label>
                <Select
                    on_change=Box::new(move |v| {
                        app_lang.set(v.clone());
                        lang.set(Language::from_str(&v));
                    })
                    class="w-full max-w-xs"
                >
                    <SelectOption value=String::from("ru")>{t(lang.get(), "settings.appearance.langRu")}</SelectOption>
                    <SelectOption value=String::from("en")>{t(lang.get(), "settings.appearance.langEn")}</SelectOption>
                </Select>
            </div>

            <Separator />

            // Font size
            <div class="space-y-2">
                <Label class="text-foreground">{t(lang.get(), "settings.appearance.fontSize")}</Label>
                <Select
                    on_change=Box::new(move |v| {
                        font_size.set(v.clone());
                        apply_font_size(&v);
                    })
                    class="w-full max-w-xs"
                >
                    <SelectOption value=String::from("small")>{t(lang.get(), "settings.appearance.fontSmall")}</SelectOption>
                    <SelectOption value=String::from("medium")>{t(lang.get(), "settings.appearance.fontMedium")}</SelectOption>
                    <SelectOption value=String::from("large")>{t(lang.get(), "settings.appearance.fontLarge")}</SelectOption>
                </Select>
            </div>
        </div>
    }
}

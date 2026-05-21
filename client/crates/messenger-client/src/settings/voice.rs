use leptos::prelude::*;
use crate::i18n::{Language, t};

/// Voice settings — placeholder component.
#[must_use]
#[component]
pub fn VoiceSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();

    view! {
        <div class="flex flex-col items-center justify-center py-16 text-center">
            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground mb-4">
                <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
                <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                <line x1="12" y1="19" x2="12" y2="23" />
                <line x1="8" y1="23" x2="16" y2="23" />
            </svg>
            <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.voice.title")}</h3>
            <p class="text-sm text-muted-foreground mt-1 max-w-sm">
                {t(lang.get(), "settings.voice.description")}
            </p>
        </div>
    }
}

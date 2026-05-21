use leptos::prelude::*;
use crate::routes::AppRoutes;
use crate::theme::provide_theme;
use crate::i18n::Language;

#[must_use]
#[component]
pub fn App() -> impl IntoView {
    // Provide theme signal as context
    let _theme = provide_theme();
    // Provide language signal as context
    let lang = RwSignal::new(Language::Ru);
    provide_context(lang);

    view! {
        <main class="h-full bg-background text-foreground">
            <AppRoutes/>
        </main>
    }
}

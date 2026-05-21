use leptos::prelude::*;

/// Root application component.
#[component]
#[allow(clippy::must_use_candidate)]
pub fn App() -> impl IntoView {
    view! {
        <main class="min-h-screen flex items-center justify-center bg-zinc-50 dark:bg-zinc-900">
            <h1 class="text-2xl font-semibold text-zinc-900 dark:text-zinc-100">
                "messenger — bootstrap OK"
            </h1>
        </main>
    }
}

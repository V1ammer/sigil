use leptos::prelude::*;

#[must_use]
#[component]
pub fn Checkbox(
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional)] on_change: Option<Box<dyn Fn(bool) + 'static>>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    view! {
        <button
            role="checkbox"
            aria-checked=checked
            class=format!("peer h-4 w-4 shrink-0 rounded-sm border border-primary ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 {}", class)
            on:click=move |_| {
                if let Some(f) = on_change.as_ref() {
                    f(!checked.get());
                }
            }
        >
            <Show when=move || checked.get()>
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="20 6 9 17 4 12"/>
                </svg>
            </Show>
        </button>
    }
}

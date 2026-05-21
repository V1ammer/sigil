use leptos::prelude::*;

#[must_use]
#[component]
pub fn Switch(
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] on_change: Option<Box<dyn Fn(bool) + 'static>>,
) -> impl IntoView {
    view! {
        <button
            role="switch"
            aria-checked=checked
            disabled=disabled
            class=move || format!(
                "peer inline-flex h-[24px] w-[44px] shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 {}",
                if checked.get() { "bg-primary" } else { "bg-input" }
            )
            on:click=move |_| {
                if let Some(f) = on_change.as_ref() {
                    f(!checked.get());
                }
            }
        >
            <span
                class=move || format!(
                    "pointer-events-none block h-5 w-5 rounded-full bg-background shadow-lg ring-0 transition-transform {}",
                    if checked.get() { "translate-x-5" } else { "translate-x-0" }
                )
            />
        </button>
    }
}

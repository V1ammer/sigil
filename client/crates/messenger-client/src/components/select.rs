use leptos::prelude::*;

#[must_use]
#[component]
pub fn Select(
    /// Reactive selected value — keeps the dropdown in sync with state so it
    /// shows the persisted/current choice instead of always the first option.
    #[prop(optional, into)] value: Signal<String>,
    #[prop(optional)] on_change: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <select
            class=format!("flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 {}", class)
            prop:value=move || value.get()
            on:change=move |ev| {
                if let Some(f) = on_change.as_ref() {
                    f(event_target_value(&ev));
                }
            }
        >
            {children()}
        </select>
    }
}

#[must_use]
#[component]
pub fn SelectOption(
    #[prop(optional, into)] value: String,
    children: Children,
) -> impl IntoView {
    view! {
        <option value=value>{children()}</option>
    }
}

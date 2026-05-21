use leptos::prelude::*;

#[must_use]
#[component]
pub fn Progress(
    #[prop(optional, into)] value: f64,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    view! {
        <div class=format!("relative h-4 w-full overflow-hidden rounded-full bg-secondary {}", class)>
            <div
                class="h-full w-full flex-1 bg-primary transition-all"
                style=format!("transform: translateX(-{}%)", 100.0 - value)
            />
        </div>
    }
}

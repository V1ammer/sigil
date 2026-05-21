use leptos::prelude::*;

#[must_use]
#[component]
pub fn Skeleton(
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    view! {
        <div class=format!("animate-pulse rounded-md bg-muted {}", class) />
    }
}

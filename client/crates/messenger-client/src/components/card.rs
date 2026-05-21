use leptos::prelude::*;

#[must_use]
#[component]
pub fn Card(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("rounded-lg border bg-card text-card-foreground shadow-sm {}", class)>
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn CardContent(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("p-6 {}", class)>
            {children()}
        </div>
    }
}

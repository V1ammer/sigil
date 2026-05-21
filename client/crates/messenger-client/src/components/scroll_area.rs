use leptos::prelude::*;

#[must_use]
#[component]
pub fn ScrollArea(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("overflow-y-auto {}", class)>
            {children()}
        </div>
    }
}

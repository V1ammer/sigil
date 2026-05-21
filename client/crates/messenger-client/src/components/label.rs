use leptos::prelude::*;

#[must_use]
#[component]
pub fn Label(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <label class=format!("text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70 {}", class)>
            {children()}
        </label>
    }
}

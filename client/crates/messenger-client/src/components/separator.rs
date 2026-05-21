use leptos::prelude::*;

#[must_use]
#[component]
pub fn Separator(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] orientation: String,
) -> impl IntoView {
    let is_h = orientation != "vertical";
    view! {
        <div
            class=format!(
                "shrink-0 bg-border {} {}",
                if is_h { "h-[1px] w-full" } else { "h-full w-[1px]" },
                class
            )
        />
    }
}

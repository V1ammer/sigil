use leptos::prelude::*;

#[must_use]
#[component]
pub fn Tooltip(
    text: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="group relative inline-flex">
            {children()}
            <div class="absolute bottom-full left-1/2 z-50 mb-1 hidden -translate-x-1/2 group-hover:block">
                <div class="rounded-md bg-foreground px-2 py-1 text-xs text-background shadow-lg whitespace-nowrap">
                    {text}
                </div>
            </div>
        </div>
    }
}

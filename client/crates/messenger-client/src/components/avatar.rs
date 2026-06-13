use leptos::prelude::*;
use wasm_bindgen::prelude::JsCast;

#[must_use]
#[component]
pub fn Avatar(
    src: Option<String>,
    #[prop(optional, into)] alt: String,
    #[prop(optional, into)] class: String,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class=format!("relative flex shrink-0 overflow-hidden rounded-full {}", class)>
            <img
                class="aspect-square h-full w-full"
                src=src.clone().unwrap_or_default()
                alt=alt
                on:error=|ev| {
                    // Hide a broken image so the initials fallback shows.
                    if let Some(target) = ev.target() {
                        if let Some(el) = target.dyn_ref::<web_sys::HtmlElement>() {
                            let _ = el.style().set_property("display", "none");
                        }
                    }
                }
                on:load=|ev| {
                    // Un-hide on a successful load. Without this, an earlier
                    // error (a transient empty src, or a reused element that was
                    // hidden once) would leave a perfectly valid avatar hidden
                    // forever — which is exactly the chat-header case.
                    if let Some(target) = ev.target() {
                        if let Some(el) = target.dyn_ref::<web_sys::HtmlElement>() {
                            let _ = el.style().set_property("display", "");
                        }
                    }
                }
            />
            {children.map(|f| view! { <div class="flex h-full w-full items-center justify-center rounded-full bg-muted">{f()}</div> })}
        </div>
    }
}

/// Helper to get initials from a name.
pub fn get_initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|s| s.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

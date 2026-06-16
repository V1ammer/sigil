use leptos::prelude::*;
use std::sync::Arc;

/// Slide-over panel from the right (or bottom).
#[must_use]
#[component]
pub fn Sheet(
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional, into)] side: String,
    #[prop(optional, into)] class: String,
    children: ChildrenFn,
) -> impl IntoView {
    let show = is_open;
    let close_fn = Arc::new(on_close);
    let width_class = if side == "bottom" { "" } else { "max-w-md" };
    view! {
        <Show when=move || show.get()>
            <div class="fixed inset-0 z-50">
                <div
                    class="fixed inset-0 bg-black/50 animate-overlay-in"
                    on:click={
                        let cf = close_fn.clone();
                        move |_| { if let Some(ref f) = *cf { f(); } }
                    }
                />
                <div class=format!(
                    "fixed {} z-50 gap-4 bg-background p-6 shadow-lg ease-in-out {} {} {}",
                    if side == "bottom" { "bottom-0 left-0 right-0 rounded-t-xl max-h-[80vh]" } else { "right-0 top-0 h-full w-full sm:w-96 border-l" },
                    if side == "bottom" { "animate-sheet-bottom-in" } else { "animate-sheet-right-in" },
                    if side == "bottom" { "" } else { width_class },
                    class
                )>
                    {children()}
                </div>
            </div>
        </Show>
    }
}

#[must_use]
#[component]
pub fn SheetHeader(children: Children) -> impl IntoView {
    view! { <div class="mb-4">{children()}</div> }
}

#[must_use]
#[component]
pub fn SheetTitle(children: Children) -> impl IntoView {
    view! { <h2 class="text-lg font-semibold">{children()}</h2> }
}

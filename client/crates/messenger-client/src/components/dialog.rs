use leptos::prelude::*;
use std::sync::Arc;

/// Simple dialog component.
/// When `is_open` is true, renders a modal overlay.
#[must_use]
#[component]
pub fn Dialog(
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: ChildrenFn,
) -> impl IntoView {
    let show = is_open;
    let close_fn = Arc::new(on_close);
    view! {
        <Show when=move || show.get()>
            <div class="fixed inset-0 z-50 flex items-center justify-center">
                <div
                    class="fixed inset-0 bg-black/50 backdrop-blur-sm animate-overlay-in"
                    on:click={
                        let cf = close_fn.clone();
                        move |_| { if let Some(ref f) = *cf { f(); } }
                    }
                />
                <div class="relative z-50 w-full max-w-lg rounded-lg border bg-background p-6 shadow-lg animate-dialog-in">
                    {children()}
                </div>
            </div>
        </Show>
    }
}

#[must_use]
#[component]
pub fn DialogHeader(children: Children) -> impl IntoView {
    view! { <div class="mb-4">{children()}</div> }
}

#[must_use]
#[component]
pub fn DialogTitle(children: Children) -> impl IntoView {
    view! { <h2 class="text-lg font-semibold leading-none tracking-tight">{children()}</h2> }
}

#[must_use]
#[component]
pub fn DialogDescription(children: Children) -> impl IntoView {
    view! { <p class="text-sm text-muted-foreground mt-1">{children()}</p> }
}

#[must_use]
#[component]
pub fn DialogFooter(children: Children) -> impl IntoView {
    view! { <div class="flex justify-end gap-2 mt-4">{children()}</div> }
}

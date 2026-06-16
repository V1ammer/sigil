use leptos::prelude::*;
use std::sync::Arc;

#[must_use]
#[component]
pub fn AlertDialog(
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
                    class="fixed inset-0 bg-black/50 animate-overlay-in"
                    on:click={
                        let cf = close_fn.clone();
                        move |_| { if let Some(ref f) = *cf { f(); } }
                    }
                />
                <div class="relative z-50 w-full max-w-md rounded-lg border bg-background p-6 shadow-lg animate-dialog-in">
                    {children()}
                </div>
            </div>
        </Show>
    }
}

#[must_use]
#[component]
pub fn AlertDialogHeader(children: Children) -> impl IntoView {
    view! { <div class="mb-2">{children()}</div> }
}

#[must_use]
#[component]
pub fn AlertDialogTitle(children: Children) -> impl IntoView {
    view! { <h2 class="text-lg font-semibold">{children()}</h2> }
}

#[must_use]
#[component]
pub fn AlertDialogDescription(children: Children) -> impl IntoView {
    view! { <p class="text-sm text-muted-foreground mt-1">{children()}</p> }
}

#[must_use]
#[component]
pub fn AlertDialogFooter(children: Children) -> impl IntoView {
    view! { <div class="flex justify-end gap-2 mt-4">{children()}</div> }
}

#[must_use]
#[component]
pub fn AlertDialogCancel(
    #[prop(optional)] on_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: Children,
) -> impl IntoView {
    let cb = Arc::new(on_click);
    view! {
        <button
            class="inline-flex items-center justify-center rounded-md border border-input bg-background h-10 px-4 py-2 text-sm font-medium hover:bg-accent"
            on:click={
                let cf = cb.clone();
                move |_| { if let Some(ref f) = *cf { f(); } }
            }
        >
            {children()}
        </button>
    }
}

#[must_use]
#[component]
pub fn AlertDialogAction(
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: Children,
) -> impl IntoView {
    let cb = Arc::new(on_click);
    view! {
        <button
            class=format!("inline-flex items-center justify-center rounded-md bg-primary text-primary-foreground h-10 px-4 py-2 text-sm font-medium hover:bg-primary/90 {}", class)
            on:click={
                let cf = cb.clone();
                move |_| { if let Some(ref f) = *cf { f(); } }
            }
        >
            {children()}
        </button>
    }
}

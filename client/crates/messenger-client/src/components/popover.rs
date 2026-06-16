use leptos::prelude::*;
use std::sync::Arc;

#[must_use]
#[component]
pub fn Popover(
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: ChildrenFn,
) -> impl IntoView {
    let close_fn = Arc::new(on_close);
    view! {
        <Show when=move || is_open.get()>
            <div class="fixed inset-0 z-50" on:click={
                let cf = close_fn.clone();
                move |_| { if let Some(ref f) = *cf { f(); } }
            }>
                <div class="absolute z-50 rounded-md border bg-popover p-4 text-popover-foreground shadow-md outline-none origin-top animate-dropdown-in">
                    {children()}
                </div>
            </div>
        </Show>
    }
}

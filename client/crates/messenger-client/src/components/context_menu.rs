use leptos::prelude::*;

/// Simple context menu — wraps children with right-click behaviour.
#[must_use]
#[component]
pub fn ContextMenu(
    #[prop(optional, into)] content_class: String,
    children: Children,
    #[prop(optional)] menu: Option<Box<dyn Fn() -> leptos::prelude::AnyView + Send + Sync + 'static>>,
) -> impl IntoView {
    view! {
        <div class="relative">
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn ContextMenuTrigger(
    children: Children,
) -> impl IntoView {
    view! { {children()} }
}

#[must_use]
#[component]
pub fn ContextMenuContent(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md {}", class)>
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn ContextMenuItem(
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: Children,
) -> impl IntoView {
    let cb = std::sync::Arc::new(on_click);
    view! {
        <button
            class=format!("relative flex w-full cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent hover:text-accent-foreground {}", class)
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
pub fn ContextMenuSeparator() -> impl IntoView {
    view! { <div class="my-1 h-px bg-border" /> }
}

#[must_use]
#[component]
pub fn ContextMenuSub(children: Children) -> impl IntoView { view! { {children()} } }

#[must_use]
#[component]
pub fn ContextMenuSubTrigger(children: Children) -> impl IntoView {
    view! {
        <button class="relative flex w-full cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent">
            {children()}
        </button>
    }
}

#[must_use]
#[component]
pub fn ContextMenuSubContent(children: Children) -> impl IntoView {
    view! {
        <div class="ml-2 min-w-[8rem] rounded-md border bg-popover p-1 shadow-md">
            {children()}
        </div>
    }
}

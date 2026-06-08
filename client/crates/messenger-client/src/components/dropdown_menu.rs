use leptos::prelude::*;

/// Simple dropdown menu with a trigger and content.
#[must_use]
#[component]
pub fn DropdownMenu(
    children: Children,
) -> impl IntoView {
    let is_open = RwSignal::new(false);
    provide_context(is_open);
    view! { {children()} }
}

#[must_use]
#[component]
pub fn DropdownMenuTrigger(
    children: Children,
) -> impl IntoView {
    let is_open = use_context::<RwSignal<bool>>().expect("DropdownMenuTrigger must be inside DropdownMenu");
    view! {
        <span
            on:click=move |_| is_open.update(|v| *v = !*v)
            role="button"
            tabindex="0"
            style="display: contents;"
            on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                if ev.key() == "Enter" || ev.key() == " " {
                    is_open.update(|v| *v = !*v);
                }
            }
        >
            {children()}
        </span>
    }
}

#[must_use]
#[component]
pub fn DropdownMenuContent(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] align: String,
    children: Children,
) -> impl IntoView {
    let is_open = use_context::<RwSignal<bool>>().expect("DropdownMenuContent must be inside DropdownMenu");
    let align_class = match align.as_str() {
        "start" => "left-0",
        "end" => "right-0",
        _ => "left-0",
    };
    let content_class = format!(
        "absolute z-50 mt-1 min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md {} {}",
        align_class, class,
    );
    let children = children();
    view! {
        <div style:display=move || if is_open.get() { "block" } else { "none" }>
            <div
                class="fixed inset-0 z-40"
                on:click=move |_| is_open.set(false)
            />
            <div class=content_class>
                {children}
            </div>
        </div>
    }
}

#[must_use]
#[component]
pub fn DropdownMenuItem(
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: Children,
) -> impl IntoView {
    let is_open = use_context::<RwSignal<bool>>().expect("DropdownMenuItem must be inside DropdownMenu");
    let handler = move |_| {
        is_open.set(false);
        if let Some(ref f) = on_click {
            f();
        }
    };
    view! {
        <button
            class=format!("relative flex w-full cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 {}", class)
            on:click=handler
        >
            {children()}
        </button>
    }
}

#[must_use]
#[component]
pub fn DropdownMenuSeparator() -> impl IntoView {
    view! { <div class="my-1 h-px bg-border" /> }
}

#[must_use]
#[component]
pub fn DropdownMenuSub(children: Children) -> impl IntoView {
    view! { {children()} }
}

#[must_use]
#[component]
pub fn DropdownMenuSubTrigger(children: Children) -> impl IntoView {
    view! {
        <button class="relative flex w-full cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent">
            {children()}
        </button>
    }
}

#[must_use]
#[component]
pub fn DropdownMenuSubContent(children: Children) -> impl IntoView {
    view! {
        <div class="ml-2 min-w-[8rem] rounded-md border bg-popover p-1 shadow-md">
            {children()}
        </div>
    }
}

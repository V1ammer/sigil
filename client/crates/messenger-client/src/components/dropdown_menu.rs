use leptos::html::{Div, Span};
use leptos::prelude::*;

/// Context shared between trigger and content so the content can measure the
/// trigger's bounding rect when it opens and position itself within the
/// viewport (flipping above the trigger when there's no room below).
#[derive(Clone, Copy)]
struct DropdownCtx {
    is_open: RwSignal<bool>,
    trigger_ref: NodeRef<Span>,
}

#[must_use]
#[component]
pub fn DropdownMenu(children: Children) -> impl IntoView {
    let is_open = RwSignal::new(false);
    let trigger_ref: NodeRef<Span> = NodeRef::new();
    provide_context(DropdownCtx { is_open, trigger_ref });
    view! { {children()} }
}

#[must_use]
#[component]
pub fn DropdownMenuTrigger(children: Children) -> impl IntoView {
    let ctx = use_context::<DropdownCtx>().expect("DropdownMenuTrigger must be inside DropdownMenu");
    let is_open = ctx.is_open;
    view! {
        <span
            node_ref=ctx.trigger_ref
            on:click=move |_| is_open.update(|v| *v = !*v)
            role="button"
            tabindex="0"
            // `display: contents` keeps the original layout of the wrapped
            // button — the menu measures the span's first real child instead.
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
    let ctx = use_context::<DropdownCtx>().expect("DropdownMenuContent must be inside DropdownMenu");
    let is_open = ctx.is_open;
    let trigger_ref = ctx.trigger_ref;
    let content_ref: NodeRef<Div> = NodeRef::new();
    let align_end = align == "end";

    // (top_px, left_px) once measured. `None` while invisible so the menu
    // doesn't flash at the wrong position before placement.
    let pos = RwSignal::<Option<(f64, f64)>>::new(None);

    Effect::new(move |_| {
        if !is_open.get() {
            pos.set(None);
            return;
        }
        let Some(trigger_span) = trigger_ref.get() else { return };
        let Some(content_el) = content_ref.get() else { return };

        // The trigger span uses `display: contents`, so its own rect is
        // empty — measure the first real child element instead.
        let trigger_box = trigger_span
            .first_element_child()
            .map(|el| el.get_bounding_client_rect())
            .unwrap_or_else(|| trigger_span.get_bounding_client_rect());
        let content_rect = content_el.get_bounding_client_rect();
        let Some(win) = web_sys::window() else { return };
        let vw = win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(360.0);
        let vh = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(640.0);

        let gap = 4.0;
        let safe = 8.0;
        let needed_h = content_rect.height().max(1.0);
        let needed_w = content_rect.width().max(1.0);

        let space_below = vh - trigger_box.bottom();
        let space_above = trigger_box.top();
        let top = if space_below >= needed_h + gap + safe || space_below >= space_above {
            (trigger_box.bottom() + gap).min((vh - needed_h - safe).max(safe))
        } else {
            (trigger_box.top() - gap - needed_h).max(safe)
        };

        let left_raw = if align_end {
            trigger_box.right() - needed_w
        } else {
            trigger_box.left()
        };
        let left = left_raw.clamp(safe, (vw - needed_w - safe).max(safe));

        pos.set(Some((top, left)));
    });

    let content_class = format!(
        "z-50 min-w-[8rem] max-w-[calc(100vw-1rem)] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md origin-top animate-dropdown-in {class}",
    );
    let style_fn = move || match pos.get() {
        Some((top, left)) => format!("position: fixed; top: {top}px; left: {left}px;"),
        None => "position: fixed; top: 0; left: 0; visibility: hidden;".to_string(),
    };
    let children = children();
    view! {
        <div style:display=move || if is_open.get() { "block" } else { "none" }>
            <div
                class="fixed inset-0 z-40"
                on:click=move |_| is_open.set(false)
            />
            <div
                node_ref=content_ref
                class=content_class
                style=style_fn
            >
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
    let is_open = use_context::<DropdownCtx>()
        .expect("DropdownMenuItem must be inside DropdownMenu")
        .is_open;
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

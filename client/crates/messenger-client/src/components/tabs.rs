use leptos::prelude::*;

#[must_use]
#[component]
pub fn Tabs(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    // We use a simple pattern: the parent renders TabsList + TabsContent
    view! {
        <div class=class>
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn TabsList(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("inline-flex h-10 items-center justify-center rounded-md bg-muted p-1 text-muted-foreground {}", class)>
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn TabsTrigger(
    #[prop(optional, into)] value: String,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] active: Signal<bool>,
    #[prop(optional)] on_click: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    children: Children,
) -> impl IntoView {
    let active_class = move || if active.get() { "bg-background text-foreground shadow-sm" } else { "text-muted-foreground" };
    let cb = std::sync::Arc::new(on_click);
    view! {
        <button
            class=format!("inline-flex items-center justify-center whitespace-nowrap rounded-sm px-3 py-1.5 text-sm font-medium ring-offset-background transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 {}", active_class())
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
pub fn TabsContent(
    #[prop(optional, into)] value: String,
    #[prop(optional, into)] active: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <Show when=move || active.get()>
            <div class="mt-2">
                {children()}
            </div>
        </Show>
    }
}

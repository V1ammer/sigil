use leptos::prelude::*;

#[must_use]
#[component]
pub fn RadioGroup(
    children: Children,
) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-2">
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn RadioItem(
    #[prop(optional, into)] value: String,
    #[prop(optional, into)] label: String,
    #[prop(optional, into)] active: Signal<bool>,
    #[prop(optional)] on_select: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let cb = std::sync::Arc::new(on_select);
    view! {
        <button
            class="flex items-center gap-2 text-sm"
            on:click={
                let cf = cb.clone();
                move |_| { if let Some(ref f) = *cf { f(); } }
            }
        >
            <span class=format!(
                "flex h-4 w-4 items-center justify-center rounded-full border {}",
                if active.get() { "border-primary" } else { "border-muted-foreground" }
            )>
                <Show when=move || active.get()>
                    <span class="h-2 w-2 rounded-full bg-primary" />
                </Show>
            </span>
            <span>{label}</span>
        </button>
    }
}

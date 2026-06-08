use leptos::prelude::*;

#[must_use]
#[component]
pub fn Textarea(
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] value: String,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] rows: u32,
    #[prop(optional)] on_change: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_key_down: Option<Box<dyn Fn(leptos::ev::KeyboardEvent) + Send + Sync + 'static>>,
    #[prop(optional)] node_ref: NodeRef<leptos::html::Textarea>,
) -> impl IntoView {
    view! {
        <textarea
            placeholder=placeholder
            prop:value=value
            disabled=disabled
            rows=rows
            class=format!("flex min-h-[40px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 {}", class)
            on:input=move |ev| {
                if let Some(f) = on_change.as_ref() {
                    f(event_target_value(&ev));
                }
            }
            on:keydown=move |ev| {
                if let Some(f) = on_key_down.as_ref() {
                    f(ev);
                }
            }
            node_ref=node_ref
        />
    }
}

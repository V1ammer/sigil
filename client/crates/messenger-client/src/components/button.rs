use leptos::prelude::*;

#[derive(Clone, Copy, Default)]
pub enum ButtonVariant {
    #[default]
    Default,
    Destructive,
    Outline,
    Secondary,
    Ghost,
    Link,
}

#[derive(Clone, Copy, Default)]
pub enum ButtonSize {
    #[default]
    Default,
    Sm,
    Lg,
    Icon,
}

#[must_use]
#[component]
pub fn Button(
    #[prop(optional, into)] variant: Signal<ButtonVariant>,
    #[prop(optional, into)] size: Signal<ButtonSize>,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] on_click: Option<Box<dyn Fn(leptos::ev::MouseEvent) + Send + Sync + 'static>>,
    children: Children,
) -> impl IntoView {
    let base = "inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50";
    let variant_classes = move || match variant.get() {
        ButtonVariant::Default => "bg-primary text-primary-foreground hover:bg-primary/90",
        ButtonVariant::Destructive => "bg-destructive text-destructive-foreground hover:bg-destructive/90",
        ButtonVariant::Outline => "border border-input bg-background hover:bg-accent hover:text-accent-foreground",
        ButtonVariant::Secondary => "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ButtonVariant::Ghost => "hover:bg-accent hover:text-accent-foreground",
        ButtonVariant::Link => "text-primary underline-offset-4 hover:underline",
    };
    let size_classes = move || match size.get() {
        ButtonSize::Default => "h-10 px-4 py-2",
        ButtonSize::Sm => "h-9 rounded-md px-3",
        ButtonSize::Lg => "h-11 rounded-md px-8",
        ButtonSize::Icon => "h-10 w-10",
    };
    view! {
        <button
            class=move || format!("{base} {} {} {class}", variant_classes(), size_classes())
            disabled=disabled
            on:click=move |e| { if let Some(f) = on_click.as_ref() { f(e); } }
        >
            {children()}
        </button>
    }
}

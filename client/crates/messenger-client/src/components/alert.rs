use leptos::prelude::*;

#[must_use]
#[component]
pub fn Alert(
    #[prop(optional, into)] variant: String,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let base = "relative w-full rounded-lg border p-4 [&>svg~*]:pl-7 [&>svg+div]:translate-y-[-3px] [&>svg]:absolute [&>svg]:left-4 [&>svg]:top-4";
    let v = if variant == "destructive" { "border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive" } else { "bg-background text-foreground" };
    view! {
        <div class=format!("{base} {v} {class}")>
            {children()}
        </div>
    }
}

#[must_use]
#[component]
pub fn AlertDescription(
    children: Children,
) -> impl IntoView {
    view! {
        <div class="text-sm [&_p]:leading-relaxed">
            {children()}
        </div>
    }
}

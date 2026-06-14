use leptos::prelude::*;

#[must_use]
#[component]
pub fn Badge(
    #[prop(optional, into)] variant: String, // "default" | "secondary" | "outline" | "destructive"
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let base = "inline-flex items-center whitespace-nowrap rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2";
    let variant_class = match variant.as_str() {
        "secondary" => "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
        "outline" => "text-foreground",
        "destructive" => "border-transparent bg-destructive text-destructive-foreground hover:bg-destructive/80",
        _ => "border-transparent bg-primary text-primary-foreground hover:bg-primary/80",
    };
    view! {
        <div class=format!("{base} {variant_class} {class}")>
            {children()}
        </div>
    }
}

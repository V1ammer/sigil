use leptos::prelude::*;

#[must_use]
#[component]
pub fn Slider(
    #[prop(optional, into)] value: Signal<u32>,
    #[prop(optional)] on_change: Option<Box<dyn Fn(u32) + Send + Sync + 'static>>,
    #[prop(optional, into)] min: u32,
    #[prop(optional, into)] max: u32,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let pct = move || {
        if max == min { 0.0 } else { (value.get() - min) as f64 / (max - min) as f64 * 100.0 }
    };
    view! {
        <input
            type="range"
            min=min.to_string()
            max=max.to_string()
            value=value.get().to_string()
            class=format!("w-full h-2 bg-secondary rounded-lg appearance-none cursor-pointer accent-primary {}", class)
            on:input=move |ev| {
                if let Some(f) = on_change.as_ref() {
                    let v: u32 = event_target_value(&ev).parse().unwrap_or(0);
                    f(v);
                }
            }
        />
    }
}

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use gloo_timers::callback::Timeout;
use crate::i18n::I18n;
use crate::t;

#[must_use]
#[component]
pub fn LoginTokenScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let token = RwSignal::new(String::new());
    let is_loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let navigate = use_navigate();

    let format_token = move |value: &str| -> String {
        let cleaned: String = value
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_uppercase();
        let mut parts = Vec::new();
        let mut remaining = cleaned.as_str();
        while !remaining.is_empty() {
            let end = remaining.len().min(4);
            parts.push(&remaining[..end]);
            remaining = &remaining[end..];
        }
        parts.truncate(4);
        parts.join("-")
    };

    let navigate_for_submit = navigate.clone();
    let on_submit = std::sync::Arc::new(move || {
        let navigate = navigate_for_submit.clone();
        let tok = token.get();
        if tok.len() < 19 {
            error.set(Some(t!("token.error.invalid")));
            return;
        }
        is_loading.set(true);
        error.set(None);

        let nav = navigate.clone();
        Timeout::new(1500, move || {
            if tok.contains("EXPIRED") {
                error.set(Some(t!("token.error.expired")));
                is_loading.set(false);
            } else if tok.contains("USED") {
                error.set(Some(t!("token.error.exhausted")));
                is_loading.set(false);
            } else {
                nav("/register", Default::default());
            }
        })
        .forget();
    });

    let is_valid = move || token.get().len() == 19;

    view! {
        <div class="flex min-h-screen flex-col bg-background">
            <header class="flex items-center gap-4 border-b border-border p-4">
                <button
                    class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                    on:click={
                        let nav = navigate.clone();
                        move |_| nav("/login", Default::default())
                    }
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
                </button>
            </header>

            <main class="flex flex-1 flex-col items-center justify-center p-4">
                <div class="w-full max-w-md space-y-8">
                    <div class="space-y-2 text-center">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">{t!("token.title")}</h1>
                    </div>

                    <div class="space-y-4">
                        <div class="relative">
                            <input
                                type="text"
                                placeholder={t!("token.placeholder")}
                                maxlength=19u32
                                class="flex h-14 w-full rounded-md border border-input bg-background px-3 py-2 text-center font-mono text-lg tracking-wider ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                disabled=is_loading
                                prop:value=token
                                on:input=move |ev| {
                                    let formatted = format_token(&event_target_value(&ev));
                                    token.set(formatted);
                                    error.set(None);
                                }
                                on:keydown={
                                    let os = on_submit.clone();
                                    move |ev| {
                                        if ev.key() == "Enter" && !is_loading.get() && is_valid() {
                                            os();
                                        }
                                    }
                                }
                            />
                            {move || if is_valid() && error.get().is_none() {
                                view! {
                                    <svg class="absolute right-4 top-1/2 h-5 w-5 -translate-y-1/2 text-green-500" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                        </div>

                        <p class="text-center text-sm text-muted-foreground">{t!("token.hint")}</p>

                        {move || error.get().map(|e| {
                            view! {
                                <div class="relative w-full rounded-lg border border-destructive/50 p-4 bg-background text-destructive">
                                    <p class="text-sm">{e}</p>
                                </div>
                            }
                        })}

                        <button
                            class="inline-flex h-12 w-full items-center justify-center rounded-md bg-primary text-sm font-medium text-primary-foreground ring-offset-background transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
                            disabled={move || is_loading.get() || !is_valid()}
                            on:click={
                                let os = on_submit.clone();
                                move |_| os()
                            }
                        >
                            {move || if is_loading.get() { t!("loading") } else { t!("token.continue") }}
                        </button>
                    </div>
                </div>
            </main>
        </div>
    }
}

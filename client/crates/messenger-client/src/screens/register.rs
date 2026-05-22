use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use gloo_timers::callback::Timeout;
use crate::i18n::I18n;
use crate::t;

#[must_use]
#[component]
pub fn RegisterScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let username = RwSignal::new(String::new());
    let display_name = RwSignal::new(String::new());
    let username_status = RwSignal::new("idle".to_string());
    let is_submitting = RwSignal::new(false);
    let navigate = use_navigate();

    let on_username_change = move |value: &str| {
        let cleaned: String = value
            .chars()
            .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        username.set(cleaned);

        let u = username.get();
        if u.len() < 3 {
            username_status.set("idle".to_string());
            return;
        }
        username_status.set("checking".to_string());

        let u2 = u.clone();
        Timeout::new(500, move || {
            let taken = ["admin", "test", "user", "root"].contains(&u2.as_str());
            username_status.set(if taken {
                "taken".to_string()
            } else {
                "available".to_string()
            });
        })
        .forget();
    };

    let navigate_on_submit = navigate.clone();
    let on_submit = move || {
        if username.get().len() < 3
            || display_name.get().is_empty()
            || username_status.get() != "available"
        {
            return;
        }
        is_submitting.set(true);
        let nav = navigate_on_submit.clone();
        Timeout::new(1500, move || {
            nav("/chats", Default::default());
        })
        .forget();
    };

    let is_valid = move || {
        username.get().len() >= 3 && !display_name.get().is_empty() && username_status.get() == "available"
    };

    view! {
        <div class="flex min-h-screen flex-col bg-background">
            <header class="flex items-center gap-4 border-b border-border p-4">
                <button
                    class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                    on:click={
                        let nav = navigate.clone();
                        move |_| nav("/login/token", Default::default())
                    }
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
                </button>
            </header>

            <main class="flex flex-1 flex-col items-center justify-center p-4">
                <div class="w-full max-w-md space-y-8">
                    <div class="space-y-2 text-center">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">{t!("register.title")}</h1>
                    </div>

                    <div class="space-y-6">
                        <div class="flex flex-col items-center space-y-3">
                            <div class="flex h-24 w-24 cursor-pointer items-center justify-center rounded-full bg-muted">
                                <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
                            </div>
                            <p class="text-sm text-muted-foreground">{t!("register.avatar.hint")}</p>
                        </div>

                        <div class="space-y-2">
                            <label class="text-sm font-medium text-foreground">{t!("register.username")}</label>
                            <div class="relative">
                                <input
                                    type="text"
                                    placeholder="johndoe"
                                    maxlength=32u32
                                    class="flex h-12 w-full rounded-md border border-input bg-background px-3 py-2 pr-10 text-sm font-mono ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                    disabled=is_submitting
                                    prop:value=username
                                    on:input=move |ev| on_username_change(&event_target_value(&ev))
                                />
                                <div class="absolute right-3 top-1/2 -translate-y-1/2">
                                    {move || match username_status.get().as_str() {
                                        "checking" => view! { <span class="h-5 w-5 block rounded-full border-2 border-muted-foreground border-t-transparent animate-spin"/> }.into_any(),
                                        "available" => view! { <svg class="h-5 w-5 text-green-500" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg> }.into_any(),
                                        "taken" => view! { <svg class="h-5 w-5 text-destructive" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg> }.into_any(),
                                        _ => view! {}.into_any(),
                                    }}
                                </div>
                            </div>
                            <p class="text-xs text-muted-foreground">
                                {move || match username_status.get().as_str() {
                                    "taken" => t!("register.username.taken"),
                                    "available" => t!("register.username.available"),
                                    _ => t!("register.username.hint"),
                                }}
                            </p>
                        </div>

                        <div class="space-y-2">
                            <label class="text-sm font-medium text-foreground">{t!("register.displayName")}</label>
                            <input
                                type="text"
                                placeholder="John Doe"
                                maxlength=64u32
                                class="flex h-12 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                disabled=is_submitting
                                prop:value=display_name
                                on:input=move |ev| display_name.set(event_target_value(&ev))
                            />
                        </div>

                        <button
                            class="inline-flex h-12 w-full items-center justify-center rounded-md bg-primary text-sm font-medium text-primary-foreground ring-offset-background transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
                            disabled={move || is_submitting.get() || !is_valid()}
                            on:click=move |_| on_submit()
                        >
                            {move || if is_submitting.get() { t!("loading") } else { t!("register.create") }}
                        </button>

                        <p class="text-center text-xs text-muted-foreground">{t!("register.privacy")}</p>
                    </div>
                </div>
            </main>
        </div>
    }
}

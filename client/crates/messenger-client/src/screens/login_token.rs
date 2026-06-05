use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use crate::i18n::I18n;
use crate::t;
use crate::state::session::{use_session, SessionState};

#[must_use]
#[component]
pub fn LoginTokenScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let session = use_session();
    let navigate = use_navigate();

    // Redirect to chats if already authenticated
    let nav_if_auth = navigate.clone();
    Effect::new(move |_| {
        if session.is_authenticated() {
            nav_if_auth("/chats", NavigateOptions { replace: true, ..Default::default() });
        }
    });

    let token = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);

    let on_submit = {
        let navigate = navigate.clone();
        move || {
            let raw = token.get();
            let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            if cleaned.len() < 8 {
                error.set(Some(t!("token.error.invalid")));
                return;
            }
            error.set(None);
            navigate(&format!("/register?token={cleaned}"), Default::default());
        }
    };

    let is_valid = move || {
        let raw = token.get();
        let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
        cleaned.len() >= 8
    };

    view! {
        <div class="flex h-screen-safe flex-col bg-background overflow-hidden">
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
                                maxlength=64u32
                                class="flex h-14 w-full rounded-md border border-input bg-background px-3 py-2 text-center font-mono text-lg tracking-wider ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                prop:value=token
                                on:input=move |ev| {
                                    token.set(event_target_value(&ev));
                                    error.set(None);
                                }
                                on:keydown={
                                    let os = on_submit.clone();
                                    move |ev| {
                                        if ev.key() == "Enter" && is_valid() {
                                            os();
                                        }
                                    }
                                }
                            />
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
                            disabled=move || !is_valid()
                            on:click={
                                let os = on_submit.clone();
                                move |_| os()
                            }
                        >
                            {t!("token.continue")}
                        </button>
                    </div>
                </div>
            </main>
        </div>
    }
}

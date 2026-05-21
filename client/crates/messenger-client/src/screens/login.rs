use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use crate::i18n::{Language, t};

#[must_use]
#[component]
pub fn LoginScreen() -> impl IntoView {
    let lang = RwSignal::new(Language::Ru);
    let navigate = use_navigate();

    view! {
        <div class="flex min-h-screen flex-col bg-background">
            <header class="flex items-center gap-4 border-b border-border p-4">
                <button
                    class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                    on:click={
                        let nav = navigate.clone();
                        move |_| nav("/", Default::default())
                    }
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
                </button>
            </header>

            <main class="flex flex-1 flex-col items-center justify-center p-4">
                <div class="w-full max-w-md space-y-8">
                    <div class="space-y-2 text-center">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">
                            {t(lang.get(), "login.title")} " Server"
                        </h1>
                    </div>

                    <div class="space-y-4">
                        <div
                            class="cursor-pointer rounded-lg border bg-card text-card-foreground shadow-sm transition-colors hover:bg-accent"
                            on:click={
                        let nav = navigate.clone();
                        move |_| nav("/login/token", Default::default())
                    }
                        >
                            <div class="flex items-start gap-4 p-6">
                                <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-primary/10">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary"><circle cx="7.5" cy="15.5" r="5.5"/><path d="m21 2-9.6 9.6"/><path d="m15.5 7.5 3 3L22 7l-3-3"/></svg>
                                </div>
                                <div class="space-y-1">
                                    <h2 class="font-medium text-foreground">{t(lang.get(), "login.token.title")}</h2>
                                    <p class="text-sm text-muted-foreground">{t(lang.get(), "login.token.description")}</p>
                                </div>
                            </div>
                        </div>

                        <div
                            class="cursor-pointer rounded-lg border bg-card text-card-foreground shadow-sm transition-colors hover:bg-accent"
                            on:click={
                        let nav = navigate.clone();
                        move |_| nav("/login/qr", Default::default())
                    }
                        >
                            <div class="flex items-start gap-4 p-6">
                                <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-primary/10">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="5" height="5"/></svg>
                                </div>
                                <div class="space-y-1">
                                    <h2 class="font-medium text-foreground">{t(lang.get(), "login.qr.title")}</h2>
                                    <p class="text-sm text-muted-foreground">{t(lang.get(), "login.qr.description")}</p>
                                </div>
                            </div>
                        </div>
                    </div>

                    <p class="text-center text-sm text-muted-foreground">{t(lang.get(), "login.newDevice")}</p>
                </div>
            </main>
        </div>
    }
}

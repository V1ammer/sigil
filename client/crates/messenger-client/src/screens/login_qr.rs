use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use gloo_timers::callback::Timeout;
use crate::i18n::I18n;
use crate::t;

#[must_use]
#[component]
pub fn LoginQrScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let time_left = RwSignal::new(300i32);
    let is_waiting = RwSignal::new(false);
    let is_success = RwSignal::new(false);
    let navigate = use_navigate();

    // Countdown timer — spawn a local future to tick every second
    let tick_nav = navigate.clone();
    let tick_tl = time_left;
    leptos::task::spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            let v = tick_tl.get_untracked();
            if v <= 1 {
                break;
            }
            tick_tl.set(v - 1);
        }
    });

    // Simulate waiting -> success
    let nav = navigate.clone();
    Timeout::new(5000, move || {
        is_waiting.set(true);
        let nav2 = nav.clone();
        Timeout::new(3000, move || {
            is_success.set(true);
            let nav3 = nav2.clone();
            Timeout::new(1500, move || {
                nav3("/chats", Default::default());
            })
            .forget();
        })
        .forget();
    })
    .forget();

    let format_time = move || {
        let secs = time_left.get();
        format!("{}:{:02}", secs / 60, secs % 60)
    };

    view! {
        <div class="flex min-h-screen flex-col bg-background">
            <header class="flex items-center gap-4 border-b border-border p-4">
                <button
                    class="h-10 w-10 inline-flex items-center justify-center rounded-md hover:bg-accent"
                    on:click=move |_| navigate("/login", Default::default())
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
                </button>
            </header>

            <main class="flex flex-1 flex-col items-center justify-center p-4">
                <div class="w-full max-w-md space-y-8">
                    <div class="space-y-2 text-center">
                        <h1 class="text-2xl font-semibold tracking-tight text-foreground">{t!("qr.title")}</h1>
                        <p class="text-sm leading-relaxed text-muted-foreground">{t!("qr.instruction")}</p>
                    </div>

                    <div class="flex flex-col items-center space-y-4">
                        <div class="relative">
                            <div class="flex h-64 w-64 items-center justify-center rounded-2xl border border-border bg-card">
                                <svg viewBox="0 0 200 200" class="h-52 w-52">
                                    <rect x="10" y="10" width="50" height="50" rx="4" class="fill-current text-foreground"/>
                                    <rect x="20" y="20" width="30" height="30" rx="2" class="fill-card"/>
                                    <rect x="28" y="28" width="14" height="14" rx="1" class="fill-current text-foreground"/>
                                    <rect x="140" y="10" width="50" height="50" rx="4" class="fill-current text-foreground"/>
                                    <rect x="150" y="20" width="30" height="30" rx="2" class="fill-card"/>
                                    <rect x="158" y="28" width="14" height="14" rx="1" class="fill-current text-foreground"/>
                                    <rect x="10" y="140" width="50" height="50" rx="4" class="fill-current text-foreground"/>
                                    <rect x="20" y="150" width="30" height="30" rx="2" class="fill-card"/>
                                    <rect x="28" y="158" width="14" height="14" rx="1" class="fill-current text-foreground"/>
                                    {{
                                        (0..15).map(|i| {
                                            let x = 70 + (i % 5) * 12;
                                            let y = 70 + (i / 5) * 12;
                                            let opacity = if i % 2 == 0 { "1" } else { "0" };
                                            view! { <rect x={x.to_string()} y={y.to_string()} width="10" height="10" rx="1" opacity=opacity class="fill-current text-foreground"/> }
                                        }).collect::<Vec<_>>()
                                    }}
                                </svg>

                                {move || if is_success.get() {
                                    view! {
                                        <div class="absolute inset-0 flex items-center justify-center rounded-2xl bg-background/90 backdrop-blur-sm">
                                            <div class="flex flex-col items-center gap-2">
                                                <svg class="h-12 w-12 text-green-500" xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                                                <span class="text-sm font-medium text-foreground">{t!("qr.success")}</span>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else if is_waiting.get() {
                                    view! {
                                        <div class="absolute inset-0 flex items-center justify-center rounded-2xl bg-background/90 backdrop-blur-sm">
                                            <div class="flex flex-col items-center gap-2">
                                                <span class="h-8 w-8 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                                                <span class="text-sm text-muted-foreground">{t!("qr.waiting")}</span>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }}
                            </div>
                        </div>

                        {move || if !is_waiting.get() && !is_success.get() {
                            view! {
                                <>
                                    <div class="flex items-center gap-2 text-sm text-muted-foreground">
                                        <span>{t!("qr.validFor")}</span>
                                        <span class="font-mono">{format_time()}</span>
                                    </div>
                                    <button
                                        class="inline-flex items-center justify-center gap-2 rounded-md border border-input bg-background h-10 px-4 py-2 text-sm font-medium hover:bg-accent"
                                        on:click=move |_| { time_left.set(300); is_waiting.set(false); }
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M3 21v-5h5"/></svg>
                                        {t!("qr.refresh")}
                                    </button>
                                </>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }}

                        <div class="text-center">
                            <span class="text-xs text-muted-foreground">{t!("qr.requestId")}: </span>
                            <code class="font-mono text-xs text-muted-foreground">"prov_a7b3c9d2e1f0"</code>
                        </div>
                    </div>
                </div>
            </main>
        </div>
    }
}

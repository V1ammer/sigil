use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_navigate};
use crate::i18n::I18n;
use crate::state::session::use_session;
use crate::settings::*;
use crate::t;

#[must_use]
#[component]
pub fn SettingsScreen() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let session = use_session();
    let params = use_params_map();
    let navigate = use_navigate();
    let section = Signal::derive(move || {
        params
            .get()
            .get("section")
            .map(|s| s.clone())
            .unwrap_or_else(|| "account".to_string())
    });

    let is_admin = session.is_admin();

    // Build sections list based on role.
    let sections: Vec<(&'static str, &'static str)> = {
        let mut s: Vec<(&'static str, &'static str)> = vec![
            ("account", "settings.account"),
            ("devices", "settings.devices"),
            ("appearance", "settings.appearance"),
            ("notifications", "settings.notifications"),
            ("privacy", "settings.privacy"),
            ("voice", "settings.voice"),
            ("about", "settings.about"),
        ];
        if is_admin {
            s.push(("admin-invites", "settings.admin.invites"));
            s.push(("admin-users", "settings.admin.users"));
        }
        s
    };

    // Close Settings with history.back() rather than a fresh push to "/chats":
    // the push left a duplicate history entry AND didn't reset the (global)
    // selected chat, so closing Settings showed the still-open chat instead of
    // the list. back() pops the /settings entry — its popstate closes any open
    // chat overlay (selected -> None) and lands on the chat list, matching the
    // hardware back button.
    let go_chats = move |_| crate::state::back_stack::pop();

    let has_sec = Signal::derive(move || params.get().get("section").is_some());

    let render_content = move || {
        match section.get().as_str() {
            "account" => view! { <AccountSettings /> }.into_any(),
            "devices" => view! { <DevicesSettings /> }.into_any(),
            "appearance" => view! { <AppearanceSettings /> }.into_any(),
            "notifications" => view! { <NotificationsSettings /> }.into_any(),
            "privacy" => view! { <PrivacySettings /> }.into_any(),
            "voice" => view! { <VoiceSettings /> }.into_any(),
            "admin-invites" => view! { <AdminInvitesSettings /> }.into_any(),
            "admin-users" => view! { <AdminUsersSettings /> }.into_any(),
            "about" => view! { <AboutSettings /> }.into_any(),
            _ => view! { <AccountSettings /> }.into_any(),
        }
    };

    let sidebar_visible = Signal::derive(move || !has_sec.get());
    let content_visible = Signal::derive(move || has_sec.get());

    // Prebuild section buttons
    let section_buttons: Vec<_> = sections
        .iter()
        .map(|(id, label_key)| {
            let sid = *id;
            let label = t!(*label_key);
            let n = navigate.clone();
            let active = move || section.get() == sid;
            let class = move || {
                let base = "w-full flex items-center gap-3 px-3 py-2 rounded-lg text-left text-sm transition-colors";
                if active() {
                    format!("{} bg-accent text-accent-foreground", base)
                } else {
                    format!("{} hover:bg-muted text-foreground", base)
                }
            };
            let onclick = move |_| n(&format!("/settings/{}", sid), Default::default());
            view! {
                <button class=class on:click=onclick>
                    {label.clone()}
                </button>
            }
        })
        .collect();

    view! {
        <div class="flex h-full bg-background">
            {/* Sidebar */}
            <div class=move || {
                if sidebar_visible.get() {
                    "w-full md:w-64 border-r border-border flex flex-col".to_string()
                } else {
                    "hidden md:flex md:w-64 md:border-r md:border-border md:flex-col".to_string()
                }
            }>
                <div class="flex items-center justify-between p-4 border-b border-border">
                    <h1 class="text-lg font-semibold text-foreground">{t!("settings.title")}</h1>
                    <button
                        class="h-9 w-9 inline-flex items-center justify-center rounded-md hover:bg-accent"
                        on:click=go_chats
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
                    </button>
                </div>

                <div class="flex-1 overflow-y-auto p-2 space-y-1">
                    {section_buttons}
                </div>
            </div>

            {/* Content */}
            <div class=move || {
                if content_visible.get() {
                    "flex-1 overflow-y-auto".to_string()
                } else {
                    "hidden md:block md:flex-1 md:overflow-y-auto".to_string()
                }
            }>
                <div class=move || {
                    // The Users tab is a wide table — give it more room so the
                    // columns fit without a horizontal scrollbar. Other tabs keep
                    // a comfortable reading width.
                    if section.get().as_str() == "admin-users" {
                        "max-w-5xl p-6"
                    } else {
                        "max-w-2xl p-6"
                    }
                }>
                    {move || {
                        if has_sec.get() {
                            view! {
                                <div class="md:hidden mb-4">
                                    <button
                                        class="inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
                                        on:click=move |_| crate::state::back_stack::pop()
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>
                                        {t!("settings.back")}
                                    </button>
                                </div>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }
                    }}
                    // Передаём замыкание, а не результат вызова — иначе секция
                    // рендерится один раз и не реагирует на смену :section.
                    {render_content}
                </div>
            </div>
        </div>
    }
}

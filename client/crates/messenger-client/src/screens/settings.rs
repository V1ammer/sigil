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
    let section = move || {
        params
            .get()
            .get("section")
            .map(|s| s.clone())
            .unwrap_or_else(|| "account".to_string())
    };

    let is_admin = session.is_admin();

    // Build sections list based on role.
    let sections: Vec<(&str, &str)> = {
        let mut s = vec![
            ("account", "settings.account"),
            ("devices", "settings.devices"),
            ("appearance", "settings.appearance"),
            ("notifications", "settings.notifications"),
            ("privacy", "settings.privacy"),
            ("about", "settings.about"),
        ];
        if is_admin {
            s.push(("admin-invites", "settings.admin.invites"));
            s.push(("admin-users", "settings.admin.users"));
        }
        s
    };

    let render_content = move || {
        match section().as_str() {
            "account" => view! { <AccountSettings /> }.into_any(),
            "devices" => view! { <DevicesSettings /> }.into_any(),
            "appearance" => view! { <AppearanceSettings /> }.into_any(),
            "notifications" => view! { <NotificationsSettings /> }.into_any(),
            "privacy" => view! { <PrivacySettings /> }.into_any(),
            "admin-invites" => view! { <AdminInvitesSettings /> }.into_any(),
            "admin-users" => view! { <AdminUsersSettings /> }.into_any(),
            "about" => view! { <AboutSettings /> }.into_any(),
            _ => view! { <AccountSettings /> }.into_any(),
        }
    };

    view! {
        <div class="flex h-full bg-background">
            {/* Sidebar */}
            <div class="w-64 border-r border-border flex flex-col">
                <div class="flex items-center justify-between p-4 border-b border-border">
                    <h1 class="text-lg font-semibold text-foreground">{t!("settings.title")}</h1>
                    <button
                        class="h-9 w-9 inline-flex items-center justify-center rounded-md hover:bg-accent"
                        on:click={
                            let nav = navigate.clone();
                            move |_| nav("/chats", Default::default())
                        }
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
                    </button>
                </div>

                <div class="flex-1 overflow-y-auto p-2 space-y-1">
                    {sections
                        .iter()
                        .map(|(id, label_key)| {
                            let id = *id;
                            let label = t!(label_key);
                            let is_active = move || section() == id;
                            let nav = navigate.clone();
                            view! {
                                <button
                                    class={move || format!(
                                        "w-full flex items-center gap-3 px-3 py-2 rounded-lg text-left text-sm transition-colors {}",
                                        if is_active() {
                                            "bg-accent text-accent-foreground"
                                        } else {
                                            "hover:bg-muted text-foreground"
                                        }
                                    )}
                                    on:click=move |_| nav(&format!("/settings/{}", id), Default::default())
                                >
                                    {label.clone()}
                                </button>
                            }
                        })
                        .collect::<Vec<_>>()}
                </div>
            </div>

            {/* Content */}
            <div class="flex-1 overflow-y-auto">
                <div class="max-w-2xl p-6">
                    {render_content()}
                </div>
            </div>
        </div>
    }
}

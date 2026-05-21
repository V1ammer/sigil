use leptos::prelude::*;
use crate::components::button::{Button, ButtonVariant, ButtonSize};
use crate::components::badge::Badge;
use crate::components::separator::Separator;
use crate::i18n::{Language, t, format_date};
use crate::mock::{mock_admin_users, AdminUser};

/// User status badge.
fn user_status_badge(status: &str, lang: Language) -> impl IntoView {
    let (variant, text) = match status {
        "active" => ("default", "settings.adminUsers.statusActive"),
        "suspended" => ("destructive", "settings.adminUsers.statusSuspended"),
        _ => ("secondary", "settings.adminUsers.statusInactive"),
    };
    view! {
        <Badge variant=String::from(variant) class="capitalize">
            {t(lang, text)}
        </Badge>
    }
}

/// Admin users — table of users with suspend/unsuspend actions.
#[must_use]
#[component]
pub fn AdminUsersSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let users = RwSignal::new(mock_admin_users());

    let toggle_user_status = move |id: String| {
        users.update(|usrs| {
            if let Some(u) = usrs.iter_mut().find(|u| u.id == id) {
                u.status = if u.status == "active" { "suspended".into() } else { "active".into() };
            }
        });
    };

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.adminUsers.title")}</h3>
                <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.adminUsers.description")}</p>
            </div>

            <Separator />

            // Users table
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="border-b text-left text-muted-foreground">
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminUsers.user")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminUsers.role")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminUsers.status")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminUsers.created")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminUsers.lastActive")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminUsers.actions")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || users.get().into_iter().map(|u| {
                            let user_id = u.id.clone();
                            let is_owner = u.role == "owner";
                            view! {
                                <tr class="border-b last:border-0">
                                    <td class="py-3 pr-4">
                                        <div class="flex items-center gap-2">
                                            <div class="flex h-8 w-8 items-center justify-center rounded-full bg-muted text-xs font-medium text-foreground">
                                                {crate::components::avatar::get_initials(&u.display_name)}
                                            </div>
                                            <div>
                                                <p class="text-sm font-medium text-foreground">{u.display_name.clone()}</p>
                                                <p class="text-xs text-muted-foreground">@{u.username.clone()}</p>
                                            </div>
                                        </div>
                                    </td>
                                    <td class="py-3 pr-4 capitalize text-foreground">{u.role.clone()}</td>
                                    <td class="py-3 pr-4">
                                        {user_status_badge(&u.status, lang.get())}
                                    </td>
                                    <td class="py-3 pr-4 text-muted-foreground">
                                        {format_date(u.created_at, lang.get())}
                                    </td>
                                    <td class="py-3 pr-4 text-muted-foreground">
                                        {format_date(u.last_active, lang.get())}
                                    </td>
                                    <td class="py-3">
                                        {move || if is_owner {
                                            view! {
                                                <span class="text-xs text-muted-foreground">"—"</span>
                                            }.into_any()
                                        } else {
                                            let is_suspended = u.status == "suspended";
                                            view! {
                                                <Button
                                                    variant=Signal::derive(move || {
                                                        if is_suspended { ButtonVariant::Default } else { ButtonVariant::Destructive }
                                                    })
                                                    size=Signal::derive(move || ButtonSize::Sm)
                                                    on_click=Box::new({
                                                        let id = user_id.clone();
                                                        move |_| toggle_user_status(id.clone())
                                                    })
                                                >
                                                    {if is_suspended {
                                                        t(lang.get(), "settings.adminUsers.unsuspend")
                                                    } else {
                                                        t(lang.get(), "settings.adminUsers.suspend")
                                                    }}
                                                </Button>
                                            }.into_any()
                                        }}
                                    </td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

use leptos::prelude::*;
use crate::components::button::{Button, ButtonVariant, ButtonSize};
use crate::components::badge::Badge;
use crate::components::dialog::{Dialog, DialogHeader, DialogTitle, DialogDescription, DialogFooter};
use crate::components::separator::Separator;
use crate::components::select::{Select, SelectOption};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::i18n::{Language, t, format_date};
use crate::mock::{mock_invitations, Invitation};

/// Invitation status badge.
fn status_badge(status: &str, lang: Language) -> impl IntoView {
    let (variant, text) = match status {
        "active" => ("default", "settings.adminInvites.statusActive"),
        "expired" => ("outline", "settings.adminInvites.statusExpired"),
        "revoked" => ("destructive", "settings.adminInvites.statusRevoked"),
        _ => ("secondary", "settings.adminInvites.statusExhausted"),
    };
    view! {
        <Badge variant=String::from(variant) class="capitalize">
            {t(lang, text)}
        </Badge>
    }
}

/// Admin invites — table of invitations with create/revoke.
#[must_use]
#[component]
pub fn AdminInvitesSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let invitations = RwSignal::new(mock_invitations());
    let show_create = RwSignal::new(false);
    let new_role = RwSignal::new("user".to_string());
    let new_max_uses = RwSignal::new("5".to_string());

    let revoke_invite = move |id: String| {
        invitations.update(|invites| {
            if let Some(inv) = invites.iter_mut().find(|i| i.id == id) {
                inv.status = "revoked".into();
            }
        });
    };

    let create_invite = move || {
        // TODO: actually create invitation via API
        show_create.set(false);
        new_role.set("user".into());
        new_max_uses.set("5".into());
    };

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.adminInvites.title")}</h3>
                    <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.adminInvites.description")}</p>
                </div>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    size=Signal::derive(move || ButtonSize::Sm)
                    on_click=Box::new(move |_| show_create.set(true))
                >
                    {t(lang.get(), "settings.adminInvites.create")}
                </Button>
            </div>

            <Separator />

            // Invitations table
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="border-b text-left text-muted-foreground">
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminInvites.token")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminInvites.role")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminInvites.uses")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminInvites.expires")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminInvites.status")}</th>
                            <th class="pb-2 font-medium">{t(lang.get(), "settings.adminInvites.actions")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || invitations.get().into_iter().map(|inv| {
                            let inv_id = inv.id.clone();
                            view! {
                                <tr class="border-b last:border-0">
                                    <td class="py-3 pr-4">
                                        <span class="font-mono text-xs text-foreground">{inv.token.clone()}</span>
                                    </td>
                                    <td class="py-3 pr-4 capitalize text-foreground">{inv.role.clone()}</td>
                                    <td class="py-3 pr-4 text-muted-foreground">
                                        {format!("{}/{}", inv.used_count, inv.max_uses)}
                                    </td>
                                    <td class="py-3 pr-4 text-muted-foreground">
                                        {format_date(inv.expires_at, lang.get())}
                                    </td>
                                    <td class="py-3 pr-4">
                                        {status_badge(&inv.status, lang.get())}
                                    </td>
                                    <td class="py-3">
                                        {move || if inv.status == "active" {
                                            view! {
                                                <Button
                                                    variant=Signal::derive(move || ButtonVariant::Ghost)
                                                    size=Signal::derive(move || ButtonSize::Sm)
                                                    class="text-destructive hover:text-destructive"
                                                    on_click=Box::new({
                                                        let id = inv_id.clone();
                                                        move |_| revoke_invite(id.clone())
                                                    })
                                                >
                                                    {t(lang.get(), "settings.adminInvites.revoke")}
                                                </Button>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <span class="text-xs text-muted-foreground">"—"</span>
                                            }.into_any()
                                        }}
                                    </td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>

            // Create dialog
            <Dialog
                is_open=Signal::derive(move || show_create.get())
                on_close=Box::new(move || show_create.set(false))
            >
                <DialogHeader>
                    <DialogTitle>{t(lang.get(), "settings.adminInvites.createTitle")}</DialogTitle>
                    <DialogDescription>{t(lang.get(), "settings.adminInvites.createDesc")}</DialogDescription>
                </DialogHeader>
                <div class="space-y-4 py-2">
                    <div class="space-y-2">
                        <Label class="text-foreground">{t(lang.get(), "settings.adminInvites.role")}</Label>
                        <Select
                            on_change=Box::new(move |v| new_role.set(v))
                        >
                            <SelectOption value=String::from("user")>{t(lang.get(), "settings.adminInvites.roleUser")}</SelectOption>
                            <SelectOption value=String::from("admin")>{t(lang.get(), "settings.adminInvites.roleAdmin")}</SelectOption>
                        </Select>
                    </div>
                    <div class="space-y-2">
                        <Label class="text-foreground">{t(lang.get(), "settings.adminInvites.maxUses")}</Label>
                        <Input
                            value=new_max_uses.get()
                            on_change=Box::new(move |v| new_max_uses.set(v))
                            placeholder="5".to_string()
                        />
                    </div>
                </div>
                <DialogFooter>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Outline)
                        on_click=Box::new(move |_| show_create.set(false))
                    >
                        {t(lang.get(), "settings.adminInvites.cancel")}
                    </Button>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Default)
                        on_click=Box::new(move |_| create_invite())
                    >
                        {t(lang.get(), "settings.adminInvites.create")}
                    </Button>
                </DialogFooter>
            </Dialog>
        </div>
    }
}

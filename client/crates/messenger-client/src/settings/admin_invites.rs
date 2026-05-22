use std::sync::Arc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_proto::invites::*;
use uuid::Uuid;

use crate::components::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter,
    AlertDialogHeader, AlertDialogTitle,
};
use crate::components::badge::Badge;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::dialog::{
    Dialog, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::components::separator::Separator;
use crate::i18n::{format_date, t, I18n, Locale};
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::session::build_api_client;
use crate::t;

/// Derive an invite status string from `InviteSummary` fields.
fn invite_status(inv: &InviteSummary) -> &'static str {
    if inv.revoked_at.is_some() {
        "revoked"
    } else if inv.uses_count >= inv.max_uses {
        "exhausted"
    } else {
        let now_secs = (js_sys::Date::now() / 1000.0) as i64;
        if inv.expires_at > 0 && inv.expires_at < now_secs {
            "expired"
        } else {
            "active"
        }
    }
}

/// Invitation status badge.
fn status_badge(status: &str, lang: Locale) -> impl IntoView {
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
    let i18n = use_context::<I18n>().expect("I18n must be provided");
    let notifications =
        use_context::<NotificationsState>().expect("NotificationsState must be provided");

    // Invites list state
    let invites = RwSignal::new(Vec::<InviteSummary>::new());
    let loading = RwSignal::new(true);

    // Create dialog state
    let show_create = RwSignal::new(false);
    let new_role = RwSignal::new("user".to_string());
    let new_max_uses = RwSignal::new("5".to_string());
    let new_ttl = RwSignal::new("86400".to_string());

    // Token display dialog state
    let show_token = RwSignal::new(false);
    let token_hex = RwSignal::new(String::new());

    // Revoke dialog state
    let show_revoke = RwSignal::new(false);
    let revoke_target = RwSignal::new(Option::<Uuid>::None);
    let revoking = RwSignal::new(false);
    let token_copied = RwSignal::new(false);

    // ── Load invites on mount ──────────────────────────────────────
    spawn_local({
        let invs = invites;
        let loading = loading;
        let nf = notifications.clone();
        async move {
            let api = build_api_client();
            match api {
                Some(client) => match client.list_invites().await {
                    Ok(resp) => {
                        invs.set(resp.invites);
                        loading.set(false);
                    }
                    Err(e) => {
                        nf.push(ToastKind::Error, format!("Failed to load invites: {e}"));
                        loading.set(false);
                    }
                },
                None => {
                    loading.set(false);
                }
            }
        }
    });

    // ── Refresh helper ─────────────────────────────────────────────
    let refresh_invites = {
        let invs = invites;
        let nf = notifications.clone();
        move || {
            let invs = invs;
            let nf = nf.clone();
            spawn_local(async move {
                let api = build_api_client();
                match api {
                    Some(client) => match client.list_invites().await {
                        Ok(resp) => {
                            invs.set(resp.invites);
                        }
                        Err(e) => {
                            nf.push(ToastKind::Error, format!("Failed to refresh invites: {e}"));
                        }
                    },
                    None => {}
                }
            });
        }
    };

    // ── Create invite handler ──────────────────────────────────────
    let create_invite = {
        let show = show_create;
        let show_tok = show_token;
        let tok_hex = token_hex;
        let new_r = new_role;
        let new_m = new_max_uses;
        let new_t = new_ttl;
        let nf = notifications.clone();
        let refresh = refresh_invites.clone();
        move || {
            let show = show;
            let show_tok = show_tok;
            let tok_hex = tok_hex;
            let new_r = new_r;
            let new_m = new_m;
            let new_t = new_t;
            let nf = nf.clone();
            let refresh = refresh.clone();

            let role = new_r.get_untracked();
            let max_uses_str = new_m.get_untracked();
            let ttl_str = new_t.get_untracked();

            let max_uses: i32 = match max_uses_str.parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    nf.push(ToastKind::Warning, "Invalid max uses value".to_string());
                    return;
                }
            };
            let ttl_seconds: i64 = match ttl_str.parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    nf.push(ToastKind::Warning, "Invalid TTL value".to_string());
                    return;
                }
            };

            let req = CreateInviteRequest {
                role_to_grant: role.clone(),
                max_uses,
                ttl_seconds,
            };

            spawn_local(async move {
                let api = build_api_client();
                match api {
                    Some(client) => match client.create_invite(&req).await {
                        Ok(resp) => {
                            let hex: String = resp
                                .token
                                .iter()
                                .map(|b| format!("{:02x}", b))
                                .collect();
                            tok_hex.set(hex);
                            show_tok.set(true);
                            show.set(false);

                            new_r.set("user".into());
                            new_m.set("5".into());
                            new_t.set("86400".into());

                            nf.push(ToastKind::Success, t!("settings.adminInvites.created"));

                            refresh();
                        }
                        Err(e) => {
                            nf.push(
                                ToastKind::Error,
                                format!("{}: {e}", t!("settings.adminInvites.createFailed")),
                            );
                        }
                    },
                    None => {
                        nf.push(ToastKind::Error, "Not authenticated".to_string());
                    }
                }
            });
        }
    };
    let create_invite = Arc::new(create_invite);

    // ── Revoke handler ─────────────────────────────────────────────
    let do_revoke = {
        let target = revoke_target;
        let show = show_revoke;
        let rev = revoking;
        let nf = notifications.clone();
        let refresh = refresh_invites.clone();
        move || {
            let target = target;
            let show = show;
            let rev = rev;
            let nf = nf.clone();
            let refresh = refresh.clone();
            rev.set(true);
            if let Some(inv_id) = target.get_untracked() {
                let nf = nf.clone();
                let refresh = refresh.clone();
                spawn_local(async move {
                    let api = build_api_client();
                    match api {
                        Some(client) => match client.revoke_invite(inv_id).await {
                            Ok(()) => {
                                nf.push(ToastKind::Success, t!("settings.adminInvites.revoke"));
                                refresh();
                            }
                            Err(e) => {
                                nf.push(ToastKind::Error, format!("Revoke failed: {e}"));
                            }
                        },
                        None => {
                            nf.push(ToastKind::Error, "Not authenticated".to_string());
                        }
                    }
                    rev.set(false);
                    show.set(false);
                });
            } else {
                rev.set(false);
                show.set(false);
            }
        }
    };
    let do_revoke = Arc::new(do_revoke);

    // ── Clipboard copy ─────────────────────────────────────────────
    let copy_token = {
        let tok_hex = token_hex;
        let copied = token_copied;
        move || {
            let text = tok_hex.get_untracked();
            if let Some(window) = web_sys::window() {
                let _ = window.navigator().clipboard().write_text(&text);
            }
            copied.set(true);
        }
    };

    // Pre-build boxed callbacks for use inside the view (avoids FnOnce issues)
    let on_create_click: Arc<dyn Fn(web_sys::MouseEvent) + Send + Sync + 'static> = {
        let ci = create_invite.clone();
        Arc::new(move |_| ci())
    };
    let on_revoke_click: Arc<dyn Fn() + Send + Sync + 'static> = {
        let dr = do_revoke.clone();
        Arc::new(move || dr())
    };

    // Wrap non-Copy callbacks in RwSignal so the view capture is Copy-safe
    let create_click_signal: RwSignal<Option<Arc<dyn Fn(web_sys::MouseEvent) + Send + Sync + 'static>>> =
        RwSignal::new(Some(on_create_click.clone()));
    let revoke_click_signal: RwSignal<Option<Arc<dyn Fn() + Send + Sync + 'static>>> =
        RwSignal::new(Some(on_revoke_click.clone()));
    let copy_token_signal: RwSignal<Option<Arc<dyn Fn() + Send + Sync + 'static>>> =
        RwSignal::new(Some(Arc::new(copy_token)));

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h3 class="text-lg font-medium text-foreground">{t!("settings.adminInvites.title")}</h3>
                    <p class="text-sm text-muted-foreground">{t!("settings.adminInvites.description")}</p>
                </div>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    size=Signal::derive(move || ButtonSize::Sm)
                    on_click=Box::new(move |_| show_create.set(true))
                >
                    {t!("settings.adminInvites.create")}
                </Button>
            </div>

            <Separator />

            // Invitations table
            <div class="overflow-x-auto">
                {move || {
                    if loading.get() {
                        view! {
                            <div class="flex items-center justify-center py-8">
                                <span class="h-6 w-6 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                            </div>
                        }.into_any()
                    } else if invites.get().is_empty() {
                        view! {
                            <p class="text-sm text-muted-foreground text-center py-4">
                                {t!("settings.adminInvites.noInvites")}
                            </p>
                        }.into_any()
                    } else {
                        let lang = i18n.locale.get();
                        view! {
                            <table class="w-full text-sm">
                                <thead>
                                    <tr class="border-b text-left text-muted-foreground">
                                        <th class="pb-2 font-medium">{t!("settings.adminInvites.token")}</th>
                                        <th class="pb-2 font-medium">{t!("settings.adminInvites.role")}</th>
                                        <th class="pb-2 font-medium">{t!("settings.adminInvites.uses")}</th>
                                        <th class="pb-2 font-medium">{t!("settings.adminInvites.expires")}</th>
                                        <th class="pb-2 font-medium">{t!("settings.adminInvites.status")}</th>
                                        <th class="pb-2 font-medium">{t!("settings.adminInvites.actions")}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {invites.get().into_iter().map(move |inv| {
                                        let status = invite_status(&inv);
                                        let inv_id = inv.id;
                                        let show_rev = show_revoke;
                                        let rev_target = revoke_target;
                                        view! {
                                            <tr class="border-b last:border-0">
                                                <td class="py-3 pr-4">
                                                    <span class="font-mono text-xs text-foreground">
                                                        {format!("{:08x}...", inv.id.as_u128() & 0xFFFF_FFFF)}
                                                    </span>
                                                </td>
                                                <td class="py-3 pr-4 capitalize text-foreground">
                                                    {inv.role_to_grant.clone()}
                                                </td>
                                                <td class="py-3 pr-4 text-muted-foreground">
                                                    {format!("{}/{}", inv.uses_count, inv.max_uses)}
                                                </td>
                                                <td class="py-3 pr-4 text-muted-foreground">
                                                    {format_date((inv.expires_at as f64) * 1000.0, lang)}
                                                </td>
                                                <td class="py-3 pr-4">
                                                    {status_badge(status, lang)}
                                                </td>
                                                <td class="py-3">
                                                    {move || if status == "active" {
                                                        view! {
                                                            <Button
                                                                variant=Signal::derive(move || ButtonVariant::Ghost)
                                                                size=Signal::derive(move || ButtonSize::Sm)
                                                                class="text-destructive hover:text-destructive"
                                                                on_click=Box::new({
                                                                    let id = inv_id;
                                                                    let show = show_rev;
                                                                    let target = rev_target;
                                                                    move |_| {
                                                                        target.set(Some(id));
                                                                        show.set(true);
                                                                    }
                                                                })
                                                            >
                                                                {t!("settings.adminInvites.revoke")}
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
                        }.into_any()
                    }
                }}
            </div>

            // ── Create Invite Dialog ───────────────────────────────
            <Dialog
                is_open=Signal::derive(move || show_create.get())
                on_close=Box::new(move || show_create.set(false))
            >
                <DialogHeader>
                    <DialogTitle>{t!("settings.adminInvites.createTitle")}</DialogTitle>
                    <DialogDescription>{t!("settings.adminInvites.createDesc")}</DialogDescription>
                </DialogHeader>
                <div class="space-y-4 py-2">
                    <div class="space-y-2">
                        <Label class="text-foreground">{t!("settings.adminInvites.role")}</Label>
                        <Select
                            on_change=Box::new(move |v| new_role.set(v))
                        >
                            <SelectOption value=String::from("user")>{t!("settings.adminInvites.roleUser")}</SelectOption>
                            <SelectOption value=String::from("admin")>{t!("settings.adminInvites.roleAdmin")}</SelectOption>
                        </Select>
                    </div>
                    <div class="space-y-2">
                        <Label class="text-foreground">{t!("settings.adminInvites.maxUses")}</Label>
                        <Input
                            value=new_max_uses.get()
                            on_change=Box::new(move |v| new_max_uses.set(v))
                            placeholder="5".to_string()
                        />
                    </div>
                    <div class="space-y-2">
                        <Label class="text-foreground">{t!("settings.adminInvites.ttl")}</Label>
                        <Select
                            on_change=Box::new(move |v| new_ttl.set(v))
                        >
                            <SelectOption value=String::from("3600")>{t!("settings.adminInvites.ttl1h")}</SelectOption>
                            <SelectOption value=String::from("86400")>{t!("settings.adminInvites.ttl24h")}</SelectOption>
                            <SelectOption value=String::from("604800")>{t!("settings.adminInvites.ttl7d")}</SelectOption>
                            <SelectOption value=String::from("2592000")>{t!("settings.adminInvites.ttl30d")}</SelectOption>
                        </Select>
                    </div>
                </div>
                <DialogFooter>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Outline)
                        on_click=Box::new(move |_| show_create.set(false))
                    >
                        {t!("settings.adminInvites.cancel")}
                    </Button>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Default)
                        on_click=Box::new({
                            let sig = create_click_signal;
                            move |e| {
                                if let Some(ref cb) = sig.get_untracked() {
                                    cb(e);
                                }
                            }
                        })
                    >
                        {t!("settings.adminInvites.create")}
                    </Button>
                </DialogFooter>
            </Dialog>

            // ── Token Display Dialog ───────────────────────────────
            <Dialog
                is_open=Signal::derive(move || show_token.get())
                on_close=Box::new({
                    let tok = token_hex;
                    let copied = token_copied;
                    move || {
                        show_token.set(false);
                        tok.set(String::new());
                        copied.set(false);
                    }
                })
            >
                <DialogHeader>
                    <DialogTitle>{t!("settings.adminInvites.createTitle")}</DialogTitle>
                    <DialogDescription>{t!("settings.adminInvites.createDesc")}</DialogDescription>
                </DialogHeader>
                <div class="space-y-4 py-4">
                    <div class="rounded-lg border bg-muted p-4">
                        <p class="font-mono text-sm break-all text-foreground select-all">
                            {move || token_hex.get()}
                        </p>
                    </div>
                    <div class="rounded-lg border border-yellow-500/50 bg-yellow-500/10 p-3 text-sm text-yellow-600 dark:text-yellow-400">
                        {t!("settings.adminInvites.onceWarning")}
                    </div>
                    <div class="flex justify-end gap-2">
                        <Button
                            variant=Signal::derive(move || if token_copied.get() { ButtonVariant::Secondary } else { ButtonVariant::Default })
                            size=Signal::derive(move || ButtonSize::Sm)
                            on_click=Box::new({
                                let sig = copy_token_signal;
                                move |_| {
                                    if let Some(ref cb) = sig.get_untracked() {
                                        cb();
                                    }
                                }
                            })
                        >
                            {move || if token_copied.get() {
                                t!("settings.adminInvites.tokenCopied")
                            } else {
                                t!("settings.adminInvites.copyToken")
                            }}
                        </Button>
                        <Button
                            variant=Signal::derive(move || ButtonVariant::Outline)
                            size=Signal::derive(move || ButtonSize::Sm)
                            on_click=Box::new({
                                let tok = token_hex;
                                let copied = token_copied;
                                move |_| {
                                    show_token.set(false);
                                    tok.set(String::new());
                                    copied.set(false);
                                }
                            })
                        >
                            {t!("common.close")}
                        </Button>
                    </div>
                </div>
            </Dialog>

            // ── Revoke Confirmation Dialog ─────────────────────────
            <AlertDialog
                is_open=Signal::derive(move || show_revoke.get())
                on_close=Box::new({
                    let target = revoke_target;
                    move || {
                        show_revoke.set(false);
                        target.set(None);
                    }
                })
            >
                <AlertDialogHeader>
                    <AlertDialogTitle>{t!("settings.adminInvites.revokeConfirm")}</AlertDialogTitle>
                    <AlertDialogDescription>
                        {t!("settings.adminInvites.revokeDesc")}
                    </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                    <AlertDialogCancel
                        on_click=Box::new({
                            let target = revoke_target;
                            move || {
                                show_revoke.set(false);
                                target.set(None);
                            }
                        })
                    >
                        {t!("common.cancel")}
                    </AlertDialogCancel>
                    <AlertDialogAction
                        class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                        on_click=Box::new({
                            let sig = revoke_click_signal;
                            move || {
                                if let Some(ref cb) = sig.get_untracked() {
                                    cb();
                                }
                            }
                        })
                    >
                        {move || if revoking.get() {
                            view! {
                                <span class="flex items-center gap-2">
                                    <span class="h-4 w-4 block rounded-full border-2 border-current border-t-transparent animate-spin"/>
                                    {t!("settings.adminInvites.revoke")}
                                </span>
                            }.into_any()
                        } else {
                            view! { {t!("settings.adminInvites.revoke")} }.into_any()
                        }}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialog>
        </div>
    }
}
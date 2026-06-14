use std::sync::Arc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use messenger_proto::admin::*;
use uuid::Uuid;

use crate::components::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter,
    AlertDialogHeader, AlertDialogTitle,
};
use crate::components::avatar::Avatar;
use crate::components::badge::Badge;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::separator::Separator;
use crate::i18n::I18n;
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::session::{build_api_client, Session, SessionState};
use crate::state::users::UsersState;
use crate::t;

/// Short id label, e.g. `019ebdca…`, used when no username is known.
fn short_id_ellipsis(id: Uuid) -> String {
    id.to_string().chars().take(8).collect::<String>() + "…"
}

/// Best-effort (avatar, display name, secondary line) for a user row.
/// The server is blind to usernames/avatars, so everything comes from the
/// admin's local caches: own identity for the admin's own row, and the
/// `UsersState` cache (peers chatted with) for everyone else. Secondary line
/// is `@username` when known, otherwise the short id.
fn resolve_row(
    id: Uuid,
    users: Option<&UsersState>,
    own_id: Option<Uuid>,
    own_username: &Option<String>,
    own_name: &Option<String>,
    own_avatar: &Option<String>,
) -> (Option<String>, String, String) {
    let is_own = Some(id) == own_id;
    let avatar = if is_own {
        own_avatar.clone()
    } else {
        users.and_then(|u| u.avatar_for(id))
    };
    let username = if is_own {
        own_username.clone()
    } else {
        users.and_then(|u| u.username_for(id))
    };
    let name = if is_own {
        own_name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| own_username.clone())
            .unwrap_or_else(|| short_id_ellipsis(id))
    } else {
        users
            .map(|u| u.label_for(id))
            .unwrap_or_else(|| short_id_ellipsis(id))
    };
    let secondary = username
        .map(|n| format!("@{n}"))
        .unwrap_or_else(|| short_id_ellipsis(id));
    (avatar, name, secondary)
}

/// Format a Unix timestamp as YYYY-MM-DD.
fn format_ts(ts: i64) -> String {
    let ms = (ts as f64) * 1000.0;
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    format!(
        "{}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

/// Status badge for a user.
fn user_status_badge(status: &str) -> impl IntoView {
    let (variant, text) = match status {
        "active" => ("default", "Active"),
        "suspended" => ("destructive", "Suspended"),
        _ => ("secondary", "Inactive"),
    };
    view! {
        <Badge variant=String::from(variant) class="capitalize">
            {text}
        </Badge>
    }
}

/// User role display (capitalized).
fn user_role(role: &str) -> String {
    let mut chars = role.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ── Helper component for the suspend confirmation dialog body ────────────

#[must_use]
#[component]
fn SuspendDialogBody(
    suspending: RwSignal<bool>,
    on_confirm: Arc<dyn Fn() + Send + Sync + 'static>,
    on_cancel: Arc<dyn Fn() + Send + Sync + 'static>,
) -> impl IntoView {
    let is_suspending = move || suspending.get();

    view! {
        <AlertDialogHeader>
            <AlertDialogTitle>{t!("settings.adminUsers.suspendConfirm")}</AlertDialogTitle>
            <AlertDialogDescription>
                {t!("settings.adminUsers.suspendDesc")}
            </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
            <AlertDialogCancel
                on_click=Box::new({
                    let on_cancel = on_cancel.clone();
                    move || on_cancel()
                })
            >
                {t!("common.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
                class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                on_click=Box::new({
                    let on_confirm = on_confirm.clone();
                    move || on_confirm()
                })
            >
                {move || if is_suspending() {
                    view! {
                        <span class="flex items-center gap-2">
                            <span class="h-4 w-4 block rounded-full border-2 border-current border-t-transparent animate-spin"/>
                            "Suspending..."
                        </span>
                    }.into_any()
                } else {
                    view! { {t!("settings.adminUsers.suspend")} }.into_any()
                }}
            </AlertDialogAction>
        </AlertDialogFooter>
    }
}

// ── Helper component for the unsuspend confirmation dialog body ──────────

#[must_use]
#[component]
fn UnsuspendDialogBody(
    unsuspending: RwSignal<bool>,
    on_confirm: Arc<dyn Fn() + Send + Sync + 'static>,
    on_cancel: Arc<dyn Fn() + Send + Sync + 'static>,
) -> impl IntoView {
    let is_unsuspending = move || unsuspending.get();

    view! {
        <AlertDialogHeader>
            <AlertDialogTitle>{t!("settings.adminUsers.unsuspendConfirm")}</AlertDialogTitle>
            <AlertDialogDescription>
                {t!("settings.adminUsers.unsuspendDesc")}
            </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
            <AlertDialogCancel
                on_click=Box::new({
                    let on_cancel = on_cancel.clone();
                    move || on_cancel()
                })
            >
                {t!("common.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
                on_click=Box::new({
                    let on_confirm = on_confirm.clone();
                    move || on_confirm()
                })
            >
                {move || if is_unsuspending() {
                    view! {
                        <span class="flex items-center gap-2">
                            <span class="h-4 w-4 block rounded-full border-2 border-current border-t-transparent animate-spin"/>
                            "Unsuspending..."
                        </span>
                    }.into_any()
                } else {
                    view! { {t!("settings.adminUsers.unsuspend")} }.into_any()
                }}
            </AlertDialogAction>
        </AlertDialogFooter>
    }
}

// ── Main component ───────────────────────────────────────────────────────

/// Admin users — table of users with suspend/unsuspend actions backed by real API.
#[must_use]
#[component]
pub fn AdminUsersSettings() -> impl IntoView {
    let _i18n = use_context::<I18n>().expect("I18n must be provided");
    let notifications = use_context::<NotificationsState>()
        .expect("NotificationsState must be provided");
    let users_cache = use_context::<UsersState>();

    // Own identity — the admin's own row is labelled from it (the server is
    // blind to usernames/avatars/display names).
    let (own_user_id, own_username) = match use_context::<Session>().map(|s| s.state.get_untracked())
    {
        Some(SessionState::Authenticated { identity, .. }) => {
            (Some(identity.user_id), Some(identity.username.clone()))
        }
        _ => (None, None),
    };
    let own_name = own_user_id.and_then(crate::state::profile_store::load_display_name);
    let own_avatar = own_user_id.and_then(crate::state::avatar_store::load_own_avatar);
    // Seed the cache with our own username so the own row shows it too.
    if let (Some(cache), Some(uid), Some(uname)) =
        (users_cache.clone(), own_user_id, own_username.clone())
    {
        cache.remember_username(uid, &uname);
    }
    // Copy-friendly bundle so the row closures can read it freely.
    let row_meta = StoredValue::new((
        users_cache.clone(),
        own_user_id,
        own_username.clone(),
        own_name,
        own_avatar,
    ));

    // ── State ──────────────────────────────────────────────────────────
    let users = RwSignal::new(Vec::<AdminUserSummary>::new());
    let loading = RwSignal::new(true);

    // Suspend dialog state
    let show_suspend_dialog = RwSignal::new(false);
    let suspend_target = RwSignal::new(Option::<Uuid>::None);
    let suspending = RwSignal::new(false);

    // Unsuspend dialog state
    let show_unsuspend_dialog = RwSignal::new(false);
    let unsuspend_target = RwSignal::new(Option::<Uuid>::None);
    let unsuspending = RwSignal::new(false);

    // ── Load users on mount ────────────────────────────────────────────
    let notif_load = notifications.clone();
    let users_load = users;
    let loading_load = loading;
    spawn_local({
        let notif = notif_load;
        let usrs = users_load;
        let ld = loading_load;
        async move {
            match build_api_client() {
                Some(client) => match client.list_users().await {
                    Ok(resp) => {
                        usrs.set(resp.users);
                        ld.set(false);
                    }
                    Err(e) => {
                        notif.push(ToastKind::Error, format!("Failed to load users: {e}"));
                        ld.set(false);
                    }
                },
                None => {
                    ld.set(false);
                }
            }
        }
    });

    // ── Refresh users list ─────────────────────────────────────────────
    let refresh = Arc::new({
        let nf = notifications.clone();
        let usrs = users;
        move || {
            let nf = nf.clone();
            let usrs = usrs;
            spawn_local(async move {
                match build_api_client() {
                    Some(client) => match client.list_users().await {
                        Ok(resp) => {
                            usrs.set(resp.users);
                        }
                        Err(e) => {
                            nf.push(ToastKind::Error, format!("Failed to refresh users: {e}"));
                        }
                    },
                    None => {}
                }
            });
        }
    });

    // ── Suspend handler ────────────────────────────────────────────────
    let do_suspend = Arc::new({
        let nf = notifications.clone();
        let refresh = refresh.clone();
        let target = suspend_target;
        let show = show_suspend_dialog;
        let sp = suspending;
        move || {
            let nf = nf.clone();
            let refresh = refresh.clone();
            let target = target;
            let show = show;
            let sp = sp;
            spawn_local(async move {
                let user_id = match target.get_untracked() {
                    Some(id) => id,
                    None => return,
                };

                let api = match build_api_client() {
                    Some(a) => a,
                    None => {
                        nf.push(ToastKind::Error, "Not authenticated".to_string());
                        show.set(false);
                        sp.set(false);
                        target.set(None);
                        return;
                    }
                };

                sp.set(true);

                let req = SuspendUserRequest { reason: None };
                match api.suspend_user(user_id, &req).await {
                    Ok(_) => {
                        nf.push(ToastKind::Success, "User suspended successfully");
                        refresh();
                    }
                    Err(e) => {
                        nf.push(ToastKind::Error, format!("Suspend failed: {e}"));
                    }
                }

                show.set(false);
                sp.set(false);
                target.set(None);
            });
        }
    });

    // ── Unsuspend handler ──────────────────────────────────────────────
    let do_unsuspend = Arc::new({
        let nf = notifications.clone();
        let refresh = refresh.clone();
        let target = unsuspend_target;
        let show = show_unsuspend_dialog;
        let usp = unsuspending;
        move || {
            let nf = nf.clone();
            let refresh = refresh.clone();
            let target = target;
            let show = show;
            let usp = usp;
            spawn_local(async move {
                let user_id = match target.get_untracked() {
                    Some(id) => id,
                    None => return,
                };

                let api = match build_api_client() {
                    Some(a) => a,
                    None => {
                        nf.push(ToastKind::Error, "Not authenticated".to_string());
                        show.set(false);
                        usp.set(false);
                        target.set(None);
                        return;
                    }
                };

                usp.set(true);

                let req = UnsuspendUserRequest { reason: None };
                match api.unsuspend_user(user_id, &req).await {
                    Ok(_) => {
                        nf.push(ToastKind::Success, "User unsuspended successfully");
                        refresh();
                    }
                    Err(e) => {
                        nf.push(ToastKind::Error, format!("Unsuspend failed: {e}"));
                    }
                }

                show.set(false);
                usp.set(false);
                target.set(None);
            });
        }
    });

    // ── Dialog open helpers ────────────────────────────────────────────
    let open_suspend = Arc::new({
        let target = suspend_target;
        let show = show_suspend_dialog;
        move |id: Uuid| {
            target.set(Some(id));
            show.set(true);
        }
    });

    let open_unsuspend = Arc::new({
        let target = unsuspend_target;
        let show = show_unsuspend_dialog;
        move |id: Uuid| {
            target.set(Some(id));
            show.set(true);
        }
    });

    let close_suspend = {
        let show = show_suspend_dialog;
        let target = suspend_target;
        move || {
            show.set(false);
            target.set(None);
        }
    };

    let close_unsuspend = {
        let show = show_unsuspend_dialog;
        let target = unsuspend_target;
        move || {
            show.set(false);
            target.set(None);
        }
    };

    // Wrap non-Copy closures in RwSignal so they can be captured by Copy
    let open_suspend_signal: RwSignal<Option<Arc<dyn Fn(Uuid) + Send + Sync + 'static>>> =
        RwSignal::new(Some(open_suspend.clone()));
    let open_unsuspend_signal: RwSignal<Option<Arc<dyn Fn(Uuid) + Send + Sync + 'static>>> =
        RwSignal::new(Some(open_unsuspend.clone()));
    let close_suspend_signal: RwSignal<Option<Arc<dyn Fn() + Send + Sync + 'static>>> =
        RwSignal::new(Some(Arc::new(close_suspend)));
    let close_unsuspend_signal: RwSignal<Option<Arc<dyn Fn() + Send + Sync + 'static>>> =
        RwSignal::new(Some(Arc::new(close_unsuspend)));

    // Clones for dialog (avoid FnOnce)
    let do_suspend_for_action = do_suspend.clone();
    let close_suspend_for_cancel = close_suspend_signal;
    let suspending_for_body = suspending;

    let do_unsuspend_for_action = do_unsuspend.clone();
    let close_unsuspend_for_cancel = close_unsuspend_signal;
    let unsuspending_for_body = unsuspending;

    // ── View ───────────────────────────────────────────────────────────
    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.adminUsers.title")}</h3>
                <p class="text-sm text-muted-foreground">{t!("settings.adminUsers.description")}</p>
            </div>

            <Separator />

            {move || {
                if loading.get() {
                    // Loading state
                    view! {
                        <div class="flex items-center justify-center py-8">
                            <span class="h-6 w-6 block rounded-full border-2 border-primary border-t-transparent animate-spin"/>
                        </div>
                    }.into_any()
                } else if users.get().is_empty() {
                    // Empty state
                    view! {
                        <p class="text-sm text-muted-foreground text-center py-4">
                            {t!("settings.adminUsers.noUsers")}
                        </p>
                    }.into_any()
                } else {
                    // Users table
                    let on_suspend = open_suspend_signal;
                    let on_unsuspend = open_unsuspend_signal;
                    view! {
                        <div class="overflow-x-auto">
                            <table class="w-full text-sm">
                                <thead>
                                    <tr class="border-b text-left text-muted-foreground">
                                        // Headers mirror the body cells' `pr-4` so the
                                        // titles align with their columns and sit evenly
                                        // apart instead of one crowding the next.
                                        <th class="pb-2 pr-4 font-medium whitespace-nowrap">{t!("settings.adminUsers.user")}</th>
                                        <th class="pb-2 pr-4 font-medium whitespace-nowrap">{t!("settings.adminUsers.role")}</th>
                                        <th class="pb-2 pr-4 font-medium whitespace-nowrap">{t!("settings.adminUsers.status")}</th>
                                        <th class="pb-2 pr-4 font-medium whitespace-nowrap">{t!("settings.adminUsers.created")}</th>
                                        <th class="pb-2 pr-4 font-medium whitespace-nowrap">{t!("settings.adminUsers.devicesCount")}</th>
                                        <th class="pb-2 font-medium whitespace-nowrap">{t!("settings.adminUsers.actions")}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || users.get().into_iter().map(move |u| {
                                        let user_id = u.id;
                                        let is_suspended = u.status == "suspended";
                                        let role_display = user_role(&u.role);
                                        let created_display = format_ts(u.created_at);
                                        let devices_count = u.devices_count;

                                        let (avatar, name, secondary) = {
                                            let (ref cache, own_id, ref own_uname, ref own_name, ref own_avatar) = row_meta.get_value();
                                            resolve_row(user_id, cache.as_ref(), own_id, own_uname, own_name, own_avatar)
                                        };

                                        let on_suspend = on_suspend;
                                        let on_unsuspend = on_unsuspend;

                                        view! {
                                            <tr class="border-b last:border-0">
                                                <td class="py-3 pr-4">
                                                    <div class="flex items-center gap-3">
                                                        <Avatar src=avatar alt=name.clone() class="h-8 w-8 shrink-0" />
                                                        <div class="min-w-0">
                                                            <div class="font-medium text-foreground truncate">{name}</div>
                                                            <div class="text-xs text-muted-foreground font-mono truncate">{secondary}</div>
                                                        </div>
                                                    </div>
                                                </td>
                                                <td class="py-3 pr-4 capitalize text-foreground">
                                                    {role_display.clone()}
                                                </td>
                                                <td class="py-3 pr-4">
                                                    {user_status_badge(&u.status)}
                                                </td>
                                                <td class="py-3 pr-4 text-muted-foreground">
                                                    {created_display.clone()}
                                                </td>
                                                <td class="py-3 pr-4 text-muted-foreground">
                                                    {devices_count}
                                                </td>
                                                <td class="py-3">
                                    {
                                                        if is_suspended {
                                                            let on_unsuspend = on_unsuspend;
                                                            let uid = user_id;
                                                            view! {
                                                                <Button
                                                                    variant=Signal::derive(move || ButtonVariant::Default)
                                                                    size=Signal::derive(move || ButtonSize::Sm)
                                                                    on_click=Box::new(move |_| {
                                                                        if let Some(ref cb) = on_unsuspend.get_untracked() {
                                                                            cb(uid);
                                                                        }
                                                                    })
                                                                >
                                                                    {t!("settings.adminUsers.unsuspend")}
                                                                </Button>
                                                            }.into_any()
                                                        } else {
                                                            let on_suspend = on_suspend;
                                                            let uid = user_id;
                                                            view! {
                                                                <Button
                                                                    variant=Signal::derive(move || ButtonVariant::Destructive)
                                                                    size=Signal::derive(move || ButtonSize::Sm)
                                                                    on_click=Box::new(move |_| {
                                                                        if let Some(ref cb) = on_suspend.get_untracked() {
                                                                            cb(uid);
                                                                        }
                                                                    })
                                                                >
                                                                    {t!("settings.adminUsers.suspend")}
                                                                </Button>
                                                            }.into_any()
                                                        }
                                                    }
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }
            }}
        </div>

        // ── Suspend Confirmation Dialog ────────────────────────────────
        <AlertDialog
            is_open=Signal::derive(move || show_suspend_dialog.get())
            on_close=Box::new({
                let sig = close_suspend_for_cancel;
                move || {
                    if let Some(ref cb) = sig.get_untracked() {
                        cb();
                    }
                }
            })
        >
            <SuspendDialogBody
                suspending=suspending_for_body
                on_confirm=do_suspend_for_action.clone()
                on_cancel=Arc::new({
                    let sig = close_suspend_for_cancel;
                    move || {
                        if let Some(ref cb) = sig.get_untracked() {
                            cb();
                        }
                    }
                })
            />
        </AlertDialog>

        // ── Unsuspend Confirmation Dialog ──────────────────────────────
        <AlertDialog
            is_open=Signal::derive(move || show_unsuspend_dialog.get())
            on_close=Box::new({
                let sig = close_unsuspend_for_cancel;
                move || {
                    if let Some(ref cb) = sig.get_untracked() {
                        cb();
                    }
                }
            })
        >
            <UnsuspendDialogBody
                unsuspending=unsuspending_for_body
                on_confirm=do_unsuspend_for_action.clone()
                on_cancel=Arc::new({
                    let sig = close_unsuspend_for_cancel;
                    move || {
                        if let Some(ref cb) = sig.get_untracked() {
                            cb();
                        }
                    }
                })
            />
        </AlertDialog>
    }
}

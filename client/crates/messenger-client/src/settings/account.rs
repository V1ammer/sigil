use std::sync::Arc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use messenger_core::blind_index::username_blind_index;
use messenger_core::api::ApiError;
use messenger_core::ed25519::Ed25519Pair;
use messenger_core::identity::ClientIdentity;
use messenger_proto::users::ChangeUsernameRequest;
use crate::components::avatar_picker::AvatarPicker;
use crate::components::button::{Button, ButtonVariant};
use crate::components::input::Input;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::components::dialog::{Dialog, DialogHeader, DialogTitle, DialogDescription, DialogFooter};
use crate::components::alert_dialog::{AlertDialog, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction};
use crate::state::session::{build_api_client, use_session, SessionState};
use crate::state::notifications::{NotificationsState, ToastKind};
use crate::state::settings::SettingsState;
use crate::session::restore::clear_persisted_session;
use crate::t;

/// Format a user ID as a short UUID (first 8 hex characters).
fn short_uuid(id: &uuid::Uuid) -> String {
    let hex = id.to_string();
    hex[..8].to_string()
}

/// Format the safety number from the first 16 bytes of a public key.
/// Displayed as four groups of 4 hex characters separated by spaces.
fn format_safety_number(pubkey: &[u8; 32]) -> String {
    let mut groups = Vec::new();
    for chunk in pubkey[..16].chunks(4) {
        let hex_str: String = chunk.iter().map(|b| format!("{b:02x}")).collect();
        groups.push(hex_str);
    }
    groups.join(" ")
}

/// Account settings — display name, username, bio, avatar, logout.
#[must_use]
#[component]
pub fn AccountSettings() -> impl IntoView {
    let session = use_session();
    let navigate = use_navigate();
    let notifications = use_context::<NotificationsState>()
        .expect("NotificationsState must be provided");

    // Derive identity from session state
    let (identity, _role) = match session.state.get() {
        SessionState::Authenticated { identity, role } => (Some(identity), Some(role)),
        _ => (None, None),
    };

    // Local editable fields — display name falls back to the username until
    // the user sets one; both live in the local profile store.
    let display_name = RwSignal::new(identity.as_ref().map_or_else(String::new, |id| {
        crate::state::profile_store::load_display_name(id.user_id)
            .unwrap_or_else(|| id.username.clone())
    }));
    let bio = RwSignal::new(
        identity
            .as_ref()
            .map_or_else(String::new, |id| crate::state::profile_store::load_bio(id.user_id)),
    );

    // Change username dialog state
    let show_change_username = RwSignal::new(false);
    let new_username = RwSignal::new(String::new());

    // Logout confirmation dialog state
    let show_logout_confirm = RwSignal::new(false);

    // Compute safety number from identity signing public key
    let safety_number = RwSignal::new(identity.as_ref().map_or_else(
        String::new,
        |id| format_safety_number(&id.identity_signing_key.public_bytes()),
    ));

    // Format user ID
    let user_id_str = RwSignal::new(identity.as_ref().map_or_else(String::new, |id| short_uuid(&id.user_id)));

    // Username (read-only from session)
    let username = RwSignal::new(identity.as_ref().map_or_else(String::new, |id| id.username.clone()));

    // Avatar — initialized from the local store; saving/broadcasting happens
    // in the change effect below (the initial value must not re-trigger it).
    let my_user_id = identity.as_ref().map(|id| id.user_id);
    let avatar_sig = RwSignal::new(my_user_id.and_then(crate::state::avatar_store::load_own_avatar));
    {
        let notifications = notifications.clone();
        let msg_svc = use_context::<crate::state::message_service::MessageService>();
        let last_seen = StoredValue::new(avatar_sig.get_untracked());
        Effect::new(move |_| {
            let current = avatar_sig.get();
            if last_seen.get_value() == current {
                return;
            }
            last_seen.set_value(current.clone());
            let Some(uid) = my_user_id else { return };
            let removed = current.is_none();
            match current {
                Some(ref data_url) => crate::state::avatar_store::save_own_avatar(uid, data_url),
                None => crate::state::avatar_store::clear_own_avatar(uid),
            }
            notifications.push(
                ToastKind::Success,
                if removed {
                    t!("settings.account.avatarRemoved")
                } else {
                    t!("settings.account.avatarSaved")
                },
            );
            // Deliver the change (or removal) to every existing chat.
            #[cfg(target_arch = "wasm32")]
            web_sys::console::log_1(
                &format!("[avatar] settings change, svc present: {}", msg_svc.is_some()).into(),
            );
            if let Some(svc) = msg_svc.clone() {
                spawn_local(async move {
                    svc.broadcast_avatar_all().await;
                });
            }
        });
    }

    // --- Handlers ---

    let on_change_username_confirm = {
        let notifications = notifications.clone();
        move |_| {
            let uname = new_username.get_untracked().trim().to_string();
            if uname.is_empty() {
                notifications.push(ToastKind::Error, t!("settings.account.usernameEmpty"));
                return;
            }

            show_change_username.set(false);
            let notifications = notifications.clone();

            spawn_local(async move {
                // 1. Retrieve blind index key from local storage
                let blind_index_key = get_blind_index_key().await;

                // 2. Compute blind index
                let username_bi = match blind_index_key {
                    Some(key) => {
                        match username_blind_index(&uname, &key) {
                            Ok(bi) => bi,
                            Err(e) => {
                                notifications.push(
                                    ToastKind::Error,
                                    format!("{}: {e}", t!("settings.account.blindIndexError")),
                                );
                                return;
                            }
                        }
                    }
                    None => {
                        notifications.push(
                            ToastKind::Error,
                            t!("settings.account.missingBlindIndexKey"),
                        );
                        return;
                    }
                };

                // 3. Build API client and call change_username
                let api = match build_api_client() {
                    Some(client) => client,
                    None => {
                        notifications.push(ToastKind::Error, t!("settings.account.apiClientError"));
                        return;
                    }
                };

                let req = ChangeUsernameRequest {
                    new_username_blind_index: username_bi,
                };

                match api.change_username(&req).await {
                    Ok(()) => {
                        // Username is client-local truth here — the server only
                        // keeps its blind index. Update the persisted blob and
                        // the live session so the new handle shows immediately
                        // and survives reload; no re-login needed.
                        let _ = crate::session::restore::update_persisted_username(&uname);
                        if let SessionState::Authenticated { identity: id, role } =
                            session.state.get_untracked()
                        {
                            let updated = ClientIdentity {
                                user_id: id.user_id,
                                username: uname.clone(),
                                identity_signing_key: Ed25519Pair::from_seed(
                                    &id.identity_signing_key.secret_bytes(),
                                ),
                                device_id: id.device_id,
                                device_signing_key: Ed25519Pair::from_seed(
                                    &id.device_signing_key.secret_bytes(),
                                ),
                                device_hpke_seed: id.device_hpke_seed,
                                device_hpke_public: id.device_hpke_public,
                            };
                            session.state.set(SessionState::Authenticated {
                                identity: Arc::new(updated),
                                role,
                            });
                        }
                        username.set(uname.clone());
                        notifications.push(
                            ToastKind::Success,
                            t!("settings.account.usernameChanged").to_string(),
                        );
                    }
                    Err(ApiError::Api { status: 409, .. }) => {
                        notifications.push(
                            ToastKind::Error,
                            format!("{}", t!("settings.account.usernameTaken")),
                        );
                    }
                    Err(e) => {
                        notifications.push(
                            ToastKind::Error,
                            format!("{}: {e}", t!("settings.account.usernameChangeError")),
                        );
                    }
                }
            });
        }
    };

    let on_logout_confirm = {
        let navigate = navigate.clone();
        let notifications = notifications.clone();
        move || {
            show_logout_confirm.set(false);

            // a. Clear persisted session
            clear_persisted_session();

            // b. Clear device credentials from localStorage
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
            {
                let _ = storage.remove_item("messenger_device_id");
                let _ = storage.remove_item("messenger_device_signing_secret");
            }

            // c. Wipe all settings
            SettingsState::wipe_all();

            // d. Set session to Disconnected
            session.state.set(SessionState::Disconnected);

            // e. Navigate to root
            _ = navigate("/", Default::default());
        }
    };

    let on_change_username_confirm = Arc::new(on_change_username_confirm);
    let on_logout_confirm = Arc::new(on_logout_confirm);

    // Wrap in RwSignal (Copy) so the view! macro's outer closure doesn't need to move non-Copy values
    let on_change_username_sig = RwSignal::new(on_change_username_confirm);
    let on_logout_sig = RwSignal::new(on_logout_confirm);

    view! {
        <div class="space-y-6">
            // Header
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.account.title")}</h3>
                <p class="text-sm text-muted-foreground">{t!("settings.account.description")}</p>
            </div>

            <Separator />

            // Avatar section — picker saves locally and (when chats exist)
            // broadcasts the new avatar to every group via MLS.
            <div class="flex items-center gap-4">
                <AvatarPicker value=avatar_sig size_class="h-16 w-16"/>
                <div class="space-y-1">
                    <p class="text-sm font-medium text-foreground">{move || display_name.get()}</p>
                    <p class="text-xs text-muted-foreground">{move || format!("@{}", username.get())}</p>
                    {move || avatar_sig.get().map(|_| view! {
                        <Button
                            variant=Signal::derive(move || ButtonVariant::Outline)
                            size=Signal::derive(move || crate::components::button::ButtonSize::Sm)
                            on_click=Box::new(move |_| avatar_sig.set(None))
                        >
                            {t!("settings.account.removeAvatar")}
                        </Button>
                    })}
                </div>
            </div>

            <Separator />

            // Display name (locally editable)
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.account.displayName")}</Label>
                <Input
                    value=display_name.get()
                    on_change=Box::new(move |v| display_name.set(v))
                />
                <p class="text-xs text-muted-foreground">{t!("settings.account.displayNameHint")}</p>
            </div>

            // Username — shown reactively (updates the moment it's changed) with
            // a Change button. It's editable via the dialog, so no "can't be
            // changed" copy here.
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.account.username")}</Label>
                <div class="flex items-center gap-2">
                    <div class="flex h-10 flex-1 items-center rounded-md border border-input bg-muted px-3 text-sm text-foreground">
                        {move || format!("@{}", username.get())}
                    </div>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Outline)
                        on_click=Box::new({
                            let notifications = notifications.clone();
                            move |_| {
                                if identity.is_none() {
                                    notifications.push(ToastKind::Error, t!("settings.account.notAuthenticated"));
                                    return;
                                }
                                show_change_username.set(true);
                                new_username.set(String::new());
                            }
                        })
                    >
                        {t!("settings.account.changeUsername")}
                    </Button>
                </div>
                <p class="text-xs text-muted-foreground">{t!("settings.account.usernameHint")}</p>
            </div>

            // Bio (locally editable)
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.account.bio")}</Label>
                <textarea
                    class="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                    placeholder=t!("settings.account.bioPlaceholder")
                    prop:value=bio.get()
                    on:input=move |ev| bio.set(event_target_value(&ev))
                />
            </div>

            <Separator />

            // User ID (short UUID)
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.account.userId")}</Label>
                <div class="rounded-md bg-muted p-3 font-mono text-xs text-foreground break-all select-all">
                    {user_id_str.get()}
                </div>
            </div>

            // Safety number
            <div class="space-y-2">
                <Label class="text-foreground">{t!("settings.account.safetyNumber")}</Label>
                <div class="rounded-md bg-muted p-3 font-mono text-xs text-foreground break-all select-all">
                    {safety_number.get()}
                </div>
                <p class="text-xs text-muted-foreground">{t!("settings.account.safetyHint")}</p>
            </div>

            <Separator />

            // Privacy note
            <div class="rounded-md border border-muted bg-muted/50 p-3">
                <p class="text-xs text-muted-foreground">{t!("settings.account.privacyNote")}</p>
            </div>

            // Save: persist display name + bio locally. The name reaches
            // peers via sender_display_name_override; the changed profile
            // fingerprint makes ensure_avatar_broadcasts re-announce to all
            // chats, so the new name is delivered without waiting for the
            // next regular message.
            <div class="flex justify-end">
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    on_click=Box::new(move |_| {
                        let Some(uid) = my_user_id else { return };
                        crate::state::profile_store::save_display_name(uid, &display_name.get_untracked());
                        crate::state::profile_store::save_bio(uid, &bio.get_untracked());
                        notifications.push(
                            ToastKind::Success,
                            format!("{}", t!("settings.account.saveLocalSuccess")),
                        );
                        if let Some(svc) = crate::state::message_service::service_handle() {
                            spawn_local(async move {
                                svc.ensure_avatar_broadcasts().await;
                            });
                        }
                    })
                >
                    {t!("settings.account.save")}
                </Button>
            </div>

            <Separator />

            // Sign out
            <div class="space-y-3">
                <h4 class="text-sm font-medium text-foreground">{t!("settings.account.signOutSection")}</h4>
                <p class="text-xs text-muted-foreground">{t!("settings.account.signOutHint")}</p>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Destructive)
                    on_click=Box::new(move |_| {
                        show_logout_confirm.set(true);
                    })
                >
                    {t!("settings.account.signOut")}
                </Button>
            </div>
        </div>

        // Change Username Dialog
        <Dialog
            is_open=show_change_username
            on_close=Box::new(move || show_change_username.set(false))
        >
            <DialogHeader>
                <DialogTitle>{t!("settings.account.changeUsernameTitle")}</DialogTitle>
                <DialogDescription>{t!("settings.account.changeUsernameDescription")}</DialogDescription>
            </DialogHeader>
            <div class="py-4">
                <Input
                    value=new_username.get()
                    on_change=Box::new(move |v| new_username.set(v))
                    placeholder=t!("settings.account.newUsernamePlaceholder")
                />
            </div>
            <DialogFooter>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Outline)
                    on_click=Box::new(move |_| show_change_username.set(false))
                >
                    {t!("settings.account.cancel")}
                </Button>
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    on_click=Box::new({
                            let sig = on_change_username_sig;
                            move |e| {
                                let cb = sig.get_untracked();
                                cb(e);
                            }
                        })
                >
                    {t!("settings.account.confirm")}
                </Button>
            </DialogFooter>
        </Dialog>

        // Logout Confirmation AlertDialog
        <AlertDialog
            is_open=show_logout_confirm
            on_close=Box::new(move || show_logout_confirm.set(false))
        >
            <AlertDialogHeader>
                <AlertDialogTitle>{t!("settings.account.signOutConfirmTitle")}</AlertDialogTitle>
                <AlertDialogDescription>{t!("settings.account.signOutConfirmDescription")}</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
                <AlertDialogCancel on_click=Box::new(move || show_logout_confirm.set(false))>
                    {t!("settings.account.cancel")}
                </AlertDialogCancel>
                <AlertDialogAction
                    class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                    on_click=Box::new({
                            let sig = on_logout_sig;
                            move || {
                                let cb = sig.get_untracked();
                                cb();
                            }
                        })
                >
                    {t!("settings.account.signOut")}
                </AlertDialogAction>
            </AlertDialogFooter>
        </AlertDialog>
    }
}

/// Retrieve the blind index key from local storage.
async fn get_blind_index_key() -> Option<[u8; 32]> {
    if let Ok(local) = messenger_storage::init_storage("default").await {
        if let Ok(Some(hex_key)) = local.get_setting("server_blind_index_key").await {
            let bytes = hex::decode(&hex_key).ok()?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return Some(key);
            }
        }
        if let Ok(Some(hex_key)) = local.get_setting("username_blind_index_key").await {
            let bytes = hex::decode(&hex_key).ok()?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return Some(key);
            }
        }
    }
    None
}

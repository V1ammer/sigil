//! Group management sheet — member list, add/remove (owner), rename, and leave
//! (owner picks a successor). Reads its own context and talks to the server via
//! the MLS group helpers in `state::message_service`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;

use crate::state::chats::ChatsState;
use crate::state::session::{build_api_client, Session};
use crate::state::users::UsersState;

/// One row in the member list.
#[derive(Clone)]
struct MemberRow {
    user_id: Uuid,
    role: String,
}

#[must_use]
#[component]
pub fn GroupSheet(
    group_id: Uuid,
    #[prop(into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) -> impl IntoView {
    let users = use_context::<UsersState>();
    let chats = use_context::<ChatsState>().expect("ChatsState must be provided");
    let own_uid = use_context::<Session>().and_then(|s| s.current_user_id());
    // Box isn't Clone; share the close callback across handlers via Arc.
    let on_close: Option<std::sync::Arc<dyn Fn() + Send + Sync + 'static>> =
        on_close.map(std::sync::Arc::from);

    let members: RwSignal<Vec<MemberRow>> = RwSignal::new(Vec::new());
    let busy = RwSignal::new(false);
    let err = RwSignal::new(String::new());
    let add_input = RwSignal::new(String::new());
    let name_input = RwSignal::new(String::new());
    // Owner-leave successor picker.
    let leaving = RwSignal::new(false);
    let successor: RwSignal<Option<Uuid>> = RwSignal::new(None);

    // Reload the member list from the server.
    let reload = move || {
        spawn_local(async move {
            if let Some(api) = build_api_client() {
                if let Ok(resp) = api.list_group_members(group_id).await {
                    let rows: Vec<MemberRow> = resp
                        .members
                        .iter()
                        .filter(|m| m.left_at_epoch.is_none())
                        .map(|m| MemberRow {
                            user_id: m.user_id,
                            role: m.role_in_chat.clone(),
                        })
                        .collect();
                    members.set(rows);
                }
            }
        });
    };

    // Load whenever the sheet opens.
    Effect::new(move |_| {
        if is_open.get() {
            err.set(String::new());
            leaving.set(false);
            reload();
        }
    });

    let own_role = move || {
        members
            .get()
            .iter()
            .find(|m| Some(m.user_id) == own_uid)
            .map(|m| m.role.clone())
            .unwrap_or_default()
    };
    let is_owner = move || own_role() == "owner";

    // Display label for a member.
    let label = {
        let users = users.clone();
        move |uid: Uuid| {
            users
                .as_ref()
                .and_then(|u| u.username_for(uid).or_else(|| u.get(uid)))
                .unwrap_or_else(|| {
                    let s = uid.to_string();
                    s.chars().take(8).collect::<String>() + "…"
                })
        }
    };

    // Add a member by username.
    let do_add = move |_| {
        let username = add_input.get().trim().to_string();
        if username.is_empty() {
            return;
        }
        busy.set(true);
        err.set(String::new());
        spawn_local(async move {
            if let Some(api) = build_api_client() {
                match crate::state::message_service::group_add_member(&api, group_id, &username)
                    .await
                {
                    Ok(()) => {
                        if let Some(svc) = crate::state::message_service::service_handle() {
                            let _ = svc
                                .send_system_note(
                                    group_id,
                                    &format!("{} добавлен(а) в группу", username.trim()),
                                )
                                .await;
                        }
                        add_input.set(String::new());
                        reload();
                    }
                    Err(e) => err.set(e),
                }
            }
            busy.set(false);
        });
    };

    // Remove a member by user id (owner only).
    let do_remove = {
        let label = label.clone();
        move |uid: Uuid| {
            let name = label(uid);
            busy.set(true);
            err.set(String::new());
            spawn_local(async move {
                if let Some(api) = build_api_client() {
                    match crate::state::message_service::group_remove_member(&api, group_id, uid)
                        .await
                    {
                        Ok(()) => {
                            if let Some(svc) = crate::state::message_service::service_handle() {
                                let _ = svc
                                    .send_system_note(
                                        group_id,
                                        &format!("{name} удалён(а) из группы"),
                                    )
                                    .await;
                            }
                            reload();
                        }
                        Err(e) => err.set(e),
                    }
                }
                busy.set(false);
            });
        }
    };

    // Rename the group (owner only) — broadcast end-to-end.
    let do_rename = {
        let chats = chats.clone();
        move |_| {
            let name = name_input.get().trim().to_string();
            if name.is_empty() {
                return;
            }
            busy.set(true);
            let chats = chats.clone();
            spawn_local(async move {
                if let Some(svc) = crate::state::message_service::service_handle() {
                    if svc
                        .send_group_update(group_id, Some(name.clone()), None)
                        .await
                        .is_some()
                    {
                        chats.set_display_name(group_id, &name);
                    }
                }
                name_input.set(String::new());
                busy.set(false);
            });
        }
    };

    // Pick + set the group avatar (owner). Reads the chosen image and hands it
    // to set_group_avatar (compress + encrypt + upload + broadcast).
    let do_avatar = move |ev: leptos::ev::Event| {
        use wasm_bindgen::JsCast;
        let input = event_target::<web_sys::HtmlInputElement>(&ev);
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };
        let mime = file.type_();
        busy.set(true);
        err.set(String::new());
        spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buf) => {
                    let arr: js_sys::ArrayBuffer = buf.unchecked_into();
                    let bytes = js_sys::Uint8Array::new(&arr).to_vec();
                    if let Some(svc) = crate::state::message_service::service_handle() {
                        let _ = svc.set_group_avatar(group_id, bytes, mime).await;
                    }
                }
                Err(_) => err.set("не удалось прочитать файл".into()),
            }
            busy.set(false);
        });
    };

    // Leave the group. Owner must transfer ownership to the chosen successor
    // first; everyone else just leaves.
    let do_leave = {
        let chats = chats.clone();
        let on_close = on_close.clone();
        let label = label.clone();
        move || {
            let owner = is_owner();
            let succ = successor.get();
            if owner && succ.is_none() {
                err.set("выберите, кому передать права".into());
                return;
            }
            busy.set(true);
            err.set(String::new());
            let chats = chats.clone();
            let on_close = on_close.clone();
            let my_name = own_uid.map(|u| label(u)).unwrap_or_default();
            spawn_local(async move {
                if let Some(api) = build_api_client() {
                    if owner {
                        if let Some(s) = succ {
                            if let Err(e) = api.transfer_owner(group_id, s).await {
                                err.set(format!("передача прав не удалась: {e}"));
                                busy.set(false);
                                return;
                            }
                        }
                    }
                    // Announce the departure while still a member (can't post
                    // after leaving).
                    if let Some(svc) = crate::state::message_service::service_handle() {
                        let _ = svc
                            .send_system_note(group_id, &format!("{my_name} покинул(а) группу"))
                            .await;
                    }
                    match crate::state::message_service::group_leave(&api, group_id).await {
                        Ok(()) => {
                            chats.delete_chat(group_id);
                            chats.selected.set(None);
                            let _ = chats.load_from_server(&api).await;
                            if let Some(cb) = on_close.as_ref() {
                                cb();
                            }
                            crate::state::back_stack::pop();
                        }
                        Err(e) => err.set(e),
                    }
                }
                busy.set(false);
            });
        }
    };

    view! {
        {move || {
            if !is_open.get() {
                return view! {}.into_any();
            }
            let do_add = do_add.clone();
            let do_remove = do_remove.clone();
            let do_rename = do_rename.clone();
            let do_leave = do_leave.clone();
            let label = label.clone();
            view! {
                <div class="fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-black/50 animate-overlay-in">
                    <div class="bg-card w-full sm:max-w-md sm:rounded-lg rounded-t-2xl shadow-xl border border-border p-5 max-h-[85vh] overflow-y-auto animate-sheet-bottom-in">
                        <div class="flex items-center justify-between mb-4">
                            <h2 class="text-lg font-semibold text-foreground">"Участники группы"</h2>
                        </div>

                        // Member list
                        <div class="space-y-1">
                            <For
                                each=move || members.get()
                                key=|m| m.user_id
                                children={
                                    let label = label.clone();
                                    let do_remove = do_remove.clone();
                                    move |m: MemberRow| {
                                        let label = label.clone();
                                        let do_remove = do_remove.clone();
                                        let uid = m.user_id;
                                        let is_self = Some(uid) == own_uid;
                                        let role = m.role.clone();
                                        view! {
                                            <div class="flex items-center justify-between rounded-md px-2 py-2 hover:bg-accent/50">
                                                <span class="text-sm text-foreground truncate">{label(uid)}</span>
                                                <div class="flex items-center gap-2">
                                                    {(role == "owner").then(|| view! {
                                                        <span class="text-xs text-muted-foreground">"владелец"</span>
                                                    })}
                                                    {move || (is_owner() && !is_self).then(|| {
                                                        let do_remove = do_remove.clone();
                                                        view! {
                                                            <button
                                                                class="text-destructive text-xs hover:underline disabled:opacity-50"
                                                                on:click=move |_| do_remove(uid)
                                                                disabled=move || busy.get()
                                                            >"убрать"</button>
                                                        }
                                                    })}
                                                </div>
                                            </div>
                                        }
                                    }
                                }
                            />
                        </div>

                        // Owner-only: add member + rename
                        {move || is_owner().then({
                            let do_add = do_add.clone();
                            let do_rename = do_rename.clone();
                            let do_avatar = do_avatar.clone();
                            move || {
                                let do_add = do_add.clone();
                                let do_rename = do_rename.clone();
                                let do_avatar = do_avatar.clone();
                                view! {
                                    <div class="mt-4 border-t border-border pt-4 space-y-3">
                                        <div class="flex gap-2">
                                            <input
                                                type="text"
                                                class="flex-1 px-3 py-2 rounded-md border border-input bg-background text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                                                placeholder="Добавить участника (имя пользователя)"
                                                prop:value=add_input
                                                on:input=move |ev| add_input.set(event_target_value(&ev))
                                                disabled=move || busy.get()
                                            />
                                            <button
                                                class="inline-flex h-10 px-3 items-center justify-center rounded-md bg-secondary text-secondary-foreground text-sm hover:bg-secondary/80 disabled:opacity-50"
                                                on:click=do_add
                                                disabled=move || busy.get()
                                            >"Добавить"</button>
                                        </div>
                                        <div class="flex gap-2">
                                            <input
                                                type="text"
                                                class="flex-1 px-3 py-2 rounded-md border border-input bg-background text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                                                placeholder="Новое название группы"
                                                prop:value=name_input
                                                on:input=move |ev| name_input.set(event_target_value(&ev))
                                                disabled=move || busy.get()
                                            />
                                            <button
                                                class="inline-flex h-10 px-3 items-center justify-center rounded-md bg-secondary text-secondary-foreground text-sm hover:bg-secondary/80 disabled:opacity-50"
                                                on:click=do_rename
                                                disabled=move || busy.get()
                                            >"Переименовать"</button>
                                        </div>
                                        <label class="inline-flex h-10 px-3 items-center justify-center rounded-md bg-secondary text-secondary-foreground text-sm hover:bg-secondary/80 cursor-pointer w-fit">
                                            "Сменить аватар"
                                            <input
                                                type="file"
                                                accept="image/*"
                                                class="hidden"
                                                on:change=do_avatar
                                                disabled=move || busy.get()
                                            />
                                        </label>
                                    </div>
                                }
                            }
                        })}

                        // Error
                        {move || {
                            let e = err.get();
                            (!e.is_empty()).then(|| view! {
                                <p class="mt-3 text-sm text-destructive">{e}</p>
                            })
                        }}

                        // Owner-leave successor picker
                        {move || (is_owner() && leaving.get()).then(|| view! {
                            <div class="mt-4">
                                <label class="text-xs text-muted-foreground">"Передать права участнику:"</label>
                                <select
                                    class="mt-1 w-full px-3 py-2 rounded-md border border-input bg-background text-foreground text-sm"
                                    on:change=move |ev| {
                                        let v = event_target_value(&ev);
                                        successor.set(Uuid::parse_str(&v).ok());
                                    }
                                >
                                    <option value="">"— выберите —"</option>
                                    <For
                                        each=move || {
                                            let others: Vec<MemberRow> = members
                                                .get()
                                                .into_iter()
                                                .filter(|m| Some(m.user_id) != own_uid)
                                                .collect();
                                            others
                                        }
                                        key=|m| m.user_id
                                        children={
                                            let label = label.clone();
                                            move |m: MemberRow| {
                                                let uid = m.user_id;
                                                view! { <option value=uid.to_string()>{label(uid)}</option> }
                                            }
                                        }
                                    />
                                </select>
                            </div>
                        })}

                        // Actions
                        <div class="mt-5 flex items-center justify-between gap-3">
                            <button
                                class="text-destructive text-sm font-medium hover:underline disabled:opacity-50"
                                on:click={
                                    let do_leave = do_leave.clone();
                                    move |_| {
                                        // Owner needs to pick a successor first; reveal the picker.
                                        if is_owner() && !leaving.get() && members.get().len() > 1 {
                                            leaving.set(true);
                                        } else {
                                            do_leave();
                                        }
                                    }
                                }
                                disabled=move || busy.get()
                            >"Выйти из группы"</button>
                            <button
                                class="inline-flex h-9 px-4 items-center justify-center rounded-md border border-input bg-background text-foreground text-sm hover:bg-accent"
                                on:click={
                                    let on_close = on_close.clone();
                                    move |_| { if let Some(cb) = on_close.as_ref() { cb(); } }
                                }
                            >"Закрыть"</button>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

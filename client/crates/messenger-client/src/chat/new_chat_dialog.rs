//! New chat dialog — create a new direct or group chat.
use std::sync::Arc;
use leptos::prelude::*;
use crate::i18n::{Language, t};
use crate::mock::User;
use crate::icons::Icon;
use crate::components::dialog::{Dialog, DialogHeader, DialogTitle, DialogDescription, DialogFooter};
use crate::components::button::{Button, ButtonVariant};
use crate::components::input::Input;
use crate::components::tabs::{Tabs, TabsList, TabsTrigger, TabsContent};
use crate::components::avatar::get_initials;
use crate::components::scroll_area::ScrollArea;
use crate::components::separator::Separator;

/// Renders the direct-chat user list. Extracted into a component to avoid
/// capturing non-Copy values in the parent view! macro closure.
#[must_use]
#[component]
fn DirectUserList(
    users: Vec<User>,
    on_select: Arc<dyn Fn(String) + Send + Sync + 'static>,
) -> impl IntoView {
    let users = Arc::new(users);
    view! {
        {users.iter().map(|user| {
            let uid = user.id.clone();
            let uname = user.display_name.clone();
            let uusername = user.username.clone();
            let hsd = on_select.clone();
            view! {
                <button
                    class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm hover:bg-accent transition-colors"
                    on:click=move |_| hsd(uid.clone())
                >
                    <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-muted text-sm font-medium text-foreground">
                        {get_initials(&uname)}
                    </div>
                    <div class="min-w-0 flex-1 text-left">
                        <p class="font-medium truncate">{uname}</p>
                        <p class="text-xs text-muted-foreground truncate">@{uusername}</p>
                    </div>
                    {if let Some(ref bio) = user.bio {
                        view! {
                            <p class="hidden sm:block text-xs text-muted-foreground truncate max-w-[120px]">
                                {bio.clone()}
                            </p>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }}
                </button>
            }.into_any()
        }).collect::<Vec<AnyView>>()}
    }
}

/// Renders the group-chat user selection list. Extracted into a component to avoid
/// capturing non-Copy values in the parent view! macro closure.
#[must_use]
#[component]
fn GroupUserList(
    users: Vec<User>,
    on_toggle: Arc<dyn Fn(String) + Send + Sync + 'static>,
) -> impl IntoView {
    let users = Arc::new(users);
    view! {
        {users.iter().map(|user| {
            let uid = user.id.clone();
            let uname = user.display_name.clone();
            let toggle = on_toggle.clone();
            view! {
                <button
                    class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors hover:bg-accent/50"
                    on:click=move |_| toggle(uid.clone())
                >
                    <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-muted text-sm font-medium text-foreground">
                        {get_initials(&uname)}
                    </div>
                    <div class="min-w-0 flex-1 text-left">
                        <p class="font-medium truncate">{uname}</p>
                        <p class="text-xs text-muted-foreground truncate">@{user.username.clone()}</p>
                    </div>
                    <div class="h-5 w-5 rounded-full border-2 border-muted-foreground/30"/>
                </button>
            }.into_any()
        }).collect::<Vec<AnyView>>()}
    }
}

#[must_use]
#[component]
pub fn NewChatDialog(
    #[prop(optional, into)] lang: Signal<Language>,
    #[prop(optional, into)] is_open: Signal<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    #[prop(optional, into)] users: Vec<User>,
    #[prop(optional)] on_select_user: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_create_group: Option<Box<dyn Fn(Vec<String>, String) + Send + Sync + 'static>>,
) -> impl IntoView {
    let search_query = RwSignal::new(String::new());
    let active_tab = RwSignal::new("direct".to_string());
    let selected_users = RwSignal::new(Vec::<String>::new());
    let group_name = RwSignal::new(String::new());

    let users_for_filter = users.clone();
    let users_for_signal = users.clone();
    let filtered_users = Arc::new(move || {
        let query = search_query.get().to_lowercase();
        if query.is_empty() {
            users_for_filter.clone()
        } else {
            users_for_filter.iter()
                .filter(|u| {
                    u.display_name.to_lowercase().contains(&query)
                        || u.username.to_lowercase().contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>()
        }
    });

    let toggle_user = Arc::new(move |user_id: String| {
        let mut selected = selected_users.get();
        if let Some(pos) = selected.iter().position(|id| *id == user_id) {
            selected.remove(pos);
        } else {
            selected.push(user_id);
        }
        selected_users.set(selected);
    });

    let handle_create_group = Arc::new(move |_| {
        let name = group_name.get().trim().to_string();
        let members = selected_users.get();
        if !name.is_empty() && !members.is_empty() {
            if let Some(ref f) = on_create_group {
                f(members, name);
            }
        }
    });

    let handle_select_direct = Arc::new(move |user_id: String| {
        if let Some(ref f) = on_select_user {
            f(user_id);
        }
    });

    let close_cb = on_close.unwrap_or_else(|| Box::new(|| {}));

    // Store non-Copy values in RwSignals so the view! closure captures Copy handles
    let users_signal = RwSignal::new(users_for_signal);
    let on_select_signal = RwSignal::new(handle_select_direct);
    let toggle_signal = RwSignal::new(toggle_user);
    let close_cb_signal = RwSignal::new(close_cb);
    let on_click_signal = RwSignal::new(handle_create_group);

    view! {
        <Dialog
            is_open=is_open
            on_close=Box::new(move || { close_cb_signal.with(|cb| cb()); })
        >
            <DialogHeader>
                <DialogTitle>{t(lang.get(), "chats.new")}</DialogTitle>
                <DialogDescription>
                    {move || if active_tab.get() == "direct" {
                        t(lang.get(), "chats.new.direct")
                    } else {
                        t(lang.get(), "chats.new.group")
                    }}
                </DialogDescription>
            </DialogHeader>

            // Tab switcher
            <Tabs>
                <TabsList>
                    <TabsTrigger
                        value="direct"
                        active=Signal::derive(move || active_tab.get() == "direct")
                        on_click=Box::new(move || active_tab.set("direct".to_string()))
                    >
                        <Icon name="user" class_name="h-4 w-4 mr-1"/>
                        {t(lang.get(), "chats.new.direct")}
                    </TabsTrigger>
                    <TabsTrigger
                        value="group"
                        active=Signal::derive(move || active_tab.get() == "group")
                        on_click=Box::new(move || active_tab.set("group".to_string()))
                    >
                        <Icon name="users" class_name="h-4 w-4 mr-1"/>
                        {t(lang.get(), "chats.new.group")}
                    </TabsTrigger>
                </TabsList>

                // Direct chat tab
                <TabsContent value="direct" active=Signal::derive(move || active_tab.get() == "direct")>
                    <div class="py-3">
                        <Input
                            id="search-users"
                            input_type="text"
                            placeholder={t(lang.get(), "chats.search").to_string()}
                            on_change=Box::new({
                                let q = search_query;
                                move |val: String| q.set(val)
                            })
                        />
                    </div>

                    <ScrollArea class="max-h-72">
                        <div class="space-y-0.5">
                            <DirectUserList users={users_signal.get()} on_select={on_select_signal.get()}/>
                        </div>
                    </ScrollArea>
                </TabsContent>

                // Group chat tab
                <TabsContent value="group" active=Signal::derive(move || active_tab.get() == "group")>
                    <div class="py-3 space-y-3">
                        <Input
                            id="group-name"
                            input_type="text"
                            placeholder="Group name"
                            on_change=Box::new({
                                let q = group_name;
                                move |val: String| q.set(val)
                            })
                        />
                        <Input
                            id="search-members"
                            input_type="text"
                            placeholder={t(lang.get(), "chats.search").to_string()}
                            on_change=Box::new({
                                let q = search_query;
                                move |val: String| q.set(val)
                            })
                        />
                    </div>

                    <Separator class="my-1"/>

                    // User list for selection
                    <ScrollArea class="max-h-52">
                        <div class="space-y-0.5">
                            <GroupUserList users={users_signal.get()} on_toggle={toggle_signal.get()}/>
                        </div>
                    </ScrollArea>

                    <DialogFooter>
                        <Button
                            variant=Signal::derive(move || ButtonVariant::Default)
                            disabled=Signal::derive(move || {
                                group_name.get().trim().is_empty() || selected_users.get().is_empty()
                            })
                            on_click=Box::new(move |ev| {
                                let hcg = on_click_signal.get();
                                hcg(ev)
                            })
                        >
                            {t(lang.get(), "chats.new.group")}
                        </Button>
                    </DialogFooter>
                </TabsContent>
            </Tabs>
        </Dialog>
    }
}

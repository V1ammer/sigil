//! Route definitions with auth guards for the messenger app.

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use crate::screens::*;
use crate::screens::chats::ChatsScreen;
use crate::screens::settings::SettingsScreen;
use crate::state::session::{use_session, UserRole};

#[must_use]
#[component]
pub fn AppRoutes() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <NotFound/> }>
                // Public — no auth required
                <Route path=path!("/") view=ConnectScreen/>
                <Route path=path!("/login") view=LoginScreen/>
                <Route path=path!("/login/token") view=LoginTokenScreen/>
                <Route path=path!("/login/qr") view=LoginQrScreen/>
                <Route path=path!("/register") view=RegisterScreen/>

                // Authenticated — redirects to "/" if not logged in
                <Route
                    path=path!("/chats")
                    view=move || view! { <RequireAuth><ChatsScreen/></RequireAuth> }
                />
                <Route
                    path=path!("/chats/:id")
                    view=move || view! { <RequireAuth><ChatsScreen/></RequireAuth> }
                />
                <Route
                    path=path!("/settings")
                    view=move || view! { <RequireAuth><SettingsScreen/></RequireAuth> }
                />
                <Route
                    path=path!("/settings/:section")
                    view=move || view! { <RequireAuth><SettingsScreen/></RequireAuth> }
                />
            </Routes>
        </Router>
    }
}

/// Wraps children and redirects to `/` if the user is not authenticated.
#[component]
pub fn RequireAuth(children: Children) -> impl IntoView {
    let session = use_session();

    Effect::new(move |_| {
        if !session.is_authenticated() {
            let navigate = leptos_router::hooks::use_navigate();
            navigate("/", Default::default());
        }
    });

    view! { <>{children()}</> }
}

/// Wraps children and redirects to `/` if the user is not an admin.
#[component]
pub fn RequireAdmin(children: Children) -> impl IntoView {
    let session = use_session();

    Effect::new(move |_| {
        if !session.is_admin() {
            let navigate = leptos_router::hooks::use_navigate();
            navigate("/", Default::default());
        }
    });

    view! { <>{children()}</> }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="flex min-h-screen items-center justify-center bg-background">
            <p class="text-muted-foreground">"404 — Not Found"</p>
        </div>
    }
}

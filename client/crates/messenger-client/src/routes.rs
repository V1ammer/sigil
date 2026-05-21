//! Route definitions for the messenger app.
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use crate::screens::*;
use crate::screens::chats::ChatsScreen;
use crate::screens::settings::SettingsScreen;
use crate::i18n::Language;
use crate::theme::Theme;

#[must_use]
#[component]
pub fn AppRoutes() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <NotFound/> }>
                <Route path=path!("/") view=ConnectScreen/>
                <Route path=path!("/login") view=LoginScreen/>
                <Route path=path!("/login/token") view=LoginTokenScreen/>
                <Route path=path!("/login/qr") view=LoginQrScreen/>
                <Route path=path!("/register") view=RegisterScreen/>
                <Route path=path!("/chats") view=ChatsScreen/>
                <Route path=path!("/chats/:id") view=ChatsScreen/>
                <Route path=path!("/settings") view=SettingsScreen/>
                <Route path=path!("/settings/:section") view=SettingsScreen/>
            </Routes>
        </Router>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="flex min-h-screen items-center justify-center bg-background">
            <p class="text-muted-foreground">"404 — Not Found"</p>
        </div>
    }
}

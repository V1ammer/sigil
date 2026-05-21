use leptos::prelude::*;
use crate::components::avatar::Avatar;
use crate::components::button::{Button, ButtonVariant};
use crate::components::input::Input;
use crate::components::textarea::Textarea;
use crate::components::separator::Separator;
use crate::components::label::Label;
use crate::i18n::{Language, t};
use crate::mock::{mock_current_user, MOCK_SAFETY_NUMBER};

/// Account settings — display name, username, bio, avatar.
#[must_use]
#[component]
pub fn AccountSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let user = mock_current_user();
    let display_name = RwSignal::new(user.display_name.clone());
    let bio = RwSignal::new(user.bio.clone().unwrap_or_default());
    let user_display_name = user.display_name.clone();
    let user_display_name2 = user_display_name.clone();
    let user_username = user.username.clone();

    let on_bio_change = Box::new(move |v: String| bio.set(v));

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.account.title")}</h3>
                <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.account.description")}</p>
            </div>

            <Separator />

            // Avatar section
            <div class="flex items-center gap-4">
                <Avatar
                    src=user.avatar_url.clone()
                    alt=user.display_name.clone()
                    class="h-16 w-16"
                >
                    <span class="text-lg font-semibold text-foreground">
                        {crate::components::avatar::get_initials(&user_display_name2)}
                    </span>
                </Avatar>
                <div class="space-y-1">
                    <p class="text-sm font-medium text-foreground">{user_display_name.clone()}</p>
                    <p class="text-xs text-muted-foreground">@{user_username}</p>
                    <Button
                        variant=Signal::derive(move || ButtonVariant::Outline)
                        size=Signal::derive(move || crate::components::button::ButtonSize::Sm)
                    >
                        {t(lang.get(), "settings.account.changeAvatar")}
                    </Button>
                </div>
            </div>

            <Separator />

            // Display name
            <div class="space-y-2">
                <Label class="text-foreground">{t(lang.get(), "settings.account.displayName")}</Label>
                <Input
                    value=display_name.get()
                    on_change=Box::new(move |v| display_name.set(v))
                />
            </div>

            // Username (readonly)
            <div class="space-y-2">
                <Label class="text-foreground">{t(lang.get(), "settings.account.username")}</Label>
                <Input
                    value=user.username.clone()
                    disabled=Signal::derive(move || true)
                />
                <p class="text-xs text-muted-foreground">{t(lang.get(), "settings.account.usernameHint")}</p>
            </div>

            // Bio
            <div class="space-y-2">
                <Label class="text-foreground">{t(lang.get(), "settings.account.bio")}</Label>
                <Textarea
                    value=bio.get()
                    on_change=on_bio_change
                    placeholder=t(lang.get(), "settings.account.bioPlaceholder")
                />
            </div>

            <Separator />

            // Safety number
            <div class="space-y-2">
                <Label class="text-foreground">{t(lang.get(), "settings.account.safetyNumber")}</Label>
                <div class="rounded-md bg-muted p-3 font-mono text-xs text-foreground break-all select-all">
                    {MOCK_SAFETY_NUMBER}
                </div>
                <p class="text-xs text-muted-foreground">{t(lang.get(), "settings.account.safetyHint")}</p>
            </div>

            <Separator />

            // Privacy note
            <div class="rounded-md border border-muted bg-muted/50 p-3">
                <p class="text-xs text-muted-foreground">{t(lang.get(), "settings.account.privacyNote")}</p>
            </div>

            // Save button
            <div class="flex justify-end">
                <Button
                    variant=Signal::derive(move || ButtonVariant::Default)
                    on_click=Box::new(move |_| {
                        // TODO: save account settings
                    })
                >
                    {t(lang.get(), "settings.account.save")}
                </Button>
            </div>
        </div>
    }
}

use leptos::prelude::*;
use crate::components::separator::Separator;
use crate::components::card::{Card, CardContent};
use crate::i18n::{Language, t};
use crate::mock::mock_server_info;

/// About settings — version, server address, license.
#[must_use]
#[component]
pub fn AboutSettings() -> impl IntoView {
    let lang = use_context::<RwSignal<Language>>().unwrap_or_default();
    let (name, address, version) = mock_server_info();

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t(lang.get(), "settings.about.title")}</h3>
                <p class="text-sm text-muted-foreground">{t(lang.get(), "settings.about.description")}</p>
            </div>

            <Separator />

            <Card class="w-full">
                <CardContent class="space-y-4">
                    <div class="flex justify-between py-1">
                        <span class="text-sm text-muted-foreground">{t(lang.get(), "settings.about.serverName")}</span>
                        <span class="text-sm font-medium text-foreground">{name}</span>
                    </div>
                    <Separator />
                    <div class="flex justify-between py-1">
                        <span class="text-sm text-muted-foreground">{t(lang.get(), "settings.about.serverAddress")}</span>
                        <span class="text-sm text-foreground font-mono">{address}</span>
                    </div>
                    <Separator />
                    <div class="flex justify-between py-1">
                        <span class="text-sm text-muted-foreground">{t(lang.get(), "settings.about.version")}</span>
                        <span class="text-sm font-medium text-foreground">{version}</span>
                    </div>
                    <Separator />
                    <div class="flex justify-between py-1">
                        <span class="text-sm text-muted-foreground">{t(lang.get(), "settings.about.license")}</span>
                        <span class="text-sm text-foreground">"MIT"</span>
                    </div>
                    <Separator />
                    <div class="py-1">
                        <p class="text-xs text-muted-foreground text-center">
                            {t(lang.get(), "settings.about.copyright")}
                        </p>
                    </div>
                </CardContent>
            </Card>
        </div>
    }
}

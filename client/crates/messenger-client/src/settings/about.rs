use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::components::separator::Separator;
use crate::components::card::{Card, CardContent};
use crate::components::label::Label;
use crate::state::session::{build_api_client, load_server_url};
use crate::t;

/// External resource URLs.
const DOCS_URL: &str = "https://docs.example.com/messenger";
const SOURCE_URL: &str = "https://github.com/V1ammer/sigil";
const BUG_TRACKER_URL: &str = "https://github.com/V1ammer/sigil/issues";

/// About settings — version, server address, license, MLS info.
#[must_use]
#[component]
pub fn AboutSettings() -> impl IntoView {
    let server_pubkey_hex = RwSignal::new(String::new());
    let server_mls_version = RwSignal::new(String::new());

    spawn_local(async move {
        if let Some(api) = build_api_client() {
            if let Ok(info) = api.server_info().await {
                let hex_str: String = info
                    .server_identity_public_key
                    .iter()
                    .take(16)
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .chunks(4)
                    .map(|c| c.join(""))
                    .collect::<Vec<_>>()
                    .join(" ");
                server_pubkey_hex.set(hex_str);
                server_mls_version.set(format!("0x{:04x}", info.mls_ciphersuite));
            }
        }
    });

    let server_url = load_server_url().unwrap_or_default();

    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-lg font-medium text-foreground">{t!("settings.about.title")}</h3>
                <p class="text-sm text-muted-foreground">{t!("settings.about.description")}</p>
            </div>

            <Separator />

            <Card class="w-full">
                <CardContent class="space-y-4">
                    // App version
                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.version")}</Label>
                        <span class="text-sm font-medium text-foreground">{env!("CARGO_PKG_VERSION")}</span>
                    </div>

                    <Separator />

                    // MLS protocol version (from server if loaded, fallback to RFC 9420)
                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.mlsVersion")}</Label>
                        <span class="text-sm font-medium text-foreground">
                            {move || {
                                let mls = server_mls_version.get();
                                if mls.is_empty() {
                                    "MLS 0x0001 (RFC 9420)".to_string()
                                } else {
                                    format!("MLS {} (RFC 9420)", mls)
                                }
                            }}
                        </span>
                    </div>

                    <Separator />

                    // Server URL (from local storage)
                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.serverAddress")}</Label>
                        <span class="text-sm text-foreground font-mono">{server_url}</span>
                    </div>

                    <Separator />

                    // Server public key (hex, first 16 bytes)
                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.serverPubkey")}</Label>
                        <span class="text-sm text-foreground font-mono break-all">
                            {move || {
                                let hex = server_pubkey_hex.get();
                                if hex.is_empty() {
                                    t!("loading")
                                } else {
                                    hex
                                }
                            }}
                        </span>
                    </div>

                    <Separator />

                    // License
                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.licenseDesc")}</Label>
                        <span class="text-sm font-medium text-foreground">"AGPL-3.0"</span>
                    </div>

                    <Separator />

                    // External links
                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.docs")}</Label>
                        <a
                            href=DOCS_URL
                            target="_blank"
                            class="text-sm text-primary hover:underline"
                        >
                            {t!("settings.about.docs")}
                        </a>
                    </div>

                    <Separator />

                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.source")}</Label>
                        <a
                            href=SOURCE_URL
                            target="_blank"
                            class="text-sm text-primary hover:underline"
                        >
                            {t!("settings.about.source")}
                        </a>
                    </div>

                    <Separator />

                    <div class="flex justify-between py-1">
                        <Label class="text-muted-foreground">{t!("settings.about.bugTracker")}</Label>
                        <a
                            href=BUG_TRACKER_URL
                            target="_blank"
                            class="text-sm text-primary hover:underline"
                        >
                            {t!("settings.about.bugTracker")}
                        </a>
                    </div>

                    <Separator />

                    // Copyright
                    <div class="py-1">
                        <p class="text-xs text-muted-foreground text-center">
                            {t!("settings.about.copyright")}
                        </p>
                    </div>
                </CardContent>
            </Card>
        </div>
    }
}

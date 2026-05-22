//! Toast notification overlay — rendered at the top of the viewport.

use leptos::prelude::*;
use crate::state::notifications::{NotificationsState, ToastKind};

/// Renders the current toast queue as a fixed overlay in the top-right corner.
///
/// Insert this once in `app.rs` — it reads `NotificationsState` from context.
#[component]
pub fn ToastContainer() -> impl IntoView {
    let toasts = match use_context::<NotificationsState>() {
        Some(n) => n.toasts,
        None => return view! {}.into_any(),
    };

    view! {
        <div class="fixed top-4 right-4 z-50 flex flex-col gap-2 pointer-events-none">
            {move || {
                let list = toasts.get();
                list.into_iter()
                    .map(|t| {
                        let bg = match t.kind {
                            ToastKind::Info => "bg-blue-600",
                            ToastKind::Success => "bg-green-600",
                            ToastKind::Warning => "bg-yellow-600",
                            ToastKind::Error => "bg-red-600",
                        };
                        view! {
                            <div
                                class=format!(
                                    "{} text-white px-4 py-3 rounded-lg shadow-lg \
                                     max-w-sm pointer-events-auto animate-in fade-in slide-in-from-right",
                                    bg,
                                )
                                role="alert"
                            >
                                <p class="text-sm">{t.message}</p>
                            </div>
                        }
                    })
                    .collect::<Vec<_>>()
            }}
        </div>
    }
    .into_any()
}

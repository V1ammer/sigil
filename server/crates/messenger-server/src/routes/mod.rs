use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;

use crate::auth::middleware::require_auth;
use crate::state::AppState;

pub mod admin;
pub mod invite;
pub mod server_info;

/// Строит роутер со всеми маршрутами.
///
/// ## Публичные endpoints (без auth)
/// - `GET /health` — проверка здоровья.
/// - `GET /v1/server/info` — информация о сервере.
/// - `POST /v1/invite/redeem` — пока 501 (S07).
///
/// ## Admin endpoints (auth + admin role)
/// - `POST   /v1/admin/invites` — создать инвайт.
/// - `GET    /v1/admin/invites` — список активных.
/// - `DELETE /v1/admin/invites/:id` — отозвать инвайт.
///
/// ## Защищённые endpoints (требуют `X-Auth-Signature`)
/// - `GET /v1/users/me/test` — smoke-test аутентификации (временно).
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health))
        .route("/v1/server/info", get(server_info::info))
        .route("/v1/invite/redeem", post(invite::redeem_stub));

    let admin = Router::new()
        .route("/v1/admin/invites", post(admin::create_invite))
        .route("/v1/admin/invites", get(admin::list_invites))
        .route("/v1/admin/invites/:id", delete(admin::revoke_invite))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    let authed = Router::new()
        .route("/v1/users/me/test", get(test_authed))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    Router::new()
        .merge(public)
        .merge(admin)
        .merge(authed)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

/// Smoke-тест аутентификации. Возвращает `user_id` и `device_id`.
/// Временно — будет удалён в S07.
async fn test_authed(
    crate::auth::middleware::CurrentAuth(ctx): crate::auth::middleware::CurrentAuth,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "user_id": ctx.user.id,
        "device_id": ctx.device.id,
    }))
}

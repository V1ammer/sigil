use axum::middleware;
use axum::routing::get;
use axum::Router;

use crate::auth::middleware::{require_auth, CurrentAuth};
use crate::state::AppState;

pub mod server_info;

/// Строит роутер со всеми маршрутами.
///
/// Публичные endpoints:
/// - `GET /health` — проверка здоровья.
/// - `GET /v1/server/info` — информация о сервере (msgpack).
///
/// Защищённые endpoints (требуют `X-Auth-Signature`):
/// - `GET /v1/users/me/test` — smoke-test аутентификации (временно, удалить в S07).
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health))
        .route("/v1/server/info", get(server_info::info));

    let authed = Router::new()
        .route("/v1/users/me/test", get(test_authed))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    Router::new()
        .merge(public)
        .merge(authed)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

/// Smoke-тест аутентификации. Возвращает `user_id` и `device_id`.
/// Временно — будет удалён в S07.
async fn test_authed(CurrentAuth(ctx): CurrentAuth) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "user_id": ctx.user.id,
        "device_id": ctx.device.id,
    }))
}

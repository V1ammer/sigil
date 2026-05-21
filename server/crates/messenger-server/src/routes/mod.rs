use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::auth::middleware::require_auth;
use crate::state::AppState;

pub mod admin;
pub mod devices;
pub mod invite;
pub mod provisioning;
pub mod server_info;
pub mod users;

/// Строит роутер со всеми маршрутами.
///
/// ## Публичные endpoints (без auth)
/// - `GET /health` — проверка здоровья.
/// - `GET /v1/server/info` — информация о сервере.
/// - `POST /v1/invite/redeem` — регистрация пользователя/устройства.
/// - `POST /v1/provisioning/requests` — создание provisioning запроса.
/// - `GET /v1/provisioning/requests/:id/bootstrap` — polling (специальная auth).
///
/// ## Admin endpoints (auth + admin role)
/// - `POST   /v1/admin/invites` — создать инвайт.
/// - `GET    /v1/admin/invites` — список активных.
/// - `DELETE /v1/admin/invites/:id` — отозвать инвайт.
/// - `POST   /v1/admin/users/:id/suspend` — заморозить пользователя.
/// - `POST   /v1/admin/users/:id/unsuspend` — разморозить пользователя.
/// - `GET    /v1/admin/users` — пагинированный список пользователей.
///
/// ## Защищённые endpoints (требуют `X-Auth-Signature`)
/// - `GET   /v1/users/lookup` — поиск по `blind_index`.
/// - `GET   /v1/users/:id/identity` — identity credential.
/// - `PATCH /v1/users/me/username` — смена username.
/// - `GET   /v1/devices/me` — список своих устройств.
/// - `POST  /v1/devices/me/:device_id/revoke` — отозвать устройство.
/// - `GET   /v1/users/:user_id/devices` — список активных устройств.
/// - `GET   /v1/provisioning/requests/:id` — детали запроса.
/// - `POST  /v1/provisioning/requests/:id/approve` — одобрить запрос.
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health))
        .route("/v1/server/info", get(server_info::info))
        .route("/v1/invite/redeem", post(invite::redeem))
        .route(
            "/v1/provisioning/requests",
            post(provisioning::create_request),
        )
        .route(
            "/v1/provisioning/requests/:id/bootstrap",
            get(provisioning::get_provisioning_bootstrap),
        );

    let admin = Router::new()
        .route("/v1/admin/invites", post(admin::create_invite))
        .route("/v1/admin/invites", get(admin::list_invites))
        .route("/v1/admin/invites/:id", delete(admin::revoke_invite))
        .route("/v1/admin/users/:id/suspend", post(admin::suspend_user))
        .route(
            "/v1/admin/users/:id/unsuspend",
            post(admin::unsuspend_user),
        )
        .route("/v1/admin/users", get(admin::list_users))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    let authed = Router::new()
        .route("/v1/users/lookup", get(users::lookup))
        .route("/v1/users/:id/identity", get(users::identity))
        .route("/v1/users/me/username", patch(users::change_username))
        .route("/v1/devices/me", get(devices::list_my_devices))
        .route(
            "/v1/devices/me/:device_id/revoke",
            post(devices::revoke_device),
        )
        .route(
            "/v1/users/:user_id/devices",
            get(devices::list_user_devices),
        )
        .route(
            "/v1/provisioning/requests/:id",
            get(provisioning::get_request),
        )
        .route(
            "/v1/provisioning/requests/:id/approve",
            post(provisioning::approve_request),
        )
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

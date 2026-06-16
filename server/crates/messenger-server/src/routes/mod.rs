use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::auth::middleware::require_auth;
use crate::state::AppState;

pub mod admin;
pub mod attachments;
pub mod dev;
pub mod keypackages;
pub mod devices;
pub mod invite;
pub mod mls;
pub mod provisioning;
pub mod server_info;
pub mod users;
pub mod ws;

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
/// - `POST   /v1/admin/users/:id/role` — сменить роль пользователя.
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
/// - `POST  /v1/keypackages` — публикация `KeyPackages`.
/// - `GET   /v1/keypackages/me/count` — статистика пула.
/// - `POST  /v1/users/:user_id/devices/:device_id/keypackage/claim` — атомарный claim.
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health))
        .route("/v1/server/info", get(server_info::info))
        .route("/v1/invite/redeem", post(invite::redeem))
        .route("/v1/dev/create-token", get(dev::create_dev_token))
        .route(
            "/v1/dev/force-bootstrap/:id",
            get(dev::force_consume_provisioning),
        )
        .route(
            "/v1/provisioning/requests",
            post(provisioning::create_request),
        )
        .route(
            "/v1/provisioning/requests/:id/bootstrap",
            get(provisioning::get_provisioning_bootstrap),
        )
        .route("/v1/users/lookup/username", get(users::username_lookup))
        .route("/v1/ws", get(ws::ws_handler));

    let admin = Router::new()
        .route("/v1/admin/invites", post(admin::create_invite))
        .route("/v1/admin/invites", get(admin::list_invites))
        .route("/v1/admin/invites/:id", delete(admin::revoke_invite))
        .route("/v1/admin/users/:id/suspend", post(admin::suspend_user))
        .route(
            "/v1/admin/users/:id/unsuspend",
            post(admin::unsuspend_user),
        )
        .route("/v1/admin/users/:id/role", post(admin::set_role))
        .route("/v1/admin/users/:id", delete(admin::delete_user))
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
        .route("/v1/keypackages", post(keypackages::publish_keypackages))
        .route(
            "/v1/keypackages/me",
            delete(keypackages::delete_my_keypackages),
        )
        .route(
            "/v1/keypackages/me/count",
            get(keypackages::get_pool_stats),
        )
        .route(
            "/v1/users/:user_id/devices/:device_id/keypackage/claim",
            post(keypackages::claim_keypackage),
        )
        // S10 — MLS Messaging
        .route("/v1/groups", post(mls::create_group))
        .route("/v1/groups/create-direct", post(mls::create_direct_chat))
        .route("/v1/groups/me", get(mls::list_my_groups))
        .route("/v1/groups/:id", delete(mls::delete_group))
        .route("/v1/groups/:id/members", get(mls::get_group_members))
        .route("/v1/groups/:id/owner", post(mls::transfer_owner))
        .route("/v1/groups/:id/commit", post(mls::post_commit))
        .route("/v1/groups/:id/messages", get(mls::pull_messages))
        .route("/v1/groups/:id/messages", post(mls::post_message))
        .route("/v1/messages/:id/state", post(mls::update_message_state))
        .route("/v1/messages/:id/delivery", get(mls::get_delivery_status))
        .route("/v1/welcomes/me", get(mls::list_welcomes))
        .route("/v1/welcomes/:id/ack", post(mls::ack_welcome))
        .route("/v1/messages/:id/reactions", post(mls::add_reaction))
        .route(
            "/v1/messages/:id/reactions/:blind_index_hex",
            delete(mls::remove_reaction),
        )
        // S11 — Attachments
        // Raise the body limit for uploads above axum's 2 MiB default —
        // otherwise large attachments (e.g. a video) are rejected with 413
        // before the handler runs. The handler still enforces the real
        // `max_attachment_bytes` cap and returns a proper 400 for oversize.
        .route(
            "/v1/attachments",
            post(attachments::upload_attachment).layer(DefaultBodyLimit::max(
                attachment_body_limit(&state),
            )),
        )
        // S11b — Chunked/resumable upload for large attachments (e.g. video).
        // init declares the total; each part is a small signed request the
        // server appends to disk (no whole-file RAM buffering), then finalize
        // binds it. The part route lifts axum's 2 MiB default to fit one part.
        .route("/v1/attachments/init", post(attachments::init_attachment))
        .route(
            "/v1/attachments/:id/parts",
            post(attachments::upload_part)
                .layer(DefaultBodyLimit::max(UPLOAD_PART_BODY_LIMIT)),
        )
        .route("/v1/attachments/:id/status", get(attachments::attachment_status))
        .route("/v1/attachments/:id/finalize", post(attachments::finalize_attachment))
        .route("/v1/attachments/:id", get(attachments::download_attachment))
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

/// Body limit for attachment uploads: the configured cap plus a small slack
/// for AES-GCM overhead, so the handler's own size check (not axum) rejects
/// genuinely oversize uploads.
fn attachment_body_limit(state: &AppState) -> usize {
    let cap = state.config.max_attachment_bytes.saturating_add(1024 * 1024);
    usize::try_from(cap).unwrap_or(usize::MAX)
}

/// Max body for a single chunked-upload part: the client's part size (4 MiB)
/// plus slack. The global `RequestBodyLimitLayer` (and the 30 s timeout) easily
/// accommodate one part, so big files no longer hit either limit.
const UPLOAD_PART_BODY_LIMIT: usize = 8 * 1024 * 1024;

async fn health() -> &'static str {
    "ok"
}

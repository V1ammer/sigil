//! Admin endpoints для управления инвайт-токенами и пользователями.
//!
//! Все эндпоинты требуют аутентификации и роли `admin`.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::body::Bytes;
use base64::Engine;
use rand::RngCore;
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{CurrentAuth, RequireAdmin};
use crate::error::{decode_body, typed_response, AppError};
use crate::services::invite::now_secs;
use crate::state::AppState;
use messenger_entity::invitation_tokens;

/// Максимальный TTL для invite token: 365 дней в секундах.
const MAX_TTL_SECONDS: i64 = 365 * 24 * 3600;

// ─── Create Invite ───

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    pub role_to_grant: String, // "admin" | "user"
    pub max_uses: i32,         // ≥ 1
    pub ttl_seconds: i64,      // ≥ 60, ≤ 365*24*3600
}

#[derive(Serialize)]
pub struct CreateInviteResponse {
    pub id: Uuid,
    /// Токен (32 байта raw). Клиент может base64'ить для UI.
    #[serde(with = "serde_bytes")]
    pub token: Vec<u8>,
    /// Токен в base64url-no-pad для прямого копирования.
    pub token_display: String,
    pub expires_at: i64,
}

/// `POST /v1/admin/invites` — создать новый инвайт-токен.
///
/// # Errors
///
/// - `400 BadRequest` — невалидные параметры (`role_to_grant`, `max_uses`, `ttl_seconds`).
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — пользователь не admin.
/// - `500` — внутренняя ошибка.
pub async fn create_invite(
    CurrentAuth(ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: CreateInviteRequest = decode_body(&headers, &body)?;

    // Валидация
    if req.role_to_grant != "admin" && req.role_to_grant != "user" {
        return Err(AppError::BadRequest(
            "role_to_grant must be 'admin' or 'user'".into(),
        ));
    }
    if req.max_uses < 1 {
        return Err(AppError::BadRequest("max_uses must be >= 1".into()));
    }
    if req.ttl_seconds < 60 {
        return Err(AppError::BadRequest("ttl_seconds must be >= 60".into()));
    }
    if req.ttl_seconds > MAX_TTL_SECONDS {
        return Err(AppError::BadRequest(format!(
            "ttl_seconds must be <= {MAX_TTL_SECONDS}"
        )));
    }

    // Генерируем 32 случайных байта
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);

    // Хэшируем от base64url-no-pad представления (см. S06 spec).
    // Это позволяет клиенту послать base64 строку как «токен».
    let token_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let token_hash = blake3::hash(token_b64.as_bytes()).as_bytes().to_vec();

    let now = now_secs();
    let expires_at = now + req.ttl_seconds;
    let token_id = Uuid::now_v7();

    invitation_tokens::ActiveModel {
        id: sea_orm::Set(token_id),
        token_hash: sea_orm::Set(token_hash),
        created_by_user_id: sea_orm::Set(Some(ctx.user.id)),
        role_to_grant: sea_orm::Set(req.role_to_grant),
        max_uses: sea_orm::Set(req.max_uses),
        uses_count: sea_orm::Set(0),
        expires_at: sea_orm::Set(expires_at),
        revoked_at: sea_orm::Set(None),
        created_at: sea_orm::Set(now),
    }
    .insert(&state.db)
    .await?;

    let resp = CreateInviteResponse {
        id: token_id,
        token: token_bytes.to_vec(),
        token_display: token_b64,
        expires_at,
    };

    Ok(typed_response(&headers, StatusCode::CREATED, &resp))
}

// ─── List Invites ───

#[derive(Serialize)]
pub struct InviteSummary {
    pub id: Uuid,
    pub role_to_grant: String,
    pub max_uses: i32,
    pub uses_count: i32,
    pub expires_at: i64,
    pub created_at: i64,
    pub created_by_user_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct ListInvitesResponse {
    pub invites: Vec<InviteSummary>,
}

/// `GET /v1/admin/invites` — список активных инвайт-токенов.
///
/// Активным считается токен у которого:
/// - `revoked_at IS NULL`
/// - `expires_at > now`
/// - `uses_count < max_uses`
///
/// # Errors
///
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — пользователь не admin.
/// - `500` — внутренняя ошибка.
pub async fn list_invites(
    CurrentAuth(_ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    use messenger_entity::invitation_tokens::{self, Entity as InvitationTokens};
    use sea_orm::sea_query::Expr;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let now = now_secs();

    let active = InvitationTokens::find()
        .filter(invitation_tokens::Column::RevokedAt.is_null())
        .filter(invitation_tokens::Column::ExpiresAt.gt(now))
        .filter(Expr::col(invitation_tokens::Column::UsesCount).lt(Expr::col(
            invitation_tokens::Column::MaxUses,
        )))
        .all(&state.db)
        .await?;

    let invites = active
        .into_iter()
        .map(|m| InviteSummary {
            id: m.id,
            role_to_grant: m.role_to_grant,
            max_uses: m.max_uses,
            uses_count: m.uses_count,
            expires_at: m.expires_at,
            created_at: m.created_at,
            created_by_user_id: m.created_by_user_id,
        })
        .collect();

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ListInvitesResponse { invites },
    ))
}

// ─── Revoke Invite ───

/// `DELETE /v1/admin/invites/:id` — отзыв инвайт-токена.
///
/// Помечает `revoked_at = now`. Идемпотентен: если уже отозван → 200 OK.
///
/// # Errors
///
/// - `404 Not Found` — токен не существует.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — пользователь не admin.
/// - `500` — внутренняя ошибка.
pub async fn revoke_invite(
    CurrentAuth(_ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    use messenger_entity::invitation_tokens::{self, Entity as InvitationTokens};
    use sea_orm::{EntityTrait, Set};

    let row = InvitationTokens::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if row.revoked_at.is_some() {
        // Уже отозван — идемпотентно
        return Ok(typed_response(&headers, StatusCode::OK, &serde_json::json!({})));
    }

    let mut active: invitation_tokens::ActiveModel = row.into();
    active.revoked_at = Set(Some(now_secs()));
    active.update(&state.db).await?;

    Ok(typed_response(&headers, StatusCode::OK, &serde_json::json!({})))
}

// ──────────────────────────────────────────────
// Admin User Management
// ──────────────────────────────────────────────

/// `POST /v1/admin/users/:id/suspend` — заморозить пользователя.
///
/// # Errors
///
/// - `404 Not Found` — пользователь не существует.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — пользователь не admin.
/// - `500` — внутренняя ошибка.
pub async fn suspend_user(
    CurrentAuth(_ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Response, AppError> {
    use messenger_entity::users::{self, Entity as Users};

    let user = Users::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut active: users::ActiveModel = user.into();
    active.status = sea_orm::Set("suspended".to_string());
    active.update(&state.db).await?;

    Ok(typed_response::<()>(&headers, StatusCode::NO_CONTENT, &()))
}

/// `POST /v1/admin/users/:id/unsuspend` — разморозить пользователя.
///
/// # Errors
///
/// - `404 Not Found` — пользователь не существует.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — пользователь не admin.
/// - `500` — внутренняя ошибка.
pub async fn unsuspend_user(
    CurrentAuth(_ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Response, AppError> {
    use messenger_entity::users::{self, Entity as Users};

    let user = Users::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut active: users::ActiveModel = user.into();
    active.status = sea_orm::Set("active".to_string());
    active.update(&state.db).await?;

    Ok(typed_response::<()>(&headers, StatusCode::NO_CONTENT, &()))
}

/// Тело запроса на смену роли пользователя.
#[derive(Deserialize)]
pub struct SetRoleRequest {
    pub role: String, // "admin" | "user"
}

/// `POST /v1/admin/users/:id/role` — сменить роль пользователя.
///
/// # Errors
///
/// - `400 BadRequest` — невалидная роль или попытка снять админку с себя.
/// - `404 Not Found` — пользователь не существует.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — вызывающий не admin.
/// - `500` — внутренняя ошибка.
pub async fn set_role(
    CurrentAuth(ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    use messenger_entity::users::{self, Entity as Users};

    let req: SetRoleRequest = decode_body(&headers, &body)?;
    if req.role != "admin" && req.role != "user" {
        return Err(AppError::BadRequest("role must be 'admin' or 'user'".into()));
    }
    // Guard against an admin demoting themselves (self-lockout).
    if user_id == ctx.user.id && req.role != "admin" {
        return Err(AppError::BadRequest("cannot remove your own admin role".into()));
    }

    let user = Users::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut active: users::ActiveModel = user.into();
    active.role = sea_orm::Set(req.role);
    active.update(&state.db).await?;

    Ok(typed_response::<()>(&headers, StatusCode::NO_CONTENT, &()))
}

/// Query параметры для пагинированного списка пользователей.
#[derive(Deserialize)]
pub struct ListUsersQuery {
    #[serde(default = "default_offset")]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_offset() -> u64 {
    0
}
fn default_limit() -> u64 {
    50
}

/// Информация о пользователе для админ-списка.
#[derive(Serialize)]
pub struct AdminUserInfo {
    pub id: Uuid,
    pub role: String,
    pub status: String,
    pub created_at: i64,
    /// Активные (неотозванные) устройства — клиентская схема требует это поле.
    pub devices_count: i32,
}

/// Ответ на `GET /v1/admin/users`.
#[derive(Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<AdminUserInfo>,
    pub total: u64,
}

/// `GET /v1/admin/users` — пагинированный список пользователей (без `blind_index`).
///
/// # Errors
///
/// - `401 Unauthorized` — отсутствует auth context.
/// - `403 Forbidden` — пользователь не admin.
/// - `500` — внутренняя ошибка.
pub async fn list_users(
    CurrentAuth(_ctx): CurrentAuth,
    RequireAdmin(_): RequireAdmin,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListUsersQuery>,
) -> Result<Response, AppError> {
    use messenger_entity::users::{self, Entity as Users};


    // Общее количество
    let total = Users::find().count(&state.db).await?;

    // Пагинированный список (без blind_index)
    let users = Users::find()
        .order_by_asc(users::Column::CreatedAt)
        .offset(query.offset)
        .limit(query.limit)
        .all(&state.db)
        .await?;

    // Активные устройства всех пользователей страницы одним запросом.
    use sea_orm::{ColumnTrait, QueryFilter};
    let user_ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
    let mut device_counts: std::collections::HashMap<Uuid, i32> =
        std::collections::HashMap::new();
    if !user_ids.is_empty() {
        let devices = messenger_entity::devices::Entity::find()
            .filter(messenger_entity::devices::Column::UserId.is_in(user_ids))
            .filter(messenger_entity::devices::Column::RevokedAt.is_null())
            .all(&state.db)
            .await?;
        for d in devices {
            *device_counts.entry(d.user_id).or_default() += 1;
        }
    }

    let info: Vec<AdminUserInfo> = users
        .into_iter()
        .map(|u| AdminUserInfo {
            devices_count: device_counts.get(&u.id).copied().unwrap_or(0),
            id: u.id,
            role: u.role,
            status: u.status,
            created_at: u.created_at,
        })
        .collect();

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ListUsersResponse {
            users: info,
            total,
        },
    ))
}

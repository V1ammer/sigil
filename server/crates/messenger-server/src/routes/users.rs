//! Endpoints для работы с пользователями.
//!
//! - `GET /v1/users/lookup?blind_index=<hex>` — поиск по `blind_index`.
//! - `GET /v1/users/:id/identity` — identity credential пользователя.
//! - `PATCH /v1/users/me/username` — смена username (пересчёт `blind_index`).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::body::Bytes;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::auth::middleware::CurrentAuth;
use crate::error::{decode_body, typed_response, AppError};
use crate::state::AppState;
use messenger_entity::users::{self, Entity as Users};
use messenger_entity::user_identity_credentials::{self, Entity as UserIdentityCredentials};

// ──────────────────────────────────────────────
// GET /v1/users/lookup?blind_index=<hex>
// ──────────────────────────────────────────────

/// Query параметры для lookup.
#[derive(Debug, serde::Deserialize)]
pub struct LookupQuery {
    pub blind_index: String, // hex
}

/// Ответ lookup и identity.
#[derive(Debug, serde::Serialize)]
pub struct LookupResponse {
    pub user_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub identity_credential: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub signature_public_key: Vec<u8>,
}

/// `GET /v1/users/lookup?blind_index=<hex>`
///
/// # Errors
///
/// - `400 BadRequest` — невалидный hex в `blind_index`.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `404 Not Found` — пользователь не найден.
/// - `500` — внутренняя ошибка.
pub async fn lookup(
    CurrentAuth(_ctx): CurrentAuth,
    State(state): State<AppState>,
    Query(query): Query<LookupQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let blind_index = hex::decode(&query.blind_index)
        .map_err(|_| AppError::BadRequest("invalid hex in blind_index".into()))?;

    let user = Users::find()
        .filter(users::Column::UsernameBlindIndex.eq(blind_index))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let identity = UserIdentityCredentials::find()
        .filter(user_identity_credentials::Column::UserId.eq(user.id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &LookupResponse {
            user_id: user.id,
            identity_credential: identity.credential,
            signature_public_key: identity.signature_public_key,
        },
    ))
}

// ──────────────────────────────────────────────
// GET /v1/users/:id/identity
// ──────────────────────────────────────────────

/// `GET /v1/users/:id/identity`
///
/// # Errors
///
/// - `401 Unauthorized` — отсутствует auth context.
/// - `404 Not Found` — пользователь не найден.
/// - `500` — внутренняя ошибка.
pub async fn identity(
    CurrentAuth(_ctx): CurrentAuth,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let identity = UserIdentityCredentials::find()
        .filter(user_identity_credentials::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Получить user_id из identity (он же PK)
    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &LookupResponse {
            user_id: identity.user_id,
            identity_credential: identity.credential,
            signature_public_key: identity.signature_public_key,
        },
    ))
}

// ──────────────────────────────────────────────
// PATCH /v1/users/me/username
// ──────────────────────────────────────────────

/// Тело запроса на смену username.
#[derive(Debug, serde::Deserialize)]
pub struct ChangeUsernameRequest {
    pub new_username: String,
}

/// `PATCH /v1/users/me/username`
///
/// # Errors
///
/// - `400 BadRequest` — пустой username.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `404 Not Found` — пользователь не найден.
/// - `409 Conflict` — username занят.
/// - `500` — внутренняя ошибка.
pub async fn change_username(
    CurrentAuth(ctx): CurrentAuth,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: ChangeUsernameRequest = decode_body(&headers, &body)?;

    if req.new_username.trim().is_empty() {
        return Err(AppError::BadRequest("new_username cannot be empty".into()));
    }

    let new_blind_index = state.server_identity.blind_index(&req.new_username);

    let user = Users::find_by_id(ctx.user.id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut active: users::ActiveModel = user.into();
    active.username_blind_index = Set(new_blind_index);

    active.update(&state.db).await.map_err(|e| {
        let err_str = e.to_string();
        if err_str.contains("UNIQUE") || err_str.contains("unique") {
            AppError::UsernameTaken
        } else {
            AppError::Db(e)
        }
    })?;

    Ok(typed_response::<()>(&headers, StatusCode::NO_CONTENT, &()))
}


// ──────────────────────────────────────────────
// GET /v1/users/lookup/username?username=<name> (public)
// ──────────────────────────────────────────────

/// Query параметры для username lookup.
#[derive(Debug, serde::Deserialize)]
pub struct UsernameLookupQuery {
    pub username: String,
}

/// Минимальный ответ для username lookup (публичный, без auth).
#[derive(Debug, serde::Serialize)]
pub struct UsernameLookupResponse {
    pub user_id: Uuid,
}

/// `GET /v1/users/lookup/username?username=xxx` (без auth)
///
/// Публичный эндпоинт для QR-логина: по username возвращает user_id.
///
/// # Errors
///
/// - `400 BadRequest` — пустой username.
/// - `404 Not Found` — пользователь не найден.
/// - `500` — внутренняя ошибка.
pub async fn username_lookup(
    State(state): State<AppState>,
    Query(query): Query<UsernameLookupQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if query.username.trim().is_empty() {
        return Err(AppError::BadRequest("username cannot be empty".into()));
    }

    let blind_index = state.server_identity.blind_index(&query.username);

    let user = messenger_entity::users::Entity::find()
        .filter(messenger_entity::users::Column::UsernameBlindIndex.eq(blind_index))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &UsernameLookupResponse {
            user_id: user.id,
        },
    ))
}

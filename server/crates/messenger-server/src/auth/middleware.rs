use axum::async_trait;
use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use sea_orm::EntityTrait;

use crate::auth::header;
use crate::error::AppError;
use crate::state::AppState;

/// Контекст аутентификации, помещаемый в `extensions` запроса.
#[derive(Clone)]
pub struct AuthContext {
    pub device: messenger_entity::devices::Model,
    pub user: messenger_entity::users::Model,
}

/// Middleware аутентификации через подписанный challenge (Ed25519).
///
/// Проверяет:
/// 1. Наличие и корректный формат заголовка `X-Auth-Signature`.
/// 2. Timestamp в окне `clock_skew_tolerance_secs`.
/// 3. Nonce не повторяется (replay protection).
/// 4. Device существует в БД и не отозван.
/// 5. Подпись Ed25519 валидна (method, path, ts, nonce, blake3(body)).
/// 6. Пользователь активен (status == "active").
///
/// В случае успеха кладёт `AuthContext { device, user }` в `extensions`.
///
/// # Errors
///
/// Возвращает `AppError`:
/// - `Unauthorized` — отсутствует или невалиден заголовок `X-Auth-Signature`,
///   device не найден или не совпадает подпись.
/// - `TimestampOutOfWindow` — timestamp вне окна `clock_skew_tolerance_secs`.
/// - `NonceReplay` — nonce уже использован.
/// - `DeviceRevoked` — устройство отозвано.
/// - `BadRequest` — тело запроса слишком большое.
/// - `SignatureInvalid` — Ed25519 подпись не прошла верификацию.
/// - `Forbidden` — пользователь неактивен (не `active`).
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    // 1. Прочитать X-Auth-Signature header
    let header_value = req
        .headers()
        .get("x-auth-signature")
        .ok_or(AppError::Unauthorized)?
        .to_str()
        .map_err(|_| AppError::Unauthorized)?;

    let auth = header::AuthHeader::parse(header_value)?;

    // 2. Проверить timestamp окно
    let now = chrono::Utc::now().timestamp();
    let skew = state.config.clock_skew_tolerance_secs;
    if (auth.timestamp_secs - now).abs() > skew {
        return Err(AppError::TimestampOutOfWindow);
    }

    // 3. Проверить nonce (replay protection)
    if state.nonce_cache.check_and_insert(&auth.nonce) {
        return Err(AppError::NonceReplay);
    }

    // 4. Загрузить device из БД
    let device = messenger_entity::devices::Entity::find_by_id(auth.device_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if device.revoked_at.is_some() {
        return Err(AppError::DeviceRevoked);
    }

    // 5. Прочитать body
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, state.config.max_request_body_bytes)
        .await
        .map_err(|_| AppError::BadRequest("request body too large".into()))?;

    // 6. Сформировать canonical message и проверить подпись
    // Включаем query string в path если он есть (см. S04 Task — Recommendation)
    let signed_path = parts
        .uri
        .path_and_query()
        .map_or_else(|| parts.uri.path().to_string(), ToString::to_string);

    let canonical = messenger_crypto::canonical::build_signed_message(
        parts.method.as_str(),
        &signed_path,
        auth.timestamp_secs,
        &auth.nonce,
        &body_bytes,
    );
    messenger_crypto::signing::verify_ed25519(
        &device.device_signing_public_key,
        &canonical,
        &auth.signature,
    )
    .map_err(|_| AppError::SignatureInvalid)?;

    // 7. Загрузить пользователя и проверить статус
    let user = messenger_entity::users::Entity::find_by_id(device.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.status != "active" {
        return Err(AppError::Forbidden);
    }

    // 8. Восстановить запрос с телом обратно и положить AuthContext в extensions
    let mut req = Request::from_parts(parts, Body::from(body_bytes));
    req.extensions_mut().insert(AuthContext { device, user });

    Ok(next.run(req).await)
}

/// Extractor для получения `AuthContext` в защищённых хендлерах.
///
/// # Errors
///
/// Возвращает `AppError::Unauthorized` если `AuthContext` отсутствует
/// (хендлер не за middleware или middleware не отработал).
#[derive(Clone)]
pub struct CurrentAuth(pub AuthContext);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for CurrentAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .map(Self)
            .ok_or(AppError::Unauthorized)
    }
}

/// Extractor для хендлеров, требующих роль `admin`.
///
/// # Errors
///
/// Возвращает `AppError::Unauthorized` если `AuthContext` отсутствует,
/// или `AppError::Forbidden` если роль не `admin`.
#[derive(Clone)]
pub struct RequireAdmin(pub AuthContext);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for RequireAdmin {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or(AppError::Unauthorized)?;

        if ctx.user.role != "admin" {
            return Err(AppError::Forbidden);
        }

        Ok(Self(ctx))
    }
}

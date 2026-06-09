//! Endpoints для QR-флоу provisioning нового устройства.
//!
//! - `POST   /v1/provisioning/requests` — создание запроса (публичный).
//! - `GET    /v1/provisioning/requests/:id` — детали запроса (auth).
//! - `POST   /v1/provisioning/requests/:id/approve` — одобрение (auth).
//! - `GET    /v1/provisioning/requests/:id/bootstrap` — polling нового устройства (специальная auth).

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, NotSet, QueryFilter, Set,
};
use uuid::Uuid;

use crate::auth::middleware::CurrentAuth;
use crate::error::{decode_body, typed_response, AppError};
use crate::services::invite::{begin_immediate, now_secs};
use crate::state::AppState;

use messenger_entity::device_provisioning_requests;
use messenger_entity::device_provisioning_requests::Entity as ProvRequests;
use messenger_entity::devices;
use messenger_entity::key_change_events;
use messenger_entity::user_identity_credentials;
use messenger_entity::user_identity_credentials::Entity as UserIdentityCredentials;
use messenger_entity::users::Entity as Users;

// ── Request/Response типы ──────────────────────────────────

/// Тело запроса создания provisioning request (шаг 1 — новое устройство).
#[derive(Debug, serde::Deserialize)]
pub struct CreateProvisioningRequest {
    pub user_id: Uuid,

    #[serde(with = "serde_bytes")]
    pub new_device_temp_public_key: Vec<u8>,      // X25519, 32 байта

    #[serde(with = "serde_bytes")]
    pub new_device_temp_signing_public_key: Vec<u8>, // Ed25519, 32 байта

    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,                             // 16-32 байта
}

/// Ответ на создание provisioning request.
#[derive(Debug, serde::Serialize)]
pub struct CreateProvisioningResponse {
    pub provisioning_id: Uuid,
    pub expires_at: i64,
}

/// Детали provisioning запроса (шаг 2 — старое устройство смотрит перед approve).
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, serde::Serialize)]
pub struct ProvisioningDetails {
    pub provisioning_id: Uuid,
    pub user_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub new_device_temp_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    pub status: String,
    pub expires_at: i64,
}

/// Ответ на approve (шаг 3 — старое устройство получает device_id нового).
#[derive(Debug, serde::Serialize)]
pub struct ApproveProvisioningResponse {
    pub device_id: Uuid,
}

/// Тело запроса на approve (шаг 3 — старое устройство).
#[derive(Debug, serde::Deserialize)]
pub struct ApproveProvisioningRequest {
    #[serde(with = "serde_bytes")]
    pub encrypted_bootstrap_blob: Vec<u8>,

    #[serde(with = "serde_bytes")]
    pub new_device_hpke_public_key: Vec<u8>,

    #[serde(with = "serde_bytes")]
    pub new_device_signing_public_key: Vec<u8>,

    #[serde(with = "serde_bytes")]
    pub device_authorization_signature: Vec<u8>,

    pub device_authorization_timestamp: i64,
}

/// Ответ на polling bootstrap (шаг 4 — новое устройство).
#[derive(Debug, serde::Serialize)]
pub struct BootstrapResponse {
    pub new_device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub encrypted_bootstrap_blob: Vec<u8>,
}

// ── Endpoints ───────────────────────────────────────────────

/// `POST /v1/provisioning/requests` — публичный.
///
/// Новое устройство создаёт provisioning request.
/// Сервер проверяет: `user_id` существует, статус active, размеры ключей.
///
/// # Errors
///
/// - `BadRequest` — если ключи невалидны, пользователь не найден или неактивен.
/// - `Db` — ошибка БД.
#[allow(clippy::missing_panics_doc)]
pub async fn create_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: CreateProvisioningRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    // Валидация ключей
    if req.new_device_temp_public_key.len() != 32 {
        return Err(AppError::BadRequest(
            "new_device_temp_public_key must be 32 bytes".into(),
        ));
    }
    if req.new_device_temp_signing_public_key.len() != 32 {
        return Err(AppError::BadRequest(
            "new_device_temp_signing_public_key must be 32 bytes".into(),
        ));
    }
    if req.nonce.len() < 16 || req.nonce.len() > 32 {
        return Err(AppError::BadRequest(
            "nonce must be between 16 and 32 bytes".into(),
        ));
    }

    // Проверить что user существует и активен
    let user = Users::find_by_id(req.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::BadRequest("user not found".into()))?;

    if user.status != "active" {
        return Err(AppError::BadRequest("user is not active".into()));
    }

    let provisioning_id = Uuid::now_v7();
    let expires_at = now + 300; // 5 минут

    device_provisioning_requests::ActiveModel {
        id: Set(provisioning_id),
        user_id: Set(req.user_id),
        new_device_temp_public_key: Set(req.new_device_temp_public_key),
        new_device_temp_signing_public_key: Set(req.new_device_temp_signing_public_key),
        nonce: Set(req.nonce),
        status: Set("pending".to_string()),
        expires_at: Set(expires_at),
        encrypted_bootstrap_blob: Set(None),
        approved_by_device_id: Set(None),
        new_device_id: Set(None),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    Ok(typed_response(
        &headers,
        StatusCode::CREATED,
        &CreateProvisioningResponse {
            provisioning_id,
            expires_at,
        },
    ))
}

/// `GET /v1/provisioning/requests/:id` — auth required.
///
/// Старое устройство смотрит детали перед approve.
/// Проверка: `provisioning.user_id == ctx.user.id`.
///
/// # Errors
///
/// - `NotFound` — запрос не найден.
/// - `Forbidden` — запрос принадлежит другому пользователю.
/// - `Db` — ошибка БД.
#[allow(clippy::missing_panics_doc)]
pub async fn get_request(
    CurrentAuth(ctx): CurrentAuth,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    use device_provisioning_requests::Entity as ProvRequests;

    let request = ProvRequests::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Только владелец может смотреть
    if request.user_id != ctx.user.id {
        return Err(AppError::Forbidden);
    }

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ProvisioningDetails {
            provisioning_id: request.id,
            user_id: request.user_id,
            new_device_temp_public_key: request.new_device_temp_public_key,
            nonce: request.nonce,
            status: request.status,
            expires_at: request.expires_at,
        },
    ))
}

/// `POST /v1/provisioning/requests/:id/approve` — auth required.
///
/// Старое устройство одобряет добавление нового устройства.
/// В транзакции: создаёт device, обновляет provisioning request,
/// вставляет `key_change_event`.
///
/// # Errors
///
/// - `NotFound` — запрос не найден.
/// - `ProvisioningExpired` — статус не pending или истёк срок.
/// - `Forbidden` — запрос принадлежит другому пользователю.
/// - `BadRequest` — timestamp вне окна или неверный формат подписи.
/// - `SignatureInvalid` — подпись не прошла верификацию.
/// - `Db` — ошибка БД.
#[allow(clippy::missing_panics_doc)]
pub async fn approve_request(
    CurrentAuth(ctx): CurrentAuth,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: ApproveProvisioningRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    // Найти provisioning request
    let provisioning = ProvRequests::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Проверить статус
    if provisioning.status != "pending" {
        return Err(AppError::ProvisioningExpired);
    }

    // Проверить expires_at
    if provisioning.expires_at <= now {
        return Err(AppError::ProvisioningExpired);
    }

    // Проверить что provisioning.user_id == ctx.user.id (нельзя апрувать чужое)
    if provisioning.user_id != ctx.user.id {
        return Err(AppError::Forbidden);
    }

    // Проверить device_authorization_timestamp
    if (req.device_authorization_timestamp - now).abs() > 300 {
        return Err(AppError::BadRequest(
            "device_authorization_timestamp out of ±300s window".into(),
        ));
    }

    // Получить identity key пользователя для проверки подписи
    let identity = UserIdentityCredentials::find()
        .filter(user_identity_credentials::Column::UserId.eq(ctx.user.id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let identity_pk = VerifyingKey::from_bytes(
        identity
            .signature_public_key
            .as_slice()
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid stored identity key")))?,
    )
    .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid stored identity key")))?;

    // Проверить device_authorization_signature
    // msg = new_device_signing_pk || new_device_hpke_pk || ts_le
    let ts_bytes = req.device_authorization_timestamp.to_le_bytes();
    let mut auth_msg = Vec::new();
    auth_msg.extend_from_slice(&req.new_device_signing_public_key);
    auth_msg.extend_from_slice(&req.new_device_hpke_public_key);
    auth_msg.extend_from_slice(&ts_bytes);

    let auth_sig = Signature::from_slice(&req.device_authorization_signature)
        .map_err(|_| AppError::BadRequest("invalid device_authorization_signature format".into()))?;

    identity_pk
        .verify(&auth_msg, &auth_sig)
        .map_err(|_| AppError::SignatureInvalid)?;

    // Транзакция: создать device + обновить provisioning + key_change_event
    let txn = begin_immediate(&state.db).await?;

    // 1. Создать новое устройство
    let new_device_id = Uuid::now_v7();
    devices::ActiveModel {
        id: Set(new_device_id),
        user_id: Set(ctx.user.id),
        hpke_init_public_key: Set(req.new_device_hpke_public_key.clone()),
        device_signing_public_key: Set(req.new_device_signing_public_key.clone()),
        authorization_signature: Set(req.device_authorization_signature.clone()),
        authorized_by_device_id: Set(Some(ctx.device.id)),
        created_at: Set(now),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(&txn)
    .await?;

    // 2. UPDATE provisioning request
    let mut active: device_provisioning_requests::ActiveModel = provisioning.into();
    active.status = Set("approved".to_string());
    active.approved_by_device_id = Set(Some(ctx.device.id));
    active.new_device_id = Set(Some(new_device_id));
    active.encrypted_bootstrap_blob = Set(Some(req.encrypted_bootstrap_blob));
    active.update(&txn).await?;

    // 3. INSERT key_change_event
    key_change_events::ActiveModel {
        id: NotSet,
        user_id: Set(ctx.user.id),
        device_id: Set(new_device_id),
        event_type: Set("device_added".to_string()),
        created_at: Set(now),
    }
    .insert(&txn)
    .await?;

    txn.commit().await?;

    let resp = ApproveProvisioningResponse {
        device_id: new_device_id,
    };
    Ok(typed_response(&headers, StatusCode::OK, &resp))
}

/// `GET /v1/provisioning/requests/:id/bootstrap` — специальная auth.
///
/// Новое устройство поллит provisioning request с подписью `temp_signing_key`.
/// - Если ещё pending → 202 Accepted.
/// - Если approved → 200 OK с bootstrap blob (one-shot: после выдачи статус → consumed).
///
/// Аутентификация через header `X-Provisioning-Signature`:
/// Формат: `<ts>:<nonce_hex>:<signature_hex>`
/// Подпись: `"GET\n/v1/provisioning/requests/{id}/bootstrap\n{ts}\n{nonce_hex}\n{empty_body_blake3}"`
///
/// # Errors
///
/// - `Unauthorized` — отсутствует или невалиден заголовок `X-Provisioning-Signature`.
/// - `TimestampOutOfWindow` — timestamp вне окна `clock_skew_tolerance_secs`.
/// - `NonceReplay` — nonce уже использован.
/// - `NotFound` — запрос не найден.
/// - `ProvisioningExpired` — срок истёк или статус не `approved`.
/// - `SignatureInvalid` — подпись не прошла верификацию.
/// - `Db` — ошибка БД.
#[allow(clippy::missing_panics_doc)]
pub async fn get_provisioning_bootstrap(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let now = now_secs();

    // Парсим специальный auth header.
    // Предпочитаем X-Provisioning-Signature (3 parts: ts:nonce:sig).
    // Если его нет, пробуем X-Auth-Signature (4 parts: pk:ts:nonce:sig) для совместимости
    // со старыми клиентами, которые ошибочно отправляют X-Auth-Signature.
    let (ts, nonce_hex, sig_hex) = if let Some(val) = headers.get("x-provisioning-signature") {
        let auth_header = val.to_str().map_err(|_| AppError::Unauthorized)?;
        parse_provisioning_signature(auth_header)?
    } else if let Some(val) = headers.get("x-auth-signature") {
        let auth_header = val.to_str().map_err(|_| AppError::Unauthorized)?;
        // X-Auth-Signature format: <pk_hex>:<ts>:<nonce_hex>:<sig_hex>
        // We extract parts 2-4 (ts, nonce, sig)
        let parts: Vec<&str> = auth_header.split(':').collect();
        if parts.len() != 4 {
            return Err(AppError::BadRequest(
                "expected X-Auth-Signature format <pk_hex>:<ts>:<nonce_hex>:<sig_hex>".into(),
            ));
        }
        let ts = parts[1]
            .parse::<i64>()
            .map_err(|_| AppError::BadRequest("invalid timestamp".into()))?;
        (ts, parts[2].to_string(), parts[3].to_string())
    } else {
        return Err(AppError::Unauthorized);
    };

    // Проверка timestamp
    let skew = state.config.clock_skew_tolerance_secs;
    if (ts - now).abs() > skew {
        return Err(AppError::TimestampOutOfWindow);
    }

    // Nonce replay check
    let nonce =
        hex::decode(&nonce_hex).map_err(|_| AppError::BadRequest("invalid nonce hex".into()))?;
    if state.nonce_cache.check_and_insert(&nonce) {
        return Err(AppError::NonceReplay);
    }

    // Найти provisioning request
    let provisioning = ProvRequests::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Проверить expiration
    if provisioning.expires_at <= now {
        return Err(AppError::ProvisioningExpired);
    }

    // Проверить подпись: canonical message без body (GET)
    let path = format!("/v1/provisioning/requests/{id}/bootstrap");
    let canonical = messenger_crypto::canonical::build_signed_message(
        "GET",
        &path,
        ts,
        &nonce,
        &body, // должно быть пустым
    );

    let sig = Signature::from_slice(
        &hex::decode(&sig_hex)
            .map_err(|_| AppError::BadRequest("invalid signature hex".into()))?,
    )
    .map_err(|_| AppError::BadRequest("invalid signature format".into()))?;

    let temp_pk = VerifyingKey::from_bytes(
        provisioning
            .new_device_temp_signing_public_key
            .as_slice()
            .try_into()
            .map_err(|_| {
                AppError::Internal(anyhow::anyhow!("invalid stored temp signing key"))
            })?,
    )
    .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid stored temp signing key")))?;

    temp_pk
        .verify(&canonical, &sig)
        .map_err(|_| AppError::SignatureInvalid)?;

    // Если ещё pending — 202
    if provisioning.status == "pending" {
        return Ok(Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(axum::body::Body::empty())
            .expect("static response builder"));
    }

    // Если approved — отдаём blob и помечаем consumed
    if provisioning.status == "approved" {
        let bootstrap_blob = provisioning
            .encrypted_bootstrap_blob
            .clone()
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("approved without blob")))?;

        let new_device_id = provisioning
            .new_device_id
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("approved without device_id")))?;

        // One-shot: атомарно переводим в consumed и обнуляем blob
        let mut active: device_provisioning_requests::ActiveModel = provisioning.into();
        active.status = Set("consumed".to_string());
        active.encrypted_bootstrap_blob = Set(None);
        active.update(&state.db).await?;

        let resp = BootstrapResponse {
            new_device_id,
            encrypted_bootstrap_blob: bootstrap_blob,
        };
        return Ok(typed_response(&headers, StatusCode::OK, &resp));
    }

    // Любой другой статус (expired, consumed, etc.) — 410
    Err(AppError::ProvisioningExpired)
}

// ── Helpers ─────────────────────────────────────────────────

/// Парсит заголовок `X-Provisioning-Signature`.
///
/// Формат: `<ts>:<nonce_hex>:<signature_hex>`
///
/// # Errors
///
/// - `BadRequest` — если формат неверный или timestamp не парсится.
fn parse_provisioning_signature(header: &str) -> Result<(i64, String, String), AppError> {
    let parts: Vec<&str> = header.split(':').collect();
    if parts.len() != 3 {
        return Err(AppError::BadRequest(
            "X-Provisioning-Signature must be <ts>:<nonce_hex>:<sig_hex>".into(),
        ));
    }

    let ts = parts[0]
        .parse::<i64>()
        .map_err(|_| AppError::BadRequest("invalid timestamp in provisioning signature".into()))?;

    Ok((ts, parts[1].to_string(), parts[2].to_string()))
}

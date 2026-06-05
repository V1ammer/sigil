//! Публичный endpoint `POST /v1/invite/redeem`.
//!
//! Регистрирует нового пользователя (`NewUser`) или добавляет устройство
//! существующему пользователю (`NewDevice`) по инвайт-токену.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::body::Bytes;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sea_orm::{ActiveModelTrait, EntityTrait, NotSet, Set};
use uuid::Uuid;

use crate::error::{decode_body, typed_response, AppError};
use crate::services::invite::{begin_immediate, consume_token, now_secs, validate_token};
use crate::state::AppState;
use messenger_entity::users::{self, Entity as Users};
use messenger_entity::user_identity_credentials::{self, Entity as UserIdentityCredentials};
use sea_orm::ColumnTrait;
use sea_orm::QueryFilter;

/// Тип redeem-запроса.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedeemKind {
    NewUser,
    NewDevice,
}

/// Тело запроса на redeem инвайт-токена.
#[derive(Debug, serde::Deserialize)]
pub struct RedeemRequest {
    /// Инвайт-токен (base64 url-safe no-pad строка).
    pub token: String,

    /// Тип регистрации.
    pub kind: RedeemKind,

    // ── `NewUser` поля ──
    /// MLS `BasicCredential` (сериализованный).
    #[serde(default)]
    pub identity_credential: Option<Vec<u8>>,

    /// Ed25519 public key пользователя (32 байта).
    #[serde(default)]
    pub signature_public_key: Option<Vec<u8>>,

    /// Plaintext username (только для `NewUser`).
    pub username: Option<String>,

    // ── `NewDevice` поля ──
    /// Подпись challenge'а identity-ключом пользователя (только для `NewDevice`).
    #[serde(default)]
    pub existing_identity_proof: Option<Vec<u8>>,

    // ── Поля для нового устройства (всегда) ──
    /// HPKE init public key нового устройства.
    #[serde(with = "serde_bytes")]
    pub device_init_public_key: Vec<u8>,

    /// Ed25519 signing public key нового устройства.
    #[serde(with = "serde_bytes")]
    pub device_signing_public_key: Vec<u8>,

    /// Подпись identity-ключом пользователя:
    /// `Ed25519(identity_sk, device_signing_pk || device_init_pk || ts_le)`.
    #[serde(with = "serde_bytes")]
    pub device_authorization_signature: Vec<u8>,

    /// Timestamp в секундах (для проверки `device_authorization_signature`).
    /// Должен быть в пределах ±300 секунд от now.
    pub device_authorization_timestamp: i64,
}

/// Успешный ответ на redeem.
#[derive(Debug, serde::Serialize)]
pub struct RedeemResponse {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub role: String,
}

/// `POST /v1/invite/redeem`
///
/// Регистрирует нового пользователя или добавляет устройство.
///
/// # Errors
///
/// - `400 BadRequest` — невалидные параметры запроса.
/// - `409 Conflict` — username занят.
/// - `500` — внутренняя ошибка.
pub async fn redeem(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: RedeemRequest = decode_body(&headers, &body)?;
    tracing::info!(kind = ?req.kind, username = ?req.username, "redeem request received");

    let now = now_secs();

    match req.kind {
        RedeemKind::NewUser => handle_new_user(&state, &req, now).await,
        RedeemKind::NewDevice => handle_new_device(&state, &req, now).await,
    }
}

/// Обрабатывает регистрацию нового пользователя.
async fn handle_new_user(
    state: &AppState,
    req: &RedeemRequest,
    now: i64,
) -> Result<Response, AppError> {
    tracing::info!("handle_new_user: step 0 - validating fields");
    // ── валидация полей ──
    let identity_credential = req
        .identity_credential
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("missing identity_credential".into()))?;
    let signature_pk_bytes = req
        .signature_public_key
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("missing signature_public_key".into()))?;
    let username = req
        .username
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("missing username".into()))?;

    if username.trim().is_empty() {
        return Err(AppError::BadRequest("username cannot be empty".into()));
    }

    let signature_pk = VerifyingKey::from_bytes(
        signature_pk_bytes
            .try_into()
            .map_err(|_| AppError::BadRequest("signature_public_key must be 32 bytes".into()))?,
    )
    .map_err(|_| AppError::BadRequest("invalid signature_public_key".into()))?;

    tracing::info!("handle_new_user: step 1 - verifying device auth signature");
    // ── проверка device_authorization_signature ──
    verify_device_auth_signature(&signature_pk, req)?;

    if (req.device_authorization_timestamp - now).abs() > 300 {
        return Err(AppError::BadRequest(
            "device_authorization_timestamp out of ±300s window".into(),
        ));
    }

    tracing::info!("handle_new_user: step 2 - beginning transaction");
    // ── транзакция ──
    let txn = begin_immediate(&state.db).await?;

    tracing::info!("handle_new_user: step 3 - validating token");
    let token_row = validate_token(&txn, &req.token).await?;

    // Admin-токены только для NewUser
    if token_row.role_to_grant == "admin" {
        // уже NewUser по match выше, ничего делать не нужно
    }

    tracing::info!(
        token_role = %token_row.role_to_grant,
        "handle_new_user: step 4a - before blind_index"
    );

    let blind_index = state.server_identity.blind_index(username);
    tracing::info!("handle_new_user: step 4b - blind_index done");

    // Pre-check: is this username already taken?
    let existing = Users::find()
        .filter(users::Column::UsernameBlindIndex.eq(blind_index.clone()))
        .one(&txn)
        .await?;
    if existing.is_some() {
        tracing::info!("handle_new_user: step 4b5 - username already taken");
        return Err(AppError::UsernameTaken);
    }
    tracing::info!("handle_new_user: step 4b6 - username available");

    let user_id = Uuid::now_v7();
    tracing::info!("handle_new_user: step 4c - uuid generated");
    let device_id = Uuid::now_v7();

    // INSERT users
    users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set(blind_index.clone()),
        username_hash_version: Set(state.server_identity.username_hash_version),
        role: Set(token_row.role_to_grant.clone()),
        status: Set("active".to_string()),
        created_at: Set(now / 86400 * 86400),
        send_read_receipts: Set(false),
    }
    .insert(&txn)
    .await
    .map_err(map_unique_violation)?;

    tracing::info!("handle_new_user: step 4d - user inserted successfully");

    tracing::info!("handle_new_user: step 5 - inserting credential");
    // INSERT user_identity_credentials
    user_identity_credentials::ActiveModel {
        user_id: Set(user_id),
        signature_public_key: Set(signature_pk_bytes.to_vec()),
        credential: Set(identity_credential.to_vec()),
        created_at: Set(now),
    }
    .insert(&txn)
    .await?;

    tracing::info!("handle_new_user: step 6 - inserted credential, inserting device");
    // INSERT device
    insert_device(&txn, user_id, device_id, req, now).await?;

    tracing::info!("handle_new_user: step 7 - inserted device, consuming token");
    // consume token
    consume_token(&txn, token_row.id, user_id, device_id).await?;

    tracing::info!("handle_new_user: step 8 - token consumed, inserting key_change_event");
    // key_change_event
    insert_key_change_event(&txn, user_id, device_id, "device_added", now).await?;

    tracing::info!("handle_new_user: step 9 - committing");
    txn.commit().await?;

    tracing::info!("handle_new_user: step 10 - done, responding");
    Ok(typed_response(
        &HeaderMap::new(),
        StatusCode::CREATED,
        &RedeemResponse {
            user_id,
            device_id,
            role: token_row.role_to_grant,
        },
    ))
}

/// Обрабатывает добавление нового устройства существующему пользователю.
async fn handle_new_device(
    state: &AppState,
    req: &RedeemRequest,
    now: i64,
) -> Result<Response, AppError> {
    // ── валидация полей ──
    let signature_pk_bytes = req
        .signature_public_key
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("missing signature_public_key".into()))?;

    let existing_identity_proof = req
        .existing_identity_proof
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("missing existing_identity_proof".into()))?;

    let signature_pk = VerifyingKey::from_bytes(
        signature_pk_bytes
            .try_into()
            .map_err(|_| AppError::BadRequest("signature_public_key must be 32 bytes".into()))?,
    )
    .map_err(|_| AppError::BadRequest("invalid signature_public_key".into()))?;

    // ── транзакция ──
    let txn = begin_immediate(&state.db).await?;

    let token_row = validate_token(&txn, &req.token).await?;

    // Найти пользователя по identity key
    let identity_row = UserIdentityCredentials::find()
        .filter(user_identity_credentials::Column::SignaturePublicKey.eq(signature_pk_bytes.to_vec()))
        .one(&txn)
        .await?
        .ok_or(AppError::IdentityNotFound)?;

    // Проверить existing_identity_proof: challenge = "messenger-provisioning-v1:" || token_str || ":" || ts_le
    let challenge = build_provisioning_challenge(&req.token, req.device_authorization_timestamp);
    let sig = Signature::from_slice(existing_identity_proof)
        .map_err(|_| AppError::BadRequest("invalid existing_identity_proof format".into()))?;
    signature_pk
        .verify(&challenge, &sig)
        .map_err(|_| AppError::SignatureInvalid)?;

    // Проверить device_authorization_signature
    verify_device_auth_signature(&signature_pk, req)?;

    if (req.device_authorization_timestamp - now).abs() > 300 {
        return Err(AppError::BadRequest(
            "device_authorization_timestamp out of ±300s window".into(),
        ));
    }

    let user_id = identity_row.user_id;
    let device_id = Uuid::now_v7();

    // INSERT device
    insert_device(&txn, user_id, device_id, req, now).await?;

    // consume token
    consume_token(&txn, token_row.id, user_id, device_id).await?;

    // key_change_event
    insert_key_change_event(&txn, user_id, device_id, "device_added", now).await?;

    // Загрузить пользователя для получения роли
    let user = Users::find_by_id(user_id)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;

    txn.commit().await?;

    Ok(typed_response(
        &HeaderMap::new(),
        StatusCode::CREATED,
        &RedeemResponse {
            user_id,
            device_id,
            role: user.role,
        },
    ))
}

/// Вставляет запись `devices`.
async fn insert_device(
    txn: &sea_orm::DatabaseTransaction,
    user_id: Uuid,
    device_id: Uuid,
    req: &RedeemRequest,
    now: i64,
) -> Result<(), AppError> {
    use messenger_entity::devices;
    devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(req.device_init_public_key.clone()),
        device_signing_public_key: Set(req.device_signing_public_key.clone()),
        authorization_signature: Set(req.device_authorization_signature.clone()),
        authorized_by_device_id: Set(None),
        created_at: Set(now),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(txn)
    .await?;
    Ok(())
}

/// Вставляет запись `key_change_events`.
async fn insert_key_change_event(
    txn: &sea_orm::DatabaseTransaction,
    user_id: Uuid,
    device_id: Uuid,
    event_type: &str,
    now: i64,
) -> Result<(), AppError> {
    use messenger_entity::key_change_events;
    key_change_events::ActiveModel {
        id: NotSet,
        user_id: Set(user_id),
        device_id: Set(device_id),
        event_type: Set(event_type.to_string()),
        created_at: Set(now),
    }
    .insert(txn)
    .await?;
    Ok(())
}

/// Проверяет `device_authorization_signature`.
///
/// Message: `device_signing_public_key || device_init_public_key || device_authorization_timestamp` (LE i64, 8 байт).
fn verify_device_auth_signature(
    identity_pk: &VerifyingKey,
    req: &RedeemRequest,
) -> Result<(), AppError> {
    let ts_bytes = req.device_authorization_timestamp.to_le_bytes();
    let mut msg = Vec::with_capacity(
        req.device_signing_public_key.len() + req.device_init_public_key.len() + 8,
    );
    msg.extend_from_slice(&req.device_signing_public_key);
    msg.extend_from_slice(&req.device_init_public_key);
    msg.extend_from_slice(&ts_bytes);

    let sig = Signature::from_slice(&req.device_authorization_signature)
        .map_err(|_| AppError::BadRequest("invalid device_authorization_signature format".into()))?;

    identity_pk
        .verify(&msg, &sig)
        .map_err(|_| AppError::SignatureInvalid)
}

/// Строит challenge для `existing_identity_proof`:
/// `b"messenger-provisioning-v1:" || token_str_bytes || b":" || ts_le_bytes`
fn build_provisioning_challenge(token_str: &str, ts: i64) -> Vec<u8> {
    let prefix = b"messenger-provisioning-v1:";
    let ts_bytes = ts.to_le_bytes();
    let mut challenge = Vec::with_capacity(prefix.len() + token_str.len() + 1 + 8);
    challenge.extend_from_slice(prefix);
    challenge.extend_from_slice(token_str.as_bytes());
    challenge.push(b':');
    challenge.extend_from_slice(&ts_bytes);
    challenge
}

/// Маппит UNIQUE violation на `UsernameTaken`.
fn map_unique_violation(e: sea_orm::DbErr) -> AppError {
    let err_str = e.to_string();
    if err_str.contains("UNIQUE") || err_str.contains("unique") {
        return AppError::UsernameTaken;
    }
    AppError::Db(e)
}

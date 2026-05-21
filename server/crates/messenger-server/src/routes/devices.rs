//! Endpoints для работы с устройствами.
//!
//! - `GET /v1/devices/me` — список своих устройств.
//! - `POST /v1/devices/me/:device_id/revoke` — отозвать устройство.
//! - `GET /v1/users/:user_id/devices` — список активных устройств пользователя.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, NotSet, QueryFilter, QuerySelect, Set,
};
use uuid::Uuid;

use crate::auth::middleware::CurrentAuth;
use crate::error::{decode_body, typed_response, AppError};
use crate::routes::ws::notify_key_change;
use crate::services::invite::now_secs;
use crate::state::AppState;
use messenger_entity::devices::{self, Entity as Devices};
use messenger_entity::key_change_events;
use messenger_entity::mls_group_members;
use messenger_entity::user_identity_credentials::{self, Entity as UserIdentityCredentials};

// ──────────────────────────────────────────────
// GET /v1/devices/me
// ──────────────────────────────────────────────

/// Информация об устройстве (для владельца).
#[derive(Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub id: Uuid,
    #[serde(with = "serde_bytes")]
    pub hpke_init_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_signing_public_key: Vec<u8>,
    pub authorized_by_device_id: Option<Uuid>,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub is_current: bool,
}

/// Ответ на `GET /v1/devices/me`.
#[derive(Debug, serde::Serialize)]
pub struct ListMyDevicesResponse {
    pub devices: Vec<DeviceInfo>,
}

/// `GET /v1/devices/me`
///
/// # Errors
///
/// - `401 Unauthorized` — отсутствует auth context.
/// - `500` — внутренняя ошибка.
pub async fn list_my_devices(
    CurrentAuth(ctx): CurrentAuth,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let devices = Devices::find()
        .filter(devices::Column::UserId.eq(ctx.user.id))
        .all(&state.db)
        .await?;

    let my_device_id = ctx.device.id;

    let info: Vec<DeviceInfo> = devices
        .into_iter()
        .map(|d| DeviceInfo {
            id: d.id,
            hpke_init_public_key: d.hpke_init_public_key,
            device_signing_public_key: d.device_signing_public_key,
            authorized_by_device_id: d.authorized_by_device_id,
            created_at: d.created_at,
            revoked_at: d.revoked_at,
            is_current: d.id == my_device_id,
        })
        .collect();

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ListMyDevicesResponse { devices: info },
    ))
}

// ──────────────────────────────────────────────
// POST /v1/devices/me/:device_id/revoke
// ──────────────────────────────────────────────

/// Тело запроса на ревокацию устройства.
#[derive(Debug, serde::Deserialize)]
pub struct RevokeDeviceRequest {
    /// Подпись identity-ключом пользователя:
    /// `Ed25519(identity_sk, "revoke:" || device_id_bytes || ":" || ts_string)`.
    #[serde(with = "serde_bytes")]
    pub revocation_signature: Vec<u8>,

    /// Timestamp подписи (в секундах).
    pub revocation_timestamp: i64,
}

/// `POST /v1/devices/me/:device_id/revoke`
///
/// # Errors
///
/// - `400 BadRequest` — невалидные параметры запроса.
/// - `401 Unauthorized` — отсутствует auth context.
/// - `404 Not Found` — устройство не найдено.
/// - `422 SignatureInvalid` — неверная подпись.
/// - `500` — внутренняя ошибка.
#[allow(clippy::too_many_lines)]
pub async fn revoke_device(
    CurrentAuth(ctx): CurrentAuth,
    State(state): State<AppState>,
    Path(target_device_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: RevokeDeviceRequest = decode_body(&headers, &body)?;

    let now = now_secs();

    // Проверка timestamp
    if (req.revocation_timestamp - now).abs() > 300 {
        return Err(AppError::BadRequest(
            "revocation_timestamp out of ±300s window".into(),
        ));
    }

    // Найти целевое устройство
    let target_device = Devices::find_by_id(target_device_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Устройство должно принадлежать этому пользователю
    if target_device.user_id != ctx.user.id {
        return Err(AppError::NotFound);
    }

    // Idempotent: уже revoked → 204
    if target_device.revoked_at.is_some() {
        return Ok(typed_response::<()>(
            &headers,
            StatusCode::NO_CONTENT,
            &(),
        ));
    }

    // Получить identity key пользователя
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

    // Проверить подпись: msg = "revoke:" || device_id_bytes || ":" || ts.to_string()
    let ts_str = req.revocation_timestamp.to_string();
    let mut msg = Vec::new();
    msg.extend_from_slice(b"revoke:");
    msg.extend_from_slice(target_device_id.as_bytes());
    msg.push(b':');
    msg.extend_from_slice(ts_str.as_bytes());

    let sig = Signature::from_slice(&req.revocation_signature)
        .map_err(|_| AppError::BadRequest("invalid revocation_signature format".into()))?;

    identity_pk
        .verify(&msg, &sig)
        .map_err(|_| AppError::SignatureInvalid)?;

    // UPDATE revoked_at, revoked_by_device_id
    let mut active: devices::ActiveModel = target_device.into();
    active.revoked_at = Set(Some(now));
    active.revoked_by_device_id = Set(Some(ctx.device.id));
    active.update(&state.db).await?;

    // INSERT key_change_event
    key_change_events::ActiveModel {
        id: NotSet,
        user_id: Set(ctx.user.id),
        device_id: Set(target_device_id),
        event_type: Set("device_revoked".to_string()),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    // WS уведомление: найти контакты (users в общих группах)
    let state_clone = state.clone();
    let user_id = ctx.user.id;
    tokio::spawn(async move {
        let contacts = find_contacts(&state_clone, user_id).await;
        notify_key_change(
            &state_clone,
            &contacts,
            user_id,
            target_device_id,
            "revoked",
        )
        .await;
    });

    Ok(typed_response::<()>(&headers, StatusCode::NO_CONTENT, &()))
}

// ──────────────────────────────────────────────
// GET /v1/users/:user_id/devices
// ──────────────────────────────────────────────

/// Публичная информация об устройстве.
#[derive(Debug, serde::Serialize)]
pub struct PublicDeviceInfo {
    pub id: Uuid,
    #[serde(with = "serde_bytes")]
    pub hpke_init_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub device_signing_public_key: Vec<u8>,
    pub created_at: i64,
}

/// `GET /v1/users/:user_id/devices`
///
/// # Errors
///
/// - `401 Unauthorized` — отсутствует auth context.
/// - `500` — внутренняя ошибка.
pub async fn list_user_devices(
    CurrentAuth(_ctx): CurrentAuth,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // Только активные (revoked_at IS NULL)
    let devices = Devices::find()
        .filter(devices::Column::UserId.eq(user_id))
        .filter(devices::Column::RevokedAt.is_null())
        .all(&state.db)
        .await?;

    let info: Vec<PublicDeviceInfo> = devices
        .into_iter()
        .map(|d| PublicDeviceInfo {
            id: d.id,
            hpke_init_public_key: d.hpke_init_public_key,
            device_signing_public_key: d.device_signing_public_key,
            created_at: d.created_at,
        })
        .collect();

    Ok(typed_response(&headers, StatusCode::OK, &info))
}

// ─── Helpers ───

/// Находит пользователей, которые имеют общие группы с `user_id`.
async fn find_contacts(state: &AppState, user_id: Uuid) -> Vec<Uuid> {
    // Найти все group_id, где user_id — активный member
    let Ok(group_ids) = mls_group_members::Entity::find()
        .select_only()
        .column(mls_group_members::Column::GroupId)
        .filter(
            Condition::all()
                .add(mls_group_members::Column::UserId.eq(user_id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .into_tuple::<Uuid>()
        .all(&state.db)
        .await
    else {
        return Vec::new();
    };

    if group_ids.is_empty() {
        return Vec::new();
    }

    // Найти всех active members в этих группах, исключая user_id
    mls_group_members::Entity::find()
        .select_only()
        .column(mls_group_members::Column::UserId)
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.is_in(group_ids))
                .add(mls_group_members::Column::UserId.ne(user_id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .distinct()
        .into_tuple::<Uuid>()
        .all(&state.db)
        .await
        .unwrap_or_default()
}

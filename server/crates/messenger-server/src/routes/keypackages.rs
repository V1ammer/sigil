//! Endpoints для публикации, статистики и выдачи (claim) MLS `KeyPackages`.
//!
//! `KeyPackage` — это одноразовый токен, который устройство публикует для того,
//! чтобы другие участники группы могли добавить его в MLS-группу (Add Proposal).
//!
//! ## Key security invariants
//! - `KeyPackage` используется **не более одного раза** (forward secrecy welcome).
//! - `init_key_hash` (`BLAKE3(init_key)`) — UNIQUE, дедуп по нему.
//! - Last-resort `KeyPackage` переиспользуется (не помечается consumed).
//! - Claim атомарен через `SERIALIZABLE` транзакцию.
//! - Pool size per-device ограничен 1000 (защита от `DoS`).

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use messenger_entity::devices::Entity as Devices;
use messenger_entity::key_packages::{self, Entity as KeyPackages};
use messenger_entity::users::Entity as Users;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, IsolationLevel, PaginatorTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::CurrentAuth;
use crate::error::{decode_body, typed_response, AppError};
use crate::services::invite::now_secs;
use crate::state::AppState;

// ─── Request/Response types ───

/// Запрос на публикацию батча `KeyPackages`.
#[derive(Deserialize)]
pub struct PublishKeyPackagesRequest {
    pub key_packages: Vec<KeyPackageUpload>,
}

/// Один `KeyPackage` в батче.
#[derive(Deserialize)]
pub struct KeyPackageUpload {
    #[serde(with = "serde_bytes")]
    pub key_package: Vec<u8>, // serialized MLS KeyPackage
    #[serde(with = "serde_bytes")]
    pub init_key_hash: Vec<u8>, // BLAKE3(init_key) = 32 байт
    pub expires_at: i64,        // unix timestamp
    pub is_last_resort: bool,
}

/// Ответ на публикацию.
#[derive(Serialize)]
pub struct PublishKeyPackagesResponse {
    pub stored_count: usize,
    pub skipped_count: usize,
    pub current_pool_size: i64,
    pub last_resort_present: bool,
}

/// Статистика пула текущего устройства.
#[derive(Serialize)]
pub struct PoolStats {
    pub available: i64,
    pub consumed_total: i64,
    pub last_resort_present: bool,
    pub oldest_available_created_at: Option<i64>,
}

/// Ответ на claim `KeyPackage`.
#[derive(Serialize)]
pub struct ClaimKeyPackageResponse {
    pub key_package_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub key_package: Vec<u8>,
}

/// Ответ на удаление собственного пула `KeyPackages`.
#[derive(Serialize)]
pub struct DeleteKeyPackagesResponse {
    pub deleted: u64,
}

// ─── Constants ───

/// Максимум `KeyPackages` в одном запросе.
const MAX_KEYPACKAGES_PER_BATCH: usize = 100;

/// Максимум неconsumed `KeyPackages` на устройство (защита от `DoS`).
const MAX_POOL_SIZE_PER_DEVICE: i64 = 1000;

/// Размер `init_key_hash` (BLAKE3 = 32 байта).
const INIT_KEY_HASH_LEN: usize = 32;

// ─── Handlers ───

/// Публикует батч `KeyPackages`.
///
/// Каждый `KeyPackage` проходит базовую валидацию: длина `key_package`, длина
/// `init_key_hash`, `expires_at > now`. При нарушении UNIQUE (`init_key_hash`)
/// запись пропускается (не 500).
///
/// После вставки считает текущий пул устройства и проверяет last-resort.
///
/// # Errors
///
/// - `400 ERR_BAD_REQUEST` — превышен лимит батча (100), невалидные размеры,
///   превышен лимит пула (1000).
/// - `500 ERR_INTERNAL` — ошибка БД.
#[allow(clippy::too_many_lines, clippy::cast_possible_wrap)]
pub async fn publish_keypackages(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: PublishKeyPackagesRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    // Ограничение батча
    if req.key_packages.is_empty() {
        return Err(AppError::BadRequest("empty batch".into()));
    }
    if req.key_packages.len() > MAX_KEYPACKAGES_PER_BATCH {
        return Err(AppError::BadRequest(format!(
            "batch too large: max {MAX_KEYPACKAGES_PER_BATCH}"
        )));
    }

    // Базовая валидация каждого upload
    for upload in &req.key_packages {
        if upload.key_package.is_empty() {
            return Err(AppError::BadRequest("empty key_package".into()));
        }
        if upload.init_key_hash.len() != INIT_KEY_HASH_LEN {
            return Err(AppError::BadRequest(format!(
                "init_key_hash must be {INIT_KEY_HASH_LEN} bytes"
            )));
        }
        if upload.expires_at <= now {
            return Err(AppError::BadRequest("expires_at must be in the future".into()));
        }
    }

    // Проверка pool size limit до вставки
    let current_pool = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::ConsumedAt.is_null())
                .add(key_packages::Column::IsLastResort.eq(false))
                .add(key_packages::Column::ExpiresAt.gt(now)),
        )
        .count(&state.db)
        .await? as i64;

    let batch_len = req.key_packages.len() as i64;
    if current_pool + batch_len > MAX_POOL_SIZE_PER_DEVICE {
        return Err(AppError::BadRequest(format!(
            "pool size limit ({MAX_POOL_SIZE_PER_DEVICE}) exceeded"
        )));
    }

    // Транзакция: вставка с дедупом
    let txn = state
        .db
        .begin_with_config(
            Some(IsolationLevel::Serializable),
            Some(sea_orm::AccessMode::ReadWrite),
        )
        .await?;

    let mut stored_count: usize = 0;
    let mut skipped_count: usize = 0;

    for upload in &req.key_packages {
        let insert_result = key_packages::ActiveModel {
            id: Set(Uuid::now_v7()),
            device_id: Set(ctx.device.id),
            key_package: Set(upload.key_package.clone()),
            init_key_hash: Set(upload.init_key_hash.clone()),
            created_at: Set(now),
            expires_at: Set(upload.expires_at),
            consumed_at: Set(None),
            consumed_by_user_id: Set(None),
            is_last_resort: Set(upload.is_last_resort),
        }
        .insert(&txn)
        .await;

        match insert_result {
            Ok(_) => {
                stored_count += 1;
            }
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(ref e)))
                if is_unique_violation(e) =>
            {
                skipped_count += 1;
            }
            Err(e) => return Err(AppError::Db(e)),
        }
    }

    txn.commit().await?;

    // Посчитать текущий пул после вставки
    let new_pool_size = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::ConsumedAt.is_null())
                .add(key_packages::Column::IsLastResort.eq(false))
                .add(key_packages::Column::ExpiresAt.gt(now_secs())),
        )
        .count(&state.db)
        .await? as i64;

    // Проверить наличие last-resort
    let lr_count = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::IsLastResort.eq(true))
                .add(key_packages::Column::ExpiresAt.gt(now_secs())),
        )
        .count(&state.db)
        .await?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &PublishKeyPackagesResponse {
            stored_count,
            skipped_count,
            current_pool_size: new_pool_size,
            last_resort_present: lr_count > 0,
        },
    ))
}

/// Возвращает статистику пула `KeyPackages` текущего устройства.
///
/// # Errors
///
/// - `500 ERR_INTERNAL` — ошибка БД.
#[allow(clippy::cast_possible_wrap)]
pub async fn get_pool_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
) -> Result<Response, AppError> {
    let now = now_secs();

    // Available: not consumed, not last_resort, not expired
    let available = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::ConsumedAt.is_null())
                .add(key_packages::Column::IsLastResort.eq(false))
                .add(key_packages::Column::ExpiresAt.gt(now)),
        )
        .count(&state.db)
        .await? as i64;

    // Consumed total (historical)
    let consumed_total = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::ConsumedAt.is_not_null()),
        )
        .count(&state.db)
        .await? as i64;

    // Last resort present?
    let lr_count = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::IsLastResort.eq(true))
                .add(key_packages::Column::ExpiresAt.gt(now)),
        )
        .count(&state.db)
        .await?;

    // Oldest available created_at
    let oldest = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::ConsumedAt.is_null())
                .add(key_packages::Column::IsLastResort.eq(false))
                .add(key_packages::Column::ExpiresAt.gt(now)),
        )
        .order_by_asc(key_packages::Column::CreatedAt)
        .one(&state.db)
        .await?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &PoolStats {
            available,
            consumed_total,
            last_resort_present: lr_count > 0,
            oldest_available_created_at: oldest.map(|kp| kp.created_at),
        },
    ))
}

/// `DELETE /v1/keypackages/me` — удаляет все неconsumed `KeyPackages` (включая
/// last-resort) текущего устройства.
///
/// Нужно для согласования: старые билды оставляли на сервере `KeyPackages`,
/// чьи приватные бандлы устройство уже потеряло локально (после сброса
/// хранилища/сессии). Peer тогда claim'ит непригодный пакет и не может войти
/// (`NoMatchingKeyPackage`). Клиент один раз чистит свой пул и публикует новый,
/// локально-обеспеченный.
///
/// # Errors
///
/// - `500 ERR_INTERNAL` — ошибка БД.
pub async fn delete_my_keypackages(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
) -> Result<Response, AppError> {
    let res = KeyPackages::delete_many()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(ctx.device.id))
                .add(key_packages::Column::ConsumedAt.is_null()),
        )
        .exec(&state.db)
        .await?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &DeleteKeyPackagesResponse {
            deleted: res.rows_affected,
        },
    ))
}

/// Атомарно забирает один `KeyPackage` из пула target device.
///
/// Логика:
/// 1. Target device существует и не отозван.
/// 2. Target user существует и active.
/// 3. В транзакции `SERIALIZABLE`:
///    a. Ищем неconsumed, не expired, не last-resort, самый старый.
///    b. Если нет — fallback на last-resort (не помечаем consumed).
///    c. Если found и не last-resort — помечаем consumed.
/// 4. Коммит.
///
/// # Errors
///
/// - `404 ERR_NOT_FOUND` — target device или user не найден.
/// - `403 ERR_FORBIDDEN` — target user не active.
/// - `503 ERR_KEYPACKAGE_EXHAUSTED` — пул пуст и нет last-resort.
/// - `500 ERR_INTERNAL` — ошибка БД.
#[allow(clippy::too_many_lines)]
pub async fn claim_keypackage(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path((user_id, device_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let now = now_secs();

    // 1. Проверить target user
    let target_user = Users::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if target_user.status != "active" {
        return Err(AppError::Forbidden);
    }

    // 2. Проверить target device
    let target_device = Devices::find_by_id(device_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if target_device.user_id != user_id {
        return Err(AppError::NotFound);
    }

    if target_device.revoked_at.is_some() {
        return Err(AppError::DeviceRevoked);
    }

    // 3. Атомарный claim
    let txn = state
        .db
        .begin_with_config(
            Some(IsolationLevel::Serializable),
            Some(sea_orm::AccessMode::ReadWrite),
        )
        .await?;

    // Сначала обычные (не last-resort)
    let candidate = KeyPackages::find()
        .filter(
            Condition::all()
                .add(key_packages::Column::DeviceId.eq(device_id))
                .add(key_packages::Column::ConsumedAt.is_null())
                .add(key_packages::Column::IsLastResort.eq(false))
                .add(key_packages::Column::ExpiresAt.gt(now)),
        )
        // Newest first: a device's most recently published KeyPackages are the
        // ones whose private bundles it still holds locally. Claiming the oldest
        // could hand out a stale package whose bundle the device lost (after a
        // storage/session reset) -> the recipient fails to join with
        // `NoMatchingKeyPackage`.
        .order_by_desc(key_packages::Column::CreatedAt)
        .one(&txn)
        .await?;

    let pkg = if let Some(p) = candidate {
        p
    } else {
        // Last resort fallback
        KeyPackages::find()
            .filter(
                Condition::all()
                    .add(key_packages::Column::DeviceId.eq(device_id))
                    .add(key_packages::Column::IsLastResort.eq(true))
                    .add(key_packages::Column::ExpiresAt.gt(now)),
            )
            .one(&txn)
            .await?
            .ok_or(AppError::KeyPackageExhausted)?
    };

    // Если НЕ last-resort — помечаем consumed
    if !pkg.is_last_resort {
        let mut active: key_packages::ActiveModel = pkg.clone().into();
        active.consumed_at = Set(Some(now));
        active.consumed_by_user_id = Set(Some(ctx.user.id));
        active.update(&txn).await?;
    }

    txn.commit().await?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ClaimKeyPackageResponse {
            key_package_id: pkg.id,
            key_package: pkg.key_package,
        },
    ))
}

// ─── Helpers ───

/// Определяет, является ли ошибка БД нарушением UNIQUE constraint.
///
/// `SQLite`: код 2067 (`SQLITE_CONSTRAINT_UNIQUE`).
/// Postgres: код 23505.
fn is_unique_violation(err: &sea_orm::sqlx::error::Error) -> bool {
    use sea_orm::sqlx::error::Error as SqlxError;
    match err {
        SqlxError::Database(ref db_err) => {
            let code = db_err.code();
            code.as_deref() == Some("2067") || code.as_deref() == Some("23505")
        }
        _ => false,
    }
}

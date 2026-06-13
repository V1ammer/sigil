//! Endpoints для загрузки и скачивания зашифрованных вложений.
//!
//! ## Endpoints
//!
//! | Method | Path | Описание |
//! |--------|------|----------|
//! | POST   | `/v1/attachments` | Загрузка attachment'а |
//! | POST   | `/v1/attachments/:id/finalize` | Привязка к сообщению |
//! | GET    | `/v1/attachments/:id` | Скачивание (с поддержкой Range) |

use std::io::SeekFrom;
use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::attachments::{StorageBackend, StoredRef};
use crate::auth::middleware::CurrentAuth;
use crate::error::{decode_body, typed_response, AppError};
use crate::services::invite::now_secs;
use crate::state::AppState;

// ─── Request/Response types ───

/// Ответ на загрузку attachment'а.
#[derive(Serialize)]
pub struct UploadAttachmentResponse {
    pub attachment_id: Uuid,
    pub expires_at: i64,
}

/// Запрос на финализацию attachment'а.
#[derive(Deserialize)]
pub struct FinalizeAttachmentRequest {
    pub message_id: Uuid,
}

// ─── Constants ───

const DEFAULT_UNFINALIZED_LIMIT: u32 = 10;

// ─── Handlers ───

/// Загрузка attachment'а.
///
/// Body — raw bytes ciphertext'а.
///
/// Headers:
/// - `Content-Length` — обязателен, проверяется против `padded_size`.
/// - `X-Attachment-Padded-Size: <bytes>` — должен совпадать с Content-Length.
/// - `X-Attachment-Size-Bucket: <int>` — ведро размера.
///
/// # Errors
///
/// - `400` — невалидный размер, превышен лимит, много unfinalized.
/// - `500` — ошибка БД или ввода-вывода.
#[allow(clippy::too_many_lines)]
pub async fn upload_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    body: Bytes,
) -> Result<Response, AppError> {
    let now = now_secs();

    // 1. Content-Length
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| AppError::BadRequest("Content-Length header required".into()))?;

    if content_length > state.config.max_attachment_bytes {
        return Err(AppError::BadRequest(format!(
            "attachment too large: {content_length} > {}",
            state.config.max_attachment_bytes
        )));
    }

    // 2. X-Attachment-Padded-Size
    let padded_size = headers
        .get("x-attachment-padded-size")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| AppError::BadRequest("X-Attachment-Padded-Size header required".into()))?;

    #[allow(clippy::cast_possible_wrap)]
    if padded_size != content_length as i64 {
        return Err(AppError::BadRequest(
            "X-Attachment-Padded-Size must match Content-Length".into(),
        ));
    }

    // 3. X-Attachment-Size-Bucket
    let size_bucket = headers
        .get("x-attachment-size-bucket")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .ok_or_else(|| AppError::BadRequest("X-Attachment-Size-Bucket header required".into()))?;

    // 4. Лимит unfinalized
    let limit = if state.config.attachment_max_unfinalized_per_device > 0 {
        state.config.attachment_max_unfinalized_per_device
    } else {
        DEFAULT_UNFINALIZED_LIMIT
    };

    let count = messenger_entity::attachments::Entity::find()
        .filter(
            Condition::all()
                .add(messenger_entity::attachments::Column::UploaderDeviceId.eq(ctx.device.id))
                .add(messenger_entity::attachments::Column::MessageId.is_null()),
        )
        .count(&state.db)
        .await?;

    if count >= u64::from(limit) {
        return Err(AppError::BadRequest(
            "too many unfinalized attachments".into(),
        ));
    }

    // 5. Store
    let id = Uuid::now_v7();
    let storage_ref = state.storage.store(id, &body).await?;

    let (payload_ciphertext, storage_string) = match &storage_ref {
        StoredRef::Inline(data) => (Some(data.clone()), None),
        StoredRef::OnDisk { relative_path, .. } => {
            (None, Some(relative_path.to_string_lossy().to_string()))
        }
    };

    #[allow(clippy::cast_possible_wrap)]
    let expires_at = now + state.config.attachment_ttl_unfinalized_secs as i64;

    messenger_entity::attachments::ActiveModel {
        id: Set(id),
        message_id: Set(None),
        uploader_device_id: Set(ctx.device.id),
        payload_ciphertext: Set(payload_ciphertext),
        storage_ref: Set(storage_string),
        padded_size: Set(padded_size),
        size_bucket: Set(size_bucket),
        created_at: Set(now),
        expires_at: Set(expires_at),
    }
    .insert(&state.db)
    .await?;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &UploadAttachmentResponse { attachment_id: id, expires_at },
    ))
}

/// Финализация attachment'а — привязка к сообщению.
///
/// Транзакция: проверяет владение, членство в группе, обновляет attachment.
///
/// # Errors
///
/// - `404` — attachment или message не найден.
/// - `403` — attachment принадлежит другому устройству.
/// - `409` — attachment уже финализирован (`ERR_ATTACHMENT_NOT_FINALIZED`).
/// - `500` — ошибка БД.
pub async fn finalize_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: FinalizeAttachmentRequest = decode_body(&headers, &body)?;

    // Authorization reads run in autocommit (no read-then-write transaction).
    // A Serializable transaction that reads then writes triggers WAL
    // SQLITE_BUSY_SNAPSHOT (error 517) under concurrent writes, which
    // busy_timeout can't resolve — finalize then 500'd intermittently and the
    // attachment never bound to its message. The bind is a single atomic
    // conditional UPDATE instead.

    // 1. Attachment существует
    let attachment = messenger_entity::attachments::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Already finalized (e.g. our own retry, or a concurrent finalize that
    // already won) — idempotent success, not an error.
    if attachment.message_id.is_some() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    // 2. Uploader == ctx.device.id
    if attachment.uploader_device_id != ctx.device.id {
        return Err(AppError::Forbidden);
    }

    // 3. Message существует, sender == ctx
    let message = messenger_entity::mls_messages::Entity::find_by_id(req.message_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if message.sender_user_id != ctx.user.id || message.sender_device_id != ctx.device.id {
        return Err(AppError::Forbidden);
    }

    // 4. User — активный member группы
    let membership = messenger_entity::mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(messenger_entity::mls_group_members::Column::GroupId.eq(message.group_id))
                .add(messenger_entity::mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(messenger_entity::mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&state.db)
        .await?;

    if membership.is_none() {
        return Err(AppError::GroupMembershipRequired);
    }

    // 5. Атомарная привязка: единственный UPDATE с guard `message_id IS NULL`.
    //    GC проверяет message_id IS NULL вместе с expired, так что finalized — в безопасности.
    //    rows_affected == 0 означает «уже finalize'нут параллельно» — тоже успех.
    use sea_orm::sea_query::Expr;
    messenger_entity::attachments::Entity::update_many()
        .col_expr(
            messenger_entity::attachments::Column::MessageId,
            Expr::value(req.message_id),
        )
        .col_expr(
            messenger_entity::attachments::Column::ExpiresAt,
            Expr::value(0_i64),
        )
        .filter(messenger_entity::attachments::Column::Id.eq(id))
        .filter(messenger_entity::attachments::Column::MessageId.is_null())
        .exec(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Скачивание attachment'а с поддержкой Range-запросов.
///
/// Authorization:
/// - Finalized: проверяет членство в группе сообщения.
/// - Unfinalized: только uploader.
///
/// Range-запросы: `bytes=START-END` → 206 Partial Content.
/// Невалидный range → 416 Range Not Satisfiable.
/// Большие on-disk файлы (> threshold) — стриминг без загрузки в RAM.
///
/// # Errors
///
/// - `404` — attachment не найден.
/// - `403` — доступ запрещён.
/// - `416` — невалидный Range.
/// - `500` — ошибка БД или ввода-вывода.
///
/// # Panics
///
/// Паникует только при ошибке построения HTTP-ответа (что указывает на баг).
#[allow(clippy::too_many_lines)]
pub async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let attachment = messenger_entity::attachments::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // ── Авторизация ──
    if let Some(message_id) = attachment.message_id {
        // Finalized — проверяем членство в группе
        let message = messenger_entity::mls_messages::Entity::find_by_id(message_id)
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let membership = messenger_entity::mls_group_members::Entity::find()
            .filter(
                Condition::all()
                    .add(messenger_entity::mls_group_members::Column::GroupId
                        .eq(message.group_id))
                    .add(messenger_entity::mls_group_members::Column::UserId.eq(ctx.user.id))
                    .add(messenger_entity::mls_group_members::Column::LeftAtEpoch.is_null()),
            )
            .one(&state.db)
            .await?;
        if membership.is_none() {
            return Err(AppError::Forbidden);
        }
    } else if attachment.uploader_device_id != ctx.device.id {
        // Unfinalized — только uploader
        return Err(AppError::Forbidden);
    }

    // ── Восстанавливаем StoredRef ──
    #[allow(clippy::cast_sign_loss)]
    let total_size = attachment.padded_size as u64;
    let is_on_disk = attachment.storage_ref.is_some();

    // ── Range header ──
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| parse_range_header(s, total_size));

    if let Some((start, end)) = range {
        if start >= total_size || end >= total_size || start > end {
            return Ok(
                Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{total_size}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            );
        }

        let chunk_size = end - start + 1;
        let use_streaming = is_on_disk
            && chunk_size >= state.config.attachment_stream_threshold_bytes;

        let body = if use_streaming {
            let path = resolve_disk_path(&state.storage, attachment.storage_ref.as_deref())?;
            let file = tokio::fs::File::open(&path).await
                .map_err(|e| AppError::Internal(anyhow::anyhow!(
                    "cannot open {}: {e}", path.display()
                )))?;
            let mut reader = tokio::io::BufReader::new(file);
            reader.seek(SeekFrom::Start(start)).await
                .map_err(|e| AppError::Internal(anyhow::anyhow!(
                    "cannot seek {}: {e}", path.display()
                )))?;
            let limited = reader.take(chunk_size);
            axum::body::Body::from_stream(ReaderStream::new(limited))
        } else {
            let stored_ref = reconstruct_ref(&attachment);
            let data = state.storage.read_range(&stored_ref, start, end).await?;
            axum::body::Body::from(data)
        };

        Ok(
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CONTENT_LENGTH, chunk_size.to_string())
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total_size}"),
                )
                .header(header::ACCEPT_RANGES, "bytes")
                .body(body)
                .unwrap(),
        )
    } else {
        // Без Range
        let use_streaming = is_on_disk
            && total_size >= state.config.attachment_stream_threshold_bytes;

        let body = if use_streaming {
            let path = resolve_disk_path(&state.storage, attachment.storage_ref.as_deref())?;
            let file = tokio::fs::File::open(&path).await
                .map_err(|e| AppError::Internal(anyhow::anyhow!(
                    "cannot open {}: {e}", path.display()
                )))?;
            let reader = tokio::io::BufReader::new(file);
            axum::body::Body::from_stream(ReaderStream::new(reader))
        } else {
            let stored_ref = reconstruct_ref(&attachment);
            let data = state.storage.read(&stored_ref).await?;
            axum::body::Body::from(data)
        };

        Ok(
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CONTENT_LENGTH, total_size.to_string())
                .header(header::ACCEPT_RANGES, "bytes")
                .body(body)
                .unwrap(),
        )
    }
}

// ─── Helpers ───

/// Парсит Range header.
///
/// Поддерживает только `bytes=START-END` или `bytes=START-` (весь файл с START).
/// Возвращает `(start, end)` c end, скорректированным под размер файла.
fn parse_range_header(range_str: &str, total_size: u64) -> Option<(u64, u64)> {
    let range_str = range_str.trim();
    if !range_str.starts_with("bytes=") {
        return None;
    }
    let range_val = &range_str[6..];
    let (start_str, end_str) = range_val.split_once('-')?;
    let start: u64 = start_str.parse().ok()?;
    let end: u64 = if end_str.is_empty() {
        total_size.saturating_sub(1)
    } else {
        end_str.parse().ok()?
    };
    Some((start, end))
}

/// Восстанавливает `StoredRef` из модели attachment'а.
#[allow(clippy::cast_sign_loss)]
fn reconstruct_ref(attachment: &messenger_entity::attachments::Model) -> StoredRef {
    if let Some(ref data) = attachment.payload_ciphertext {
        StoredRef::Inline(data.clone())
    } else if let Some(ref path) = attachment.storage_ref {
        StoredRef::OnDisk {
            relative_path: PathBuf::from(path),
            size: attachment.padded_size as u64,
        }
    } else {
        // Этого не должно быть; на всякий случай возвращаем пустой Inline
        StoredRef::Inline(vec![])
    }
}

/// Разрешает полный путь к файлу на диске.
#[allow(clippy::match_wildcard_for_single_variants)]
fn resolve_disk_path(storage: &StorageBackend, storage_ref: Option<&str>) -> Result<PathBuf, AppError> {
    let rel = storage_ref.ok_or_else(|| AppError::Internal(anyhow::anyhow!(
        "attachment has no storage_ref"
    )))?;
    match storage {
        StorageBackend::FileSystem { root, .. } => Ok(root.join(rel)),
        StorageBackend::InDatabase => Err(AppError::Internal(anyhow::anyhow!(
            "storage backend is not FileSystem"
        ))),
    }
}

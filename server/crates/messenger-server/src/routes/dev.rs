//! Dev-only endpoints для упрощения разработки и тестирования.
//!
//! Эти эндпоинты **не** предназначены для production и отключены
//! в релизных сборках.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use base64::Engine;
use rand::RngCore;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::Serialize;
use uuid::Uuid;

use crate::error::{typed_response, AppError};
use crate::services::invite::now_secs;
use crate::state::AppState;
use messenger_entity::device_provisioning_requests;
use messenger_entity::device_provisioning_requests::Entity as ProvRequests;
use messenger_entity::invitation_tokens;

#[derive(Serialize)]
pub struct DevTokenResponse {
    pub id: Uuid,
    /// Сырой токен (32 байта).
    pub token: String,
    /// Токен в base64url-no-pad для прямого копирования.
    pub token_display: String,
    pub expires_at: i64,
}

/// `GET /v1/dev/create-token` — создать инвайт-токен (без auth).
///
/// Доступен только в debug-сборках. Создаёт токен с ролью `user`,
/// 10 использованиями, TTL 365 дней.
///
/// # Errors
///
/// - `404` — эндпоинт недоступен в release-сборках.
/// - `500` — внутренняя ошибка.
pub async fn create_dev_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, crate::error::AppError> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (state, headers);
        return Err(crate::error::AppError::NotFound);
    }

    #[cfg(debug_assertions)]
    {
        let mut token_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut token_bytes);

        let token_hex = hex::encode(token_bytes);
        let token_hash = blake3::hash(token_hex.as_bytes()).as_bytes().to_vec();
        tracing::info!(token = %token_hex, token_hash = %hex::encode(&token_hash), "created dev token");

        let now = now_secs();
        let expires_at = now + 365 * 24 * 3600;
        let token_id = Uuid::now_v7();

        invitation_tokens::ActiveModel {
            id: sea_orm::Set(token_id),
            token_hash: sea_orm::Set(token_hash),
            created_by_user_id: sea_orm::Set(None),
            role_to_grant: sea_orm::Set("user".to_string()),
            max_uses: sea_orm::Set(10),
            uses_count: sea_orm::Set(0),
            expires_at: sea_orm::Set(expires_at),
            revoked_at: sea_orm::Set(None),
            created_at: sea_orm::Set(now),
        }
        .insert(&state.db)
        .await?;

        let resp = DevTokenResponse {
            id: token_id,
            token: token_hex.clone(),
            token_display: token_hex,
            expires_at,
        };

        Ok(typed_response(&headers, StatusCode::CREATED, &resp))
    }
}

/// `GET /v1/dev/force-bootstrap/:id` — dev-only: consume a provisioning request
/// without signature verification. Only available in debug builds.
#[allow(clippy::module_name_repetitions)]
pub async fn force_consume_provisioning(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (state, id, headers);
        return Err(AppError::NotFound);
    }

    #[cfg(debug_assertions)]
    {
        let now = now_secs();
        let provisioning = ProvRequests::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;

        if provisioning.status != "approved" {
            return Err(AppError::ProvisioningExpired);
        }
        if provisioning.expires_at <= now {
            return Err(AppError::ProvisioningExpired);
        }

        let bootstrap_blob = provisioning
            .encrypted_bootstrap_blob
            .clone()
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("approved without blob")))?;

        let new_device_id = provisioning
            .new_device_id
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("approved without device_id")))?;

        // Mark as consumed
        let mut active: device_provisioning_requests::ActiveModel = provisioning.into();
        active.status = Set("consumed".to_string());
        active.encrypted_bootstrap_blob = Set(None);
        active.update(&state.db).await?;

        #[derive(Serialize)]
        struct ForceBootstrapResponse {
            new_device_id: Uuid,
            #[serde(with = "serde_bytes")]
            encrypted_bootstrap_blob: Vec<u8>,
        }

        Ok(typed_response(
            &headers,
            StatusCode::OK,
            &ForceBootstrapResponse {
                new_device_id,
                encrypted_bootstrap_blob: bootstrap_blob,
            },
        ))
    }
}

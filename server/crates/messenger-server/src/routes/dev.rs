//! Dev-only endpoints для упрощения разработки и тестирования.
//!
//! Эти эндпоинты **не** предназначены для production и отключены
//! в релизных сборках.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use base64::Engine;
use rand::RngCore;
use sea_orm::ActiveModelTrait;
use serde::Serialize;
use uuid::Uuid;

use crate::error::typed_response;
use crate::services::invite::now_secs;
use crate::state::AppState;
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

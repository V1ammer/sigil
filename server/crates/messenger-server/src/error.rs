use std::convert::Infallible;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Общий тип ошибки приложения. Маппится в HTTP-ответ с msgpack-телом.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error")]
    Db(#[source] sea_orm::DbErr),

    #[error("config error: {0}")]
    Config(String),

    #[error("not found")]
    NotFound,

    #[error("conflict")]
    Conflict,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),

    // ─── доменные ошибки, машинно-читаемые коды ───
    #[error("ERR_INVITE_INVALID")]
    InviteInvalid,
    #[error("ERR_INVITE_EXPIRED")]
    InviteExpired,
    #[error("ERR_INVITE_EXHAUSTED")]
    InviteExhausted,
    #[error("ERR_USERNAME_TAKEN")]
    UsernameTaken,
    #[error("ERR_IDENTITY_NOT_FOUND")]
    IdentityNotFound,
    #[error("ERR_DEVICE_REVOKED")]
    DeviceRevoked,
    #[error("ERR_SIGNATURE_INVALID")]
    SignatureInvalid,
    #[error("ERR_TIMESTAMP_OUT_OF_WINDOW")]
    TimestampOutOfWindow,
    #[error("ERR_NONCE_REPLAY")]
    NonceReplay,
    #[error("ERR_KEYPACKAGE_EXHAUSTED")]
    KeyPackageExhausted,
    #[error("ERR_EPOCH_OUTDATED")]
    EpochOutdated,
    #[error("ERR_GROUP_MEMBERSHIP_REQUIRED")]
    GroupMembershipRequired,
    #[error("ERR_RATE_LIMITED")]
    RateLimited,
    #[error("ERR_PROVISIONING_EXPIRED")]
    ProvisioningExpired,
    #[error("ERR_ATTACHMENT_NOT_FINALIZED")]
    AttachmentNotFinalized,
    #[error("ERR_BOOTSTRAP_ALREADY_DONE")]
    BootstrapAlreadyDone,
}

/// Тело ошибки в msgpack-формате.
#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InviteInvalid
            | Self::InviteExpired
            | Self::InviteExhausted
            | Self::ProvisioningExpired => StatusCode::GONE,
            Self::UsernameTaken
            | Self::Conflict
            | Self::EpochOutdated
            | Self::AttachmentNotFinalized
            | Self::BootstrapAlreadyDone => StatusCode::CONFLICT,
            Self::IdentityNotFound | Self::NotFound => StatusCode::NOT_FOUND,
            Self::DeviceRevoked | Self::Forbidden | Self::GroupMembershipRequired => {
                StatusCode::FORBIDDEN
            }
            Self::SignatureInvalid
            | Self::TimestampOutOfWindow
            | Self::NonceReplay
            | Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::KeyPackageExhausted => StatusCode::SERVICE_UNAVAILABLE,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) | Self::Db(_) | Self::Config(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::Db(_) | Self::Internal(_) | Self::Config(_) => "ERR_INTERNAL",
            Self::NotFound => "ERR_NOT_FOUND",
            Self::Conflict => "ERR_CONFLICT",
            Self::Unauthorized => "ERR_UNAUTHORIZED",
            Self::Forbidden => "ERR_FORBIDDEN",
            Self::BadRequest(_) => "ERR_BAD_REQUEST",
            Self::InviteInvalid => "ERR_INVITE_INVALID",
            Self::InviteExpired => "ERR_INVITE_EXPIRED",
            Self::InviteExhausted => "ERR_INVITE_EXHAUSTED",
            Self::UsernameTaken => "ERR_USERNAME_TAKEN",
            Self::IdentityNotFound => "ERR_IDENTITY_NOT_FOUND",
            Self::DeviceRevoked => "ERR_DEVICE_REVOKED",
            Self::SignatureInvalid => "ERR_SIGNATURE_INVALID",
            Self::TimestampOutOfWindow => "ERR_TIMESTAMP_OUT_OF_WINDOW",
            Self::NonceReplay => "ERR_NONCE_REPLAY",
            Self::KeyPackageExhausted => "ERR_KEYPACKAGE_EXHAUSTED",
            Self::EpochOutdated => "ERR_EPOCH_OUTDATED",
            Self::GroupMembershipRequired => "ERR_GROUP_MEMBERSHIP_REQUIRED",
            Self::RateLimited => "ERR_RATE_LIMITED",
            Self::ProvisioningExpired => "ERR_PROVISIONING_EXPIRED",
            Self::AttachmentNotFinalized => "ERR_ATTACHMENT_NOT_FINALIZED",
            Self::BootstrapAlreadyDone => "ERR_BOOTSTRAP_ALREADY_DONE",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        // Логируем 5xx детально
        if status.as_u16() >= 500 {
            tracing::error!(error = ?self, "internal server error");
        }

        let body = ErrorBody {
            code: self.error_code(),
            details: match &self {
                Self::BadRequest(reason) => Some(serde_json::json!({ "reason": reason })),
                _ => None,
            },
        };

        let bytes = rmp_serde::to_vec_named(&body).unwrap_or_default();
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/msgpack")
            .body(bytes.into())
            .unwrap()
    }
}

// Позволяет использовать `?` с `sea_orm::DbErr`.
impl From<sea_orm::DbErr> for AppError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::Db(e)
    }
}

// Позволяет использовать `?` в хендлерах с infallible.
impl From<Infallible> for AppError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

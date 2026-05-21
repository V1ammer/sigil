use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Serialize;

use crate::error::typed_response;
use crate::state::AppState;

#[derive(Serialize)]
pub struct ServerInfo {
    #[serde(with = "serde_bytes")]
    pub server_identity_public_key: Vec<u8>,
    pub mls_ciphersuite: u16,
    pub schema_version: i32,
    pub username_hash_version: i32,
    pub supports_provisioning: bool,
}

/// `GET /v1/server/info` — публичная информация о сервере.
///
/// Не требует аутентификации. Поддерживает JSON и `MessagePack`
/// в зависимости от `Accept` header.
pub async fn info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl axum::response::IntoResponse {
    let body = ServerInfo {
        server_identity_public_key: state
            .server_identity
            .signing_public_key
            .to_bytes()
            .to_vec(),
        mls_ciphersuite: state.server_identity.mls_ciphersuite,
        schema_version: 1,
        username_hash_version: state.server_identity.username_hash_version,
        supports_provisioning: true,
    };

    typed_response(&headers, StatusCode::OK, &body)
}

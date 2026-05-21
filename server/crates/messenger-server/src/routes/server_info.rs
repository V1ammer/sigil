use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
struct ServerInfoResponse {
    server_identity_public_key: Vec<u8>,
    mls_ciphersuite: u16,
    schema_version: i32,
    username_hash_version: i32,
    supports_provisioning: bool,
}

/// `GET /v1/server/info` — публичная информация о сервере.
pub async fn info(State(state): State<AppState>) -> impl IntoResponse {
    let body = ServerInfoResponse {
        server_identity_public_key: state.server_identity.public_key.clone(),
        mls_ciphersuite: 0x0001,
        schema_version: 1,
        username_hash_version: 1,
        supports_provisioning: true,
    };

    let bytes = rmp_serde::to_vec_named(&body).unwrap_or_default();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/msgpack")],
        bytes,
    )
}

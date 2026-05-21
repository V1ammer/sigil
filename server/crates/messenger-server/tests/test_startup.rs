use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::error::AppError;
use messenger_server::routes::build_router;
use messenger_server::state::{AppState, NonceCache, ServerIdentity};
use messenger_migration::MigratorTrait;
use sea_orm::ConnectionTrait;
use sea_orm::Database;

/// Вспомогательная функция: создаёт `AppState` с in-memory `SQLite`.
async fn test_state() -> AppState {
    let db = Database::connect("sqlite::memory:").await.unwrap();

    // Применяем миграции
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        ..AppConfig::default()
    };

    AppState {
        db,
        config: std::sync::Arc::new(config),
        nonce_cache: std::sync::Arc::new(NonceCache::new(100)),
        server_identity: std::sync::Arc::new(ServerIdentity::placeholder()),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    }
}

#[tokio::test]
async fn test_server_starts_and_serves_health() {
    let state = test_state().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Даём серверу время запуститься
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn test_server_info_endpoint() {
    let state = test_state().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/server/info"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "application/msgpack");
}

#[tokio::test]
async fn test_migrations_run_on_startup() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_server::db::run_migrations(&db).await.unwrap();

    // Проверяем что таблицы созданы
    let tables = db
        .query_all(sea_orm::Statement::from_string(
            db.get_database_backend(),
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name".to_owned(),
        ))
        .await
        .unwrap();

    let table_names: Vec<String> = tables
        .iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect();

    assert!(table_names.contains(&"server_config".to_string()));
    assert!(table_names.contains(&"users".to_string()));
}

#[test]
fn test_error_mapping_invite_invalid() {
    let err = AppError::InviteInvalid;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[test]
fn test_error_mapping_invite_expired() {
    let err = AppError::InviteExpired;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[test]
fn test_error_mapping_invite_exhausted() {
    let err = AppError::InviteExhausted;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[test]
fn test_error_mapping_username_taken() {
    let err = AppError::UsernameTaken;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[test]
fn test_error_mapping_identity_not_found() {
    let err = AppError::IdentityNotFound;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_error_mapping_not_found() {
    let err = AppError::NotFound;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_error_mapping_device_revoked() {
    let err = AppError::DeviceRevoked;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_error_mapping_signature_invalid() {
    let err = AppError::SignatureInvalid;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_error_mapping_timestamp_out_of_window() {
    let err = AppError::TimestampOutOfWindow;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_error_mapping_nonce_replay() {
    let err = AppError::NonceReplay;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_error_mapping_unauthorized() {
    let err = AppError::Unauthorized;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_error_mapping_forbidden() {
    let err = AppError::Forbidden;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_error_mapping_group_membership_required() {
    let err = AppError::GroupMembershipRequired;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_error_mapping_rate_limited() {
    let err = AppError::RateLimited;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn test_error_mapping_keypackage_exhausted() {
    let err = AppError::KeyPackageExhausted;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn test_error_mapping_epoch_outdated() {
    let err = AppError::EpochOutdated;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[test]
fn test_error_mapping_provisioning_expired() {
    let err = AppError::ProvisioningExpired;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[test]
fn test_error_mapping_attachment_not_finalized() {
    let err = AppError::AttachmentNotFinalized;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[test]
fn test_error_mapping_bootstrap_already_done() {
    let err = AppError::BootstrapAlreadyDone;
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[test]
fn test_error_mapping_bad_request() {
    let err = AppError::BadRequest("missing field".to_string());
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_error_mapping_internal() {
    let err = AppError::Internal(anyhow::anyhow!("db failure"));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_error_mapping_db() {
    let err = AppError::Db(sea_orm::DbErr::Custom("test".to_string()));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_nonce_cache() {
    let cache = NonceCache::new(10);
    let nonce = b"test-nonce";

    // Первый раз — fresh
    assert!(!cache.check_and_insert(nonce));

    // Второй раз — replay
    assert!(cache.check_and_insert(nonce));
}

#[test]
fn test_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.max_request_body_bytes, 16 * 1024 * 1024);
    assert_eq!(config.clock_skew_tolerance_secs, 60);
    assert!(config.database_url.is_empty());
}

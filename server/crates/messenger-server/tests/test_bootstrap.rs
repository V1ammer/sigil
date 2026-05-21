#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use messenger_entity::prelude::InvitationTokens;
use messenger_entity::server_config;
use messenger_server::bootstrap;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::identity::ServerIdentity;
use messenger_server::routes::build_router;
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;
use sea_orm::{Database, EntityTrait};
use serde::Deserialize;

/// Создаёт `AppState` с in-memory `SQLite` и реальным bootstrap'ом.
async fn setup_bootstrapped_state() -> AppState {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let identity = bootstrap::load_or_init(&db).await.unwrap();

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
        server_identity: std::sync::Arc::new(identity),
    }
}

/// Создаёт `AppState` с in-memory `SQLite` и заглушкой identity (без bootstrap).
#[allow(dead_code)]
async fn setup_placeholder_state() -> AppState {
    let db = Database::connect("sqlite::memory:").await.unwrap();
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
    }
}

#[derive(Deserialize)]
#[expect(dead_code)]
struct ServerInfoMsgpack {
    server_identity_public_key: Vec<u8>,
    mls_ciphersuite: u16,
    schema_version: i32,
    username_hash_version: i32,
    supports_provisioning: bool,
}

// ─── bootstrap unit tests ───

#[tokio::test]
async fn test_first_run_generates_identity_and_token() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let identity = bootstrap::load_or_init(&db).await.unwrap();

    // server_config записан
    let cfg = server_config::Entity::find_by_id(1)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(cfg.bootstrap_token_issued);
    assert_eq!(cfg.username_hash_version, 1);

    // Invitation token создан
    let tokens = InvitationTokens::find().all(&db).await.unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].role_to_grant, "admin");
    assert_eq!(tokens[0].max_uses, 1);

    // Identity не нулевой
    assert_ne!(identity.signing_public_key.to_bytes(), [0u8; 32]);
}

#[tokio::test]
async fn test_second_load_reuses_identity() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let id1 = bootstrap::load_or_init(&db).await.unwrap();
    let id2 = bootstrap::load_or_init(&db).await.unwrap();

    assert_eq!(
        id1.signing_public_key.to_bytes(),
        id2.signing_public_key.to_bytes()
    );
    assert_eq!(
        id1.username_blind_index_key,
        id2.username_blind_index_key
    );

    // Токен не выдан повторно
    let tokens = InvitationTokens::find().all(&db).await.unwrap();
    assert_eq!(tokens.len(), 1);
}

#[tokio::test]
async fn test_blind_index_canonical() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let identity = bootstrap::load_or_init(&db).await.unwrap();

    let a = identity.blind_index("Alice");
    let b = identity.blind_index("alice");
    let c = identity.blind_index(" alice ");
    assert_eq!(a, b);
    assert_eq!(a, c);

    // Разные имена дают разные индексы
    assert_ne!(a, identity.blind_index("bob"));
}

// ─── integration tests ───

#[tokio::test]
async fn test_server_info_endpoint_msgpack() {
    let state = setup_bootstrapped_state().await;
    let app = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/server/info"))
        .header("Accept", "application/msgpack")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()["content-type"],
        "application/msgpack"
    );

    let bytes = resp.bytes().await.unwrap();
    let info: ServerInfoMsgpack = rmp_serde::decode::from_slice(&bytes).unwrap();
    assert_eq!(info.server_identity_public_key.len(), 32);
    assert_eq!(info.mls_ciphersuite, 0x0001);
    assert_eq!(info.schema_version, 1);
    assert!(info.supports_provisioning);
}

#[tokio::test]
async fn test_server_info_json_via_accept_header() {
    let state = setup_bootstrapped_state().await;
    let app = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/server/info"))
        .header("Accept", "application/json")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "application/json");

    let info: serde_json::Value = resp.json().await.unwrap();
    // `serde_bytes` в JSON сериализует Vec<u8> как массив чисел
    assert_eq!(
        info["server_identity_public_key"]
            .as_array()
            .unwrap()
            .len(),
        32
    );
    assert_eq!(info["mls_ciphersuite"].as_u64().unwrap(), 0x0001);
    assert!(info["supports_provisioning"].as_bool().unwrap());
}

#[tokio::test]
async fn test_server_info_default_to_msgpack() {
    // Без Accept header должен вернуть msgpack
    let state = setup_bootstrapped_state().await;
    let app = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
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
    assert_eq!(
        resp.headers()["content-type"],
        "application/msgpack"
    );
}

#[tokio::test]
async fn test_server_info_placeholder_no_crash() {
    let state = setup_placeholder_state().await;
    let app = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
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
    let bytes = resp.bytes().await.unwrap();
    let info: ServerInfoMsgpack = rmp_serde::decode::from_slice(&bytes).unwrap();
    // Placeholder — ключ нулевой, но 32 байта
    assert_eq!(info.server_identity_public_key.len(), 32);
    assert_eq!(info.mls_ciphersuite, 0);
}

#[tokio::test]
async fn test_health_endpoint_still_works() {
    let state = setup_bootstrapped_state().await;
    let app = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

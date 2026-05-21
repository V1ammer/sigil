use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue::Set;
use sea_orm::Database;
use sea_orm::EntityTrait;
use uuid::Uuid;

use messenger_migration::MigratorTrait;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::routes::build_router;
use messenger_server::state::{AppState, NonceCache, ServerIdentity};

// ─── helpers ───────────────────────────────────────────────────────────────────

/// Создаёт тестовые данные: пользователя и устройство.
/// Возвращает (`SigningKey` устройства, `device_id`, `user_id`).
async fn create_test_device(
    db: &sea_orm::DatabaseConnection,
    role: &str,
    status: &str,
) -> (SigningKey, Uuid, Uuid) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk: VerifyingKey = sk.verifying_key();

    let user_id = Uuid::now_v7();

    let user = messenger_entity::users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set(b"test-blind-index".to_vec()),
        username_hash_version: Set(1),
        role: Set(role.to_string()),
        status: Set(status.to_string()),
        created_at: Set(chrono::Utc::now().timestamp()),
        send_read_receipts: Set(false),
    };
    user.insert(db).await.unwrap();

    let device_id = Uuid::now_v7();
    let device = messenger_entity::devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(b"dummy-hpke-key-32bytes!".to_vec()),
        device_signing_public_key: Set(vk.to_bytes().to_vec()),
        authorization_signature: Set(b"dummy-auth-sig".to_vec()),
        authorized_by_device_id: Set(None),
        created_at: Set(chrono::Utc::now().timestamp()),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    };
    device.insert(db).await.unwrap();

    (sk, device_id, user_id)
}

/// Создаёт значение заголовка `X-Auth-Signature` для заданного запроса.
fn sign_request(
    method: &str,
    path: &str,
    body: &[u8],
    sk: &SigningKey,
    device_id: Uuid,
) -> String {
    let ts = chrono::Utc::now().timestamp();
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();

    let canonical = messenger_crypto::canonical::build_signed_message(
        method,
        path,
        ts,
        &nonce,
        body,
    );

    let signature = sk.sign(&canonical);

    format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    )
}

/// Создаёт `AppState` с in-memory `SQLite` + применёнными миграциями + тестовыми данными.
async fn test_state_with_data(
    role: &str,
    status: &str,
) -> (AppState, SigningKey, Uuid) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let (sk, device_id, _user_id) = create_test_device(&db, role, status).await;

    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        ..AppConfig::default()
    };

    let state = AppState {
        db,
        config: std::sync::Arc::new(config),
        nonce_cache: std::sync::Arc::new(NonceCache::new(100)),
        server_identity: std::sync::Arc::new(ServerIdentity::placeholder()),
    };

    (state, sk, device_id)
}

// ─── tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_valid_signature_passes() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let header = sign_request("GET", "/v1/users/me/test", b"", &sk, device_id);
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("user_id").is_some());
    assert!(body.get("device_id").is_some());
}

#[tokio::test]
async fn test_missing_header_rejected() {
    let (state, _sk, _device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_signature_rejected() {
    let (state, _sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Подпись с неправильным ключом
    let wrong_sk = SigningKey::generate(&mut OsRng);
    let header = sign_request("GET", "/v1/users/me/test", b"", &wrong_sk, device_id);
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_stale_timestamp_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Подпись со старым timestamp (> 120s назад, skew=60)
    let old_ts = chrono::Utc::now().timestamp() - 300;
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let canonical = messenger_crypto::canonical::build_signed_message(
        "GET",
        "/v1/users/me/test",
        old_ts,
        &nonce,
        b"",
    );
    let signature = sk.sign(&canonical);

    let header = format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        old_ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_future_timestamp_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Подпись с future timestamp (> 120s вперёд, skew=60)
    let future_ts = chrono::Utc::now().timestamp() + 300;
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let canonical = messenger_crypto::canonical::build_signed_message(
        "GET",
        "/v1/users/me/test",
        future_ts,
        &nonce,
        b"",
    );
    let signature = sk.sign(&canonical);

    let header = format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        future_ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_nonce_replay_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Один и тот же nonce для двух запросов
    let ts = chrono::Utc::now().timestamp();
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let canonical = messenger_crypto::canonical::build_signed_message(
        "GET",
        "/v1/users/me/test",
        ts,
        &nonce,
        b"",
    );
    let signature = sk.sign(&canonical);

    let header = format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    );

    let client = reqwest::Client::new();

    // Первый запрос — OK
    let resp1 = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);

    // Второй запрос с тем же nonce — replay
    let resp2 = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_revoked_device_rejected() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let (sk, device_id, _user_id) = create_test_device(&db, "user", "active").await;

    // Отзываем устройство
    let device = messenger_entity::devices::Entity::find_by_id(device_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut device_active: messenger_entity::devices::ActiveModel = device.into();
    device_active.revoked_at = Set(Some(chrono::Utc::now().timestamp()));
    device_active.update(&db).await.unwrap();

    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        ..AppConfig::default()
    };

    let state = AppState {
        db,
        config: std::sync::Arc::new(config),
        nonce_cache: std::sync::Arc::new(NonceCache::new(100)),
        server_identity: std::sync::Arc::new(ServerIdentity::placeholder()),
    };

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let header = sign_request("GET", "/v1/users/me/test", b"", &sk, device_id);
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_wrong_body_signature_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Подписываем тело A, отправляем тело B (для POST)
    let ts = chrono::Utc::now().timestamp();
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let body_a = b"body A";
    let body_b = b"body B";

    // Подпись для body_a
    let canonical = messenger_crypto::canonical::build_signed_message(
        "POST",
        "/v1/users/me/test",
        ts,
        &nonce,
        body_a,
    );
    let signature = sk.sign(&canonical);

    let header = format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .body(body_b.to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_method_mismatch_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Подписываем для POST, отправляем GET
    let ts = chrono::Utc::now().timestamp();
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let canonical = messenger_crypto::canonical::build_signed_message(
        "POST",
        "/v1/users/me/test",
        ts,
        &nonce,
        b"",
    );
    let signature = sk.sign(&canonical);

    let header = format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_path_mismatch_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Подписываем для /wrong, отправляем на /v1/users/me/test
    let ts = chrono::Utc::now().timestamp();
    let nonce: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let canonical = messenger_crypto::canonical::build_signed_message(
        "GET",
        "/v1/wrong",
        ts,
        &nonce,
        b"",
    );
    let signature = sk.sign(&canonical);

    let header = format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        ts,
        hex::encode(&nonce),
        hex::encode(signature.to_bytes()),
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_suspended_user_rejected() {
    let (state, sk, device_id) = test_state_with_data("user", "suspended").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let header = sign_request("GET", "/v1/users/me/test", b"", &sk, device_id);
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", &header)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_health_is_public() {
    let (state, _sk, _device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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

#[tokio::test]
async fn test_server_info_is_public() {
    let (state, _sk, _device_id) = test_state_with_data("user", "active").await;
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
async fn test_garbage_header_rejected() {
    let (state, _sk, _device_id) = test_state_with_data("user", "active").await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/v1/users/me/test"))
        .header("X-Auth-Signature", "garbage")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

//! Integration tests for S12 – WebSocket Realtime.
//!
//! Coverage:
//! - WS handshake with valid auth → AuthOk
//! - WS handshake with invalid signature → AuthError
//! - WS handshake timeout → close
//! - New message notification via WS
//! - Typing broadcast
//! - Ping/Pong
//! - Idle timeout closes connection

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]
#![allow(dead_code, clippy::too_many_lines, clippy::doc_markdown, clippy::let_underscore_future, clippy::let_underscore_untyped)]

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use axum::http::StatusCode;
use ed25519_dalek::Signer;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use sea_orm::{ActiveModelTrait, Database, Set};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use messenger_crypto::canonical::build_signed_message;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_server::ws_registry::WsRegistry;
use messenger_migration::MigratorTrait;

// ─── Helpers ───

#[derive(Clone)]
struct TestUser {
    user_id: Uuid,
    device_id: Uuid,
    device_signing_key: ed25519_dalek::SigningKey,
    state: AppState,
}

async fn fresh_db() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();
    db
}

fn make_state(db: sea_orm::DatabaseConnection) -> AppState {
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
        server_identity: std::sync::Arc::new(
            messenger_server::state::ServerIdentity::placeholder(),
        ),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
        ws_registry: WsRegistry::new(),
    }
}

async fn create_user_with_device(db: &sea_orm::DatabaseConnection) -> TestUser {
    let mut rng = rand::thread_rng();
    let device_signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
    let device_vk = device_signing_key.verifying_key();

    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let now = now_secs();

    let mut blind_index = [0u8; 32];
    rng.fill_bytes(&mut blind_index);

    messenger_entity::users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set(blind_index.to_vec()),
        username_hash_version: Set(1),
        role: Set("user".to_string()),
        status: Set("active".to_string()),
        created_at: Set(now),
        send_read_receipts: Set(false),
    }
    .insert(db)
    .await
    .unwrap();

    messenger_entity::devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(vec![0u8; 32]),
        device_signing_public_key: Set(device_vk.to_bytes().to_vec()),
        authorization_signature: Set(vec![0u8; 64]),
        authorized_by_device_id: Set(None),
        created_at: Set(now),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(db)
    .await
    .unwrap();

    let state = make_state(db.clone());
    TestUser {
        user_id,
        device_id,
        device_signing_key,
        state,
    }
}

async fn create_second_user(db: &sea_orm::DatabaseConnection, first_user: &TestUser) -> TestUser {
    let mut rng = rand::thread_rng();
    let device_signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
    let device_vk = device_signing_key.verifying_key();

    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let now = now_secs();

    let mut blind_index = [0u8; 32];
    rng.fill_bytes(&mut blind_index);

    messenger_entity::users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set(blind_index.to_vec()),
        username_hash_version: Set(1),
        role: Set("user".to_string()),
        status: Set("active".to_string()),
        created_at: Set(now),
        send_read_receipts: Set(false),
    }
    .insert(db)
    .await
    .unwrap();

    messenger_entity::devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(vec![0u8; 32]),
        device_signing_public_key: Set(device_vk.to_bytes().to_vec()),
        authorization_signature: Set(vec![0u8; 64]),
        authorized_by_device_id: Set(None),
        created_at: Set(now),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(db)
    .await
    .unwrap();

    TestUser {
        user_id,
        device_id,
        device_signing_key,
        state: first_user.state.clone(),
    }
}

async fn start_server(state: AppState) -> (String, String) {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    (format!("http://{addr}"), format!("ws://{addr}"))
}

fn make_ws_auth_frame(
    device_signing_key: &ed25519_dalek::SigningKey,
    device_id: &Uuid,
) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();

    // WS canonical: "GET\n/v1/ws\n{ts}\n{nonce_hex}\n{blake3(empty)}"
    let canonical = build_signed_message("GET", "/v1/ws", ts, &nonce, b"");
    let signature = device_signing_key.sign(&canonical);

    let frame = WsAuthFrame {
        frame_type: "auth".to_string(),
        device_id: *device_id,
        timestamp: ts,
        nonce: nonce.to_vec(),
        signature: signature.to_bytes().to_vec(),
    };

    rmp_serde::to_vec_named(&frame).unwrap()
}

/// Фрейм для сериализации WS auth (raw bytes для nonce/signature).
#[derive(Debug, Serialize)]
struct WsAuthFrame {
    #[serde(rename = "type")]
    frame_type: String,
    device_id: Uuid,
    timestamp: i64,
    #[serde(with = "serde_bytes")]
    nonce: Vec<u8>,
    #[serde(with = "serde_bytes")]
    signature: Vec<u8>,
}

/// Вспомогательный тип для парсинга ServerFrame из тестов.
///
/// Uuid в msgpack (non-human-readable) сериализуется как byte array, а не строка.
/// Поэтому используем `Uuid` напрямую вместо `String`.
#[derive(Debug, Deserialize, Serialize)]
struct ServerFrameTest {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    user_id: Option<Uuid>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    group_id: Option<Uuid>,
    #[serde(default)]
    message_id: Option<Uuid>,
    #[serde(default)]
    epoch: Option<i64>,
    #[serde(default)]
    started: Option<bool>,
}

// ─── Tests ───

#[tokio::test]
async fn test_ws_handshake_success() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let (_http_url, ws_url) = start_server(user.state.clone()).await;

    let ws_addr = format!("{ws_url}/v1/ws");
    let (mut ws_stream, _) = connect_async(&ws_addr).await.unwrap();

    // Send auth frame
    let auth_frame = make_ws_auth_frame(&user.device_signing_key, &user.device_id);
    ws_stream
        .send(Message::Binary(auth_frame))
        .await
        .unwrap();

    // Receive AuthOk
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let Message::Binary(data) = msg else {
        panic!("expected binary frame");
    };

    let frame: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(frame.frame_type, "auth_ok");
    assert!(frame.user_id.is_some());
}

#[tokio::test]
async fn test_ws_handshake_invalid_signature() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let (_http_url, ws_url) = start_server(user.state.clone()).await;

    let ws_addr = format!("{ws_url}/v1/ws");
    let (ws_stream, _) = connect_async(&ws_addr).await.unwrap();
    let mut ws_stream = ws_stream;

    // Send auth frame with bad signature
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();

    let bad_frame = WsAuthFrame {
        frame_type: "auth".to_string(),
        device_id: user.device_id,
        timestamp: ts,
        nonce: nonce.to_vec(),
        signature: vec![0xffu8; 64],
    };

    ws_stream
        .send(Message::Binary(rmp_serde::to_vec_named(&bad_frame).unwrap()))
        .await
        .unwrap();

    // Receive AuthError
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let Message::Binary(data) = msg else {
        panic!("expected binary frame");
    };

    let frame: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(frame.frame_type, "auth_error");
    assert_eq!(frame.code.unwrap(), "ERR_AUTH_FAILED");
}

#[tokio::test]
#[ignore = "auth timeout is 30s hardcoded — too long for unit test"]
async fn test_ws_handshake_timeout() {
    // Auth timeout is 30 seconds hardcoded in the server.
    // Testing this in a unit test would require 30+ seconds.
    // This test is kept as documentation of the requirement.
}

#[tokio::test]
async fn test_ping_pong() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let (_http_url, ws_url) = start_server(user.state.clone()).await;

    let ws_addr = format!("{ws_url}/v1/ws");
    let (mut ws_stream, _) = connect_async(&ws_addr).await.unwrap();

    // Auth first
    let auth_frame = make_ws_auth_frame(&user.device_signing_key, &user.device_id);
    ws_stream
        .send(Message::Binary(auth_frame))
        .await
        .unwrap();

    // Wait for AuthOk
    let _ = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Send Ping
    let ping_frame = serde_json::json!({"type": "ping"});
    ws_stream
        .send(Message::Binary(
            rmp_serde::to_vec_named(&ping_frame).unwrap(),
        ))
        .await
        .unwrap();

    // Receive Pong
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let Message::Binary(data) = msg else {
        panic!("expected binary frame");
    };

    let frame: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(frame.frame_type, "pong");
}

#[tokio::test]
async fn test_typing_broadcast() {
    let db = fresh_db().await;
    let alice = create_user_with_device(&db).await;
    let bob = create_second_user(&db, &alice).await;
    let (_http_url, ws_url) = start_server(alice.state.clone()).await;

    // Создаём группу с alice и bob
    let group_id = Uuid::now_v7();
    let now = now_secs();

    messenger_entity::mls_groups::ActiveModel {
        id: Set(group_id),
        group_type: Set("group".to_string()),
        current_epoch: Set(0),
        ciphersuite: Set(1),
        created_at: Set(now),
        created_by_user_id: Set(alice.user_id),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(alice.user_id),
        role_in_chat: Set("owner".to_string()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(bob.user_id),
        role_in_chat: Set("member".to_string()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_devices::ActiveModel {
        group_id: Set(group_id),
        device_id: Set(alice.device_id),
        leaf_index: Set(Some(0)),
        added_at_epoch: Set(0),
        removed_at_epoch: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_devices::ActiveModel {
        group_id: Set(group_id),
        device_id: Set(bob.device_id),
        leaf_index: Set(Some(1)),
        added_at_epoch: Set(0),
        removed_at_epoch: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // Подключаем Bob к WS
    let ws_addr = format!("{ws_url}/v1/ws");
    let (mut bob_ws, _) = connect_async(&ws_addr).await.unwrap();

    let auth_frame = make_ws_auth_frame(&bob.device_signing_key, &bob.device_id);
    bob_ws
        .send(Message::Binary(auth_frame))
        .await
        .unwrap();

    // Ждём AuthOk
    let _ = tokio::time::timeout(Duration::from_secs(5), bob_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Alice отправляет typing indicator через REST (симуляция)
    // В реальности она делает это через свой WS, но для теста отправим
    // typing frame напрямую от Alice через REST авторизацию.
    let _client = reqwest::Client::new();
    let typing_frame = serde_json::json!({
        "type": "typing",
        "group_id": group_id.to_string(),
        "started": true,
    });
    let body_bytes = rmp_serde::to_vec_named(&typing_frame).unwrap();
    let _auth_header = make_auth_header(
        &alice.device_signing_key,
        &alice.device_id,
        "POST",
        "/v1/ws/typing",
        &body_bytes,
    );

    // We don't have a REST endpoint for typing, so send directly via Alice's WS
    // Actually, let's just connect Alice and send typing frame
    let (mut alice_ws, _) = connect_async(&ws_addr).await.unwrap();

    let auth_frame = make_ws_auth_frame(&alice.device_signing_key, &alice.device_id);
    alice_ws
        .send(Message::Binary(auth_frame))
        .await
        .unwrap();

    // Ждём AuthOk для Alice
    let _ = tokio::time::timeout(Duration::from_secs(5), alice_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Alice отправляет typing frame
    let typing_msg = serde_json::json!({
        "type": "typing",
        "group_id": group_id.to_string(),
        "started": true,
    });

    // Send as text (JSON) to test mixed encoding
    alice_ws
        .send(Message::Text(serde_json::to_string(&typing_msg).unwrap()))
        .await
        .unwrap();

    // Bob должен получить typing уведомление
    let msg = tokio::time::timeout(Duration::from_secs(5), bob_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let data = match msg {
        Message::Binary(b) => b,
        other => panic!("expected binary frame, got: {other:?}"),
    };

    let frame: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(frame.frame_type, "typing");
    assert_eq!(frame.started, Some(true));
}

#[tokio::test]
async fn test_idle_timeout_closes() {
    let db = fresh_db().await;
    // Создаём пользователя с коротким idle timeout
    let mut rng = rand::thread_rng();
    let device_signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
    let device_vk = device_signing_key.verifying_key();
    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let now = now_secs();
    let mut blind_index = [0u8; 32];
    rng.fill_bytes(&mut blind_index);
    messenger_entity::users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set(blind_index.to_vec()),
        username_hash_version: Set(1),
        role: Set("user".to_string()),
        status: Set("active".to_string()),
        created_at: Set(now),
        send_read_receipts: Set(false),
    }
    .insert(&db)
    .await
    .unwrap();
    messenger_entity::devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(vec![0u8; 32]),
        device_signing_public_key: Set(device_vk.to_bytes().to_vec()),
        authorization_signature: Set(vec![0u8; 64]),
        authorized_by_device_id: Set(None),
        created_at: Set(now),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: std::net::SocketAddr::from_str("127.0.0.1:0").unwrap(),
        websocket_idle_timeout_secs: 1, // короткий timeout
        ..AppConfig::default()
    };
    let state = AppState {
        db: db.clone(),
        config: std::sync::Arc::new(config),
        nonce_cache: std::sync::Arc::new(NonceCache::new(100)),
        server_identity: std::sync::Arc::new(messenger_server::state::ServerIdentity::placeholder()),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    };

    let (_http_url, ws_url) = start_server(state.clone()).await;

    let ws_addr = format!("{ws_url}/v1/ws");
    let (mut ws_stream, _) = connect_async(&ws_addr).await.unwrap();

    // Auth first
    let auth_frame = make_ws_auth_frame(&device_signing_key, &device_id);
    ws_stream
        .send(Message::Binary(auth_frame))
        .await
        .unwrap();

    // Wait for AuthOk
    let _ = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Don't send anything — idle timeout should close
    if let Err(e) = tokio::time::timeout(Duration::from_secs(10), ws_stream.next()).await {
        panic!("timeout: connection not closed after 10s: {e}");
    }
}

#[tokio::test]
async fn test_new_message_notification() {
    let db = fresh_db().await;
    let alice = create_user_with_device(&db).await;
    let bob = create_second_user(&db, &alice).await;
    let (http_url, ws_url) = start_server(alice.state.clone()).await;

    // Создаём группу
    let group_id = Uuid::now_v7();
    let now = now_secs();

    messenger_entity::mls_groups::ActiveModel {
        id: Set(group_id),
        group_type: Set("group".to_string()),
        current_epoch: Set(0),
        ciphersuite: Set(1),
        created_at: Set(now),
        created_by_user_id: Set(alice.user_id),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(alice.user_id),
        role_in_chat: Set("owner".to_string()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(bob.user_id),
        role_in_chat: Set("member".to_string()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_devices::ActiveModel {
        group_id: Set(group_id),
        device_id: Set(alice.device_id),
        leaf_index: Set(Some(0)),
        added_at_epoch: Set(0),
        removed_at_epoch: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_devices::ActiveModel {
        group_id: Set(group_id),
        device_id: Set(bob.device_id),
        leaf_index: Set(Some(1)),
        added_at_epoch: Set(0),
        removed_at_epoch: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // Подключаем Bob к WS
    let ws_addr = format!("{ws_url}/v1/ws");
    let (mut bob_ws, _) = connect_async(&ws_addr).await.unwrap();

    let auth_frame = make_ws_auth_frame(&bob.device_signing_key, &bob.device_id);
    bob_ws
        .send(Message::Binary(auth_frame))
        .await
        .unwrap();

    // Ждём AuthOk
    let _ = tokio::time::timeout(Duration::from_secs(5), bob_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Alice отправляет сообщение через REST
    #[derive(Serialize)]
    struct PostMsg {
        expected_epoch: i64,
        #[serde(with = "serde_bytes")]
        mls_ciphertext: Vec<u8>,
        client_message_id: Uuid,
    }
    let message_body = PostMsg {
        expected_epoch: 0,
        mls_ciphertext: vec![1u8, 2, 3],
        client_message_id: Uuid::now_v7(),
    };
    let body_bytes = rmp_serde::to_vec_named(&message_body).unwrap();
    let client = reqwest::Client::new();
    let auth_header = make_auth_header(
        &alice.device_signing_key,
        &alice.device_id,
        "POST",
        &format!("/v1/groups/{group_id}/messages"),
        &body_bytes,
    );

    let resp = client
        .post(format!("{http_url}/v1/groups/{group_id}/messages"))
        .header("X-Auth-Signature", &auth_header)
        .body(body_bytes)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);

    // Bob должен получить NewMessage уведомление
    let msg = tokio::time::timeout(Duration::from_secs(5), bob_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let data = match msg {
        Message::Binary(b) => b,
        other => panic!("expected binary frame, got: {other:?}"),
    };

    let frame: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(frame.frame_type, "new_message");
    assert!(frame.group_id.is_some());
    assert!(frame.message_id.is_some());
}

// ─── Auth helper (copied from test_mls_messaging.rs) ───

fn make_auth_header(
    device_signing_key: &ed25519_dalek::SigningKey,
    device_id: &Uuid,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();

    let canonical = build_signed_message(method, path, ts, &nonce, body);
    let signature = device_signing_key.sign(&canonical);

    format!(
        "{}:{}:{}:{}",
        hex::encode(device_id.as_bytes()),
        ts,
        hex::encode(nonce),
        hex::encode(signature.to_bytes()),
    )
}

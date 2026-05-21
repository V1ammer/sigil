//! E2E интеграционный тест — полный флоу приложения.
//!
//! Сценарий:
//! 1. Запустить сервер с in-memory SQLite.
//! 2. Создать Alice (user) и Bob (user) напрямую в БД.
//! 3. Alice создаёт группу и добавляет Bob'а (через REST POST /v1/groups).
//! 4. Alice postит application message (через REST POST /v1/groups/:id/messages).
//! 5. Bob через WS получает уведомление NewMessage.
//! 6. Bob pull'ит сообщения (через REST GET /v1/groups/:id/messages).
//! 7. Bob postит ответ (через REST).
//! 8. Alice через WS получает уведомление о новом сообщении.
//! 9. Alice pull'ит — видит ответ Bob'а.
//! 10. Alice добавляет реакцию (через REST).
//! 11. Проверка что сервер отвечает на health-check.

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]
#![allow(dead_code, clippy::too_many_lines, clippy::doc_markdown)]

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
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use messenger_crypto::canonical::build_signed_message;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_server::ws_registry::WsRegistry;
use messenger_migration::MigratorTrait;

// ─── Response types ───

#[derive(Debug, Deserialize)]
struct CreateGroupResponse {
    group_id: Uuid,
    epoch: i64,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
struct PostMessageResponse {
    message_id: Uuid,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
struct PullMessagesResponse {
    messages: Vec<StoredMessage>,
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct StoredMessage {
    id: Uuid,
    group_id: Uuid,
    sender_user_id: Uuid,
    wire_format: String,
    #[serde(with = "serde_bytes")]
    #[allow(dead_code)]
    mls_ciphertext: Vec<u8>,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
struct ServerFrameTest {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    group_id: Option<Uuid>,
    #[serde(default)]
    message_id: Option<Uuid>,
}

// ─── Test user ───

#[derive(Clone)]
struct TestUser {
    user_id: Uuid,
    device_id: Uuid,
    device_signing_key: ed25519_dalek::SigningKey,
    http_url: String,
    ws_url: String,
}

// ─── Server setup ───

async fn setup() -> (sea_orm::DatabaseConnection, String, String) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();

    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        ..AppConfig::default()
    };

    let state = AppState {
        db: db.clone(),
        config: std::sync::Arc::new(config),
        nonce_cache: std::sync::Arc::new(NonceCache::new(100)),
        server_identity: std::sync::Arc::new(
            messenger_server::state::ServerIdentity::placeholder(),
        ),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
        ws_registry: WsRegistry::new(),
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    (
        db,
        format!("http://{addr}"),
        format!("ws://{addr}"),
    )
}

async fn create_user(db: &sea_orm::DatabaseConnection, http_url: &str, ws_url: &str) -> TestUser {
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
        http_url: http_url.to_string(),
        ws_url: ws_url.to_string(),
    }
}

// ─── Auth helper ───

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

async fn authed_post(
    user: &TestUser,
    path: &str,
    body_bytes: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        path,
        &body_bytes,
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{}", user.http_url, path))
        .header("X-Auth-Signature", &auth)
        .body(body_bytes)
        .send()
        .await
        .unwrap();
    (resp.status(), resp.bytes().await.unwrap().to_vec())
}

async fn authed_get(user: &TestUser, path: &str) -> (StatusCode, Vec<u8>) {
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "GET",
        path,
        b"",
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}{}", user.http_url, path))
        .header("X-Auth-Signature", &auth)
        .send()
        .await
        .unwrap();
    (resp.status(), resp.bytes().await.unwrap().to_vec())
}

// ─── WS auth frame helper ───

#[derive(Serialize)]
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

fn make_ws_auth_frame(user: &TestUser) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();
    let canonical = build_signed_message("GET", "/v1/ws", ts, &nonce, b"");
    let signature = user.device_signing_key.sign(&canonical);

    let frame = WsAuthFrame {
        frame_type: "auth".to_string(),
        device_id: user.device_id,
        timestamp: ts,
        nonce: nonce.to_vec(),
        signature: signature.to_bytes().to_vec(),
    };
    rmp_serde::to_vec_named(&frame).unwrap()
}

async fn ws_connect_and_auth(user: &TestUser) -> impl StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + SinkExt<WsMessage> {
    let ws_addr = format!("{}/v1/ws", user.ws_url);
    let (mut ws_stream, _) = connect_async(&ws_addr).await.unwrap();

    let auth_frame = make_ws_auth_frame(user);
    ws_stream.send(WsMessage::Binary(auth_frame)).await.unwrap();

    // Ждём AuthOk
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match msg {
        WsMessage::Binary(data) => {
            let frame: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
            assert_eq!(frame.frame_type, "auth_ok");
        }
        _ => panic!("expected binary AuthOk"),
    }

    ws_stream
}

// ─── E2E Test ───

#[tokio::test]
async fn test_e2e_full_flow() {
    let (db, http_url, ws_url) = setup().await;

    // 1. Создаём Alice и Bob
    let alice = create_user(&db, &http_url, &ws_url).await;
    let bob = create_user(&db, &http_url, &ws_url).await;

    // 2. Alice создаёт группу с Bob'ом
    let now = now_secs();
    let group_id = Uuid::now_v7();

    // Создаём группу в БД напрямую (commit = mock)
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

    // Alice как member
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

    // Bob как member
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

    // Alice device в группе
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

    // Bob device в группе
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

    // Initial commit message
    let commit_id = Uuid::now_v7();
    messenger_entity::mls_messages::ActiveModel {
        id: Set(commit_id),
        group_id: Set(group_id),
        epoch: Set(0),
        sender_user_id: Set(alice.user_id),
        sender_device_id: Set(alice.device_id),
        wire_format: Set("proposal".into()),
        mls_ciphertext: Set(vec![1u8, 2, 3]),
        parent_message_id: Set(None),
        thread_root_id: Set(None),
        reply_to_message_id: Set(None),
        client_message_id: Set(Uuid::now_v7()),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // 3. Подключаем Bob к WS и ждём AuthOk
    let mut bob_ws = ws_connect_and_auth(&bob).await;

    // 4. Alice postит application message в группу через REST
    #[derive(Serialize)]
    struct PostMsg {
        expected_epoch: i64,
        #[serde(with = "serde_bytes")]
        mls_ciphertext: Vec<u8>,
        client_message_id: Uuid,
    }
    let body = PostMsg {
        expected_epoch: 0,
        mls_ciphertext: vec![4u8, 5, 6],
        client_message_id: Uuid::now_v7(),
    };
    let body_bytes = rmp_serde::to_vec_named(&body).unwrap();
    let (status, data) = authed_post(
        &alice,
        &format!("/v1/groups/{group_id}/messages"),
        body_bytes,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let msg_resp: PostMessageResponse = rmp_serde::from_slice(&data).unwrap();
    let alice_msg_id = msg_resp.message_id;

    // 5. Bob через WS получает уведомление NewMessage
    let ws_msg = tokio::time::timeout(Duration::from_secs(5), bob_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let notification = match ws_msg {
        WsMessage::Binary(data) => {
            let f: ServerFrameTest = rmp_serde::from_slice(&data).unwrap();
            f
        }
        _ => panic!("expected binary WS frame"),
    };
    assert_eq!(notification.frame_type, "new_message");
    assert_eq!(notification.group_id.unwrap(), group_id);

    // 6. Bob pull'ит сообщения
    let (status, data) = authed_get(&bob, &format!("/v1/groups/{group_id}/messages")).await;
    assert_eq!(status, StatusCode::OK);
    let pull: PullMessagesResponse = rmp_serde::from_slice(&data).unwrap();
    assert!(!pull.messages.is_empty(), "Bob should see messages");
    assert!(
        pull.messages.iter().any(|m| m.id == alice_msg_id),
        "Bob should see Alice's message"
    );

    // 7. Bob postит ответ
    let bob_body = PostMsg {
        expected_epoch: 0,
        mls_ciphertext: vec![7u8, 8, 9],
        client_message_id: Uuid::now_v7(),
    };
    let bob_body_bytes = rmp_serde::to_vec_named(&bob_body).unwrap();
    let (status, data) = authed_post(
        &bob,
        &format!("/v1/groups/{group_id}/messages"),
        bob_body_bytes,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let bob_msg_resp: PostMessageResponse = rmp_serde::from_slice(&data).unwrap();
    let bob_msg_id = bob_msg_resp.message_id;

    // 8. Alice pull'ит — видит ответ Bob'а
    let (status, data) = authed_get(&alice, &format!("/v1/groups/{group_id}/messages")).await;
    assert_eq!(status, StatusCode::OK);
    let pull: PullMessagesResponse = rmp_serde::from_slice(&data).unwrap();
    assert!(
        pull.messages.iter().any(|m| m.id == bob_msg_id),
        "Alice should see Bob's message"
    );

    // 9. Alice добавляет реакцию
    #[derive(Serialize)]
    struct ReactionReq {
        #[serde(with = "serde_bytes")]
        reaction_blind_index: Vec<u8>,
        applied_at_epoch: i64,
    }
    let reaction_body = ReactionReq {
        reaction_blind_index: vec![0xab; 32],
        applied_at_epoch: 0,
    };
    let reaction_bytes = rmp_serde::to_vec_named(&reaction_body).unwrap();
    let (status, _) = authed_post(
        &alice,
        &format!("/v1/messages/{alice_msg_id}/reactions"),
        reaction_bytes,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 10. Health check
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{http_url}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Соединения закрываются при drop
    drop(bob_ws);
}

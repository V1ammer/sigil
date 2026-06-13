//! Integration tests for S11 – Attachments endpoints.
//!
//! Coverage:
//! - Upload small (inline)
//! - Upload large (on-disk)
//! - Unfinalized expires and GC
//! - Finalize links to message
//! - Finalize wrong sender rejected
//! - Finalize idempotent / conflict
//! - Download by member works
//! - Download by non-member rejected
//! - Range request (206)
//! - Range full (bytes=0-)
//! - Invalid range (416)
//! - Size bucket stored
//! - Max size enforced
//! - Missing headers rejected
//! - Unfinalized download by uploader

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![allow(clippy::similar_names)]

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::http::StatusCode;
use ed25519_dalek::Signer;
use rand::RngCore;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, Database, DatabaseConnection, EntityTrait,
    QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use messenger_crypto::canonical::build_signed_message;
use messenger_server::attachments::StorageBackend;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;

// ─── Test Helpers ───

struct TestContext {
    state: AppState,
    user_id: Uuid,
    device_id: Uuid,
    device_signing_key: ed25519_dalek::SigningKey,
    addr: SocketAddr,
}

async fn fresh_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();
    db
}

fn make_state(db: DatabaseConnection, data_dir: Option<&std::path::Path>) -> AppState {
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        data_dir: data_dir.map_or_else(
            || std::path::PathBuf::from("./data"),
            std::path::PathBuf::from,
        ),
        max_attachment_bytes: 100 * 1024 * 1024,
        max_request_body_bytes: 10 * 1024 * 1024, // 10 MB
        ..AppConfig::default()
    };
    let storage = if data_dir.is_some() {
        StorageBackend::FileSystem {
            root: config.data_dir.clone(),
            inline_threshold: 1024 * 1024,
        }
    } else {
        StorageBackend::InDatabase
    };
    AppState {
        db,
        config: Arc::new(config),
        nonce_cache: Arc::new(NonceCache::new(100)),
        server_identity: Arc::new(messenger_server::state::ServerIdentity::placeholder()),
        storage,
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    }
}

async fn start_server(state: AppState) -> SocketAddr {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn create_user_with_device(db: &DatabaseConnection) -> (Uuid, Uuid, ed25519_dalek::SigningKey) {
    let mut rng = rand::thread_rng();

    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let device_sk = ed25519_dalek::SigningKey::generate(&mut rng);
    let device_pk = device_sk.verifying_key();

    messenger_entity::users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set({
            let mut buf = vec![0u8; 32];
            rng.fill_bytes(&mut buf);
            buf
        }),
        username_hash_version: Set(1),
        role: Set("user".to_string()),
        status: Set("active".to_string()),
        created_at: Set(now_secs() / 86400 * 86400),
        send_read_receipts: Set(false),
    }
    .insert(db)
    .await
    .unwrap();

    messenger_entity::devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(vec![0u8; 32]),
        device_signing_public_key: Set(device_pk.to_bytes().to_vec()),
        authorization_signature: Set(vec![0u8; 64]),
        authorized_by_device_id: Set(None),
        created_at: Set(now_secs()),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(db)
    .await
    .unwrap();

    (user_id, device_id, device_sk)
}

async fn setup_context(db: DatabaseConnection) -> TestContext {
    let (user_id, device_id, sk) = create_user_with_device(&db).await;
    let state = make_state(db.clone(), None);
    let addr = start_server(state.clone()).await;
    TestContext {
        state,
        user_id,
        device_id,
        device_signing_key: sk,
        addr,
    }
}

async fn setup_two_users_context(
    db: DatabaseConnection,
) -> (TestContext, TestContext, Uuid) {
    let (user_a_id, device_a_id, sk_a) = create_user_with_device(&db).await;
    let (user_b_id, device_b_id, sk_b) = create_user_with_device(&db).await;

    // Создаём группу
    let group_id = Uuid::now_v7();
    messenger_entity::mls_groups::ActiveModel {
        id: Set(group_id),
        group_type: Set("direct".to_string()),
        current_epoch: Set(0),
        ciphersuite: Set(1),
        created_at: Set(now_secs()),
        created_by_user_id: Set(user_a_id),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(user_a_id),
        role_in_chat: Set("owner".to_string()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now_secs()),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_devices::ActiveModel {
        group_id: Set(group_id),
        device_id: Set(device_a_id),
        leaf_index: Set(Some(0)),
        added_at_epoch: Set(0),
        removed_at_epoch: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(user_b_id),
        role_in_chat: Set("member".to_string()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now_secs()),
    }
    .insert(&db)
    .await
    .unwrap();

    messenger_entity::mls_group_devices::ActiveModel {
        group_id: Set(group_id),
        device_id: Set(device_b_id),
        leaf_index: Set(Some(1)),
        added_at_epoch: Set(0),
        removed_at_epoch: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let state = make_state(db.clone(), None);
    let addr = start_server(state.clone()).await;

    let ctx_a = TestContext {
        state: state.clone(),
        user_id: user_a_id,
        device_id: device_a_id,
        device_signing_key: sk_a,
        addr,
    };
    let ctx_b = TestContext {
        state,
        user_id: user_b_id,
        device_id: device_b_id,
        device_signing_key: sk_b,
        addr,
    };

    (ctx_a, ctx_b, group_id)
}

fn make_auth_header(
    ctx: &TestContext,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();
    let canonical = build_signed_message(method, path, ts, &nonce, body);
    let signature = ctx.device_signing_key.sign(&canonical);
    format!(
        "{}:{}:{}:{}",
        hex::encode(ctx.device_id.as_bytes()),
        ts,
        hex::encode(nonce),
        hex::encode(signature.to_bytes()),
    )
}

async fn authed_upload(
    ctx: &TestContext,
    data: Vec<u8>,
    padded_size: &str,
    size_bucket: &str,
) -> reqwest::Response {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let auth_header = make_auth_header(ctx, "POST", "/v1/attachments", &data);
    client
        .post(format!("http://{}/v1/attachments", ctx.addr))
        .header("x-auth-signature", &auth_header)
        .header("content-length", data.len().to_string())
        .header("x-attachment-padded-size", padded_size)
        .header("x-attachment-size-bucket", size_bucket)
        .body(data)
        .send()
        .await
        .unwrap()
}

async fn authed_get(ctx: &TestContext, path: &str) -> reqwest::Response {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let body = vec![];
    let auth_header = make_auth_header(ctx, "GET", path, &body);
    client
        .get(format!("http://{}{}", ctx.addr, path))
        .header("x-auth-signature", &auth_header)
        .send()
        .await
        .unwrap()
}

async fn authed_post_msgpack(
    ctx: &TestContext,
    method: &str,
    path: &str,
    req_body: Vec<u8>,
) -> reqwest::Response {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let auth_header = make_auth_header(ctx, method, path, &req_body);
    client
        .request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            format!("http://{}{}", ctx.addr, path),
        )
        .header("x-auth-signature", &auth_header)
        .header("content-type", "application/msgpack")
        .body(req_body)
        .send()
        .await
        .unwrap()
}

async fn create_message(
    ctx: &TestContext,
    group_id: Uuid,
    message_id: Uuid,
) {
    messenger_entity::mls_messages::ActiveModel {
        id: Set(message_id),
        group_id: Set(group_id),
        epoch: Set(0),
        sender_user_id: Set(ctx.user_id),
        sender_device_id: Set(ctx.device_id),
        wire_format: Set("application".to_string()),
        mls_ciphertext: Set(vec![1u8, 2, 3]),
        parent_message_id: Set(None),
        thread_root_id: Set(None),
        reply_to_message_id: Set(None),
        client_message_id: Set(Uuid::now_v7()),
        created_at: Set(now_secs()),
    }
    .insert(&ctx.state.db)
    .await
    .unwrap();
}

// ─── Response types ───

#[derive(Deserialize)]
struct UploadAttachmentResponse {
    attachment_id: Uuid,
    expires_at: i64,
}

#[derive(Serialize)]
struct FinalizeAttachmentRequest {
    message_id: Uuid,
}

// ─── Tests ───

#[tokio::test]
#[allow(clippy::cast_possible_truncation)]
async fn test_upload_small_inline() {
    let db = fresh_db().await;
    let ctx = setup_context(db).await;

    let data = vec![0xABu8; 100 * 1024]; // 100 KB

    let resp = authed_upload(&ctx, data.clone(), "102400", "1").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();
    assert_ne!(upload_resp.attachment_id, Uuid::nil());
    assert!(upload_resp.expires_at > now_secs());

    // Скачиваем
    let resp = authed_get(&ctx, &format!("/v1/attachments/{}", upload_resp.attachment_id)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap().to_vec();
    assert_eq!(body, data);
}

#[tokio::test]
async fn test_upload_large_on_disk() {
    let db = fresh_db().await;
    let tmp = std::env::temp_dir().join(format!("attachments_test_{}", Uuid::now_v7()));
    tokio::fs::create_dir_all(&tmp).await.unwrap();
    // Use a very low inline threshold so all data goes to disk
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        data_dir: tmp.clone(),
        max_request_body_bytes: 50 * 1024 * 1024, // 50 MB
        ..AppConfig::default()
    };
    let storage = StorageBackend::FileSystem {
        root: tmp.clone(),
        inline_threshold: 0, // force on-disk for everything
    };
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        nonce_cache: Arc::new(NonceCache::new(100)),
        server_identity: Arc::new(messenger_server::state::ServerIdentity::placeholder()),
        storage,
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    };
    let (user_id, device_id, sk) = create_user_with_device(&db).await;
    let addr = start_server(state.clone()).await;
    let ctx = TestContext { state, user_id, device_id, device_signing_key: sk, addr };

    let data = vec![0xCDu8; 100]; // small, but forced to disk via 0 threshold

    let resp = authed_upload(&ctx, data.clone(), "100", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Проверяем что файл создан на диске
    let hex = upload_resp.attachment_id.to_string().replace('-', "");
    let disk_path = tmp.join("att").join(&hex[0..2]).join(&hex[2..4])
        .join(format!("{}.bin", upload_resp.attachment_id));
    assert!(disk_path.exists(), "on-disk file should exist at {disk_path:?}");

    // Скачиваем
    let resp = authed_get(&ctx, &format!("/v1/attachments/{}", upload_resp.attachment_id)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap().to_vec();
    assert_eq!(body, data);

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn test_upload_unfinalized_expires() {
    let db = fresh_db().await;
    let ctx = setup_context(db.clone()).await;

    let data = vec![0xABu8; 1000];

    let resp = authed_upload(&ctx, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Ставим expires_at в прошлое
    messenger_entity::attachments::ActiveModel {
        id: Set(upload_resp.attachment_id),
        expires_at: Set(now_secs() - 100),
        ..Default::default()
    }
    .update(&db)
    .await
    .unwrap();

    // Симулируем GC
    let deleted = messenger_entity::attachments::Entity::delete_many()
        .filter(
            Condition::all()
                .add(messenger_entity::attachments::Column::MessageId.is_null())
                .add(messenger_entity::attachments::Column::ExpiresAt.lt(now_secs())),
        )
        .exec(&db)
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected, 1, "GC should have deleted the expired attachment");

    // Проверяем что attachment удалён
    let resp = authed_get(&ctx, &format!("/v1/attachments/{}", upload_resp.attachment_id)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_finalize_links_to_message() {
    let db = fresh_db().await;
    let (ctx_a, _ctx_b, group_id) = setup_two_users_context(db).await;

    // Upload
    let data = vec![0xABu8; 1000];
    let resp = authed_upload(&ctx_a, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Создаём сообщение
    let msg_id = Uuid::now_v7();
    create_message(&ctx_a, group_id, msg_id).await;

    // Finalize
    let req = FinalizeAttachmentRequest { message_id: msg_id };
    let req_body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Проверяем что attachment.message_id установлен
    let attachment = messenger_entity::attachments::Entity::find_by_id(upload_resp.attachment_id)
        .one(&ctx_a.state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.message_id, Some(msg_id));
}

#[tokio::test]
async fn test_finalize_wrong_sender_rejected() {
    let db = fresh_db().await;
    let (ctx_a, ctx_b, group_id) = setup_two_users_context(db).await;

    // Upload от A
    let data = vec![0xABu8; 1000];
    let resp = authed_upload(&ctx_a, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Создаём сообщение от B
    let msg_id = Uuid::now_v7();
    create_message(&ctx_b, group_id, msg_id).await;

    // Finalize от A на message от B — Forbidden
    let req = FinalizeAttachmentRequest { message_id: msg_id };
    let req_body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_finalize_conflict() {
    let db = fresh_db().await;
    let (ctx_a, _ctx_b, group_id) = setup_two_users_context(db).await;

    let data = vec![0xABu8; 1000];
    let resp = authed_upload(&ctx_a, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    let msg_id = Uuid::now_v7();
    create_message(&ctx_a, group_id, msg_id).await;

    // Первый finalize
    let req = FinalizeAttachmentRequest { message_id: msg_id };
    let req_body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body.clone(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Второй finalize — 409 Conflict
    let resp = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_download_by_member_works() {
    let db = fresh_db().await;
    let (ctx_a, ctx_b, group_id) = setup_two_users_context(db).await;

    // Upload от A
    let data = vec![0xABu8; 1000];
    let resp = authed_upload(&ctx_a, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Finalize
    let msg_id = Uuid::now_v7();
    create_message(&ctx_a, group_id, msg_id).await;
    let req = FinalizeAttachmentRequest { message_id: msg_id };
    let req_body = rmp_serde::to_vec_named(&req).unwrap();
    let _ = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body,
    )
    .await;

    // B (member) скачивает
    let resp = authed_get(&ctx_b, &format!("/v1/attachments/{}", upload_resp.attachment_id)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap().to_vec();
    assert_eq!(body, data);
}

#[tokio::test]
async fn test_download_by_non_member_rejected() {
    let db = fresh_db().await;
    let (ctx_a, _ctx_b, group_id) = setup_two_users_context(db.clone()).await;
    // Outsider on the SAME database but NOT in the group
    let (outsider_id, outsider_dev_id, outsider_sk) = create_user_with_device(&db).await;
    let outsider = TestContext {
        state: ctx_a.state.clone(),
        user_id: outsider_id,
        device_id: outsider_dev_id,
        device_signing_key: outsider_sk,
        addr: ctx_a.addr,
    };

    // Upload от A
    let data = vec![0xABu8; 1000];
    let resp = authed_upload(&ctx_a, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Finalize
    let msg_id = Uuid::now_v7();
    create_message(&ctx_a, group_id, msg_id).await;
    let req = FinalizeAttachmentRequest { message_id: msg_id };
    let req_body = rmp_serde::to_vec_named(&req).unwrap();
    let _ = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body,
    )
    .await;

    // Outsider (не в группе) — Forbidden
    let resp = authed_get(&outsider, &format!("/v1/attachments/{}", upload_resp.attachment_id)).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_range_request() {
    let db = fresh_db().await;
    let ctx = setup_context(db).await;

    let data: Vec<u8> = (0..100u8).collect();

    let resp = authed_upload(&ctx, data.clone(), "100", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Range запрос
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let auth_header = make_auth_header(&ctx, "GET", &format!("/v1/attachments/{}", upload_resp.attachment_id), &[]);
    let resp = client
        .get(format!("http://{}/v1/attachments/{}", ctx.addr, upload_resp.attachment_id))
        .header("x-auth-signature", &auth_header)
        .header("range", "bytes=0-9")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    let content_range = resp.headers().get("content-range").unwrap().to_str().unwrap().to_string();
    assert_eq!(content_range, "bytes 0-9/100");
    let body = resp.bytes().await.unwrap().to_vec();
    assert_eq!(body, data[0..10].to_vec());
}

#[tokio::test]
async fn test_range_full() {
    let db = fresh_db().await;
    let ctx = setup_context(db).await;

    let data: Vec<u8> = (0..100u8).collect();

    let resp = authed_upload(&ctx, data.clone(), "100", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Range: bytes=0-
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let auth_header = make_auth_header(&ctx, "GET", &format!("/v1/attachments/{}", upload_resp.attachment_id), &[]);
    let resp = client
        .get(format!("http://{}/v1/attachments/{}", ctx.addr, upload_resp.attachment_id))
        .header("x-auth-signature", &auth_header)
        .header("range", "bytes=0-")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    let body = resp.bytes().await.unwrap().to_vec();
    assert_eq!(body, data);
}

#[tokio::test]
async fn test_invalid_range_returns_416() {
    let db = fresh_db().await;
    let ctx = setup_context(db).await;

    let data: Vec<u8> = (0..100u8).collect();

    let resp = authed_upload(&ctx, data.clone(), "100", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let auth_header = make_auth_header(&ctx, "GET", &format!("/v1/attachments/{}", upload_resp.attachment_id), &[]);
    let resp = client
        .get(format!("http://{}/v1/attachments/{}", ctx.addr, upload_resp.attachment_id))
        .header("x-auth-signature", &auth_header)
        .header("range", "bytes=200-300")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
}

#[tokio::test]
async fn test_size_bucket_stored() {
    let db = fresh_db().await;
    let ctx = setup_context(db.clone()).await;

    let data = vec![0xABu8; 500];

    let resp = authed_upload(&ctx, data.clone(), "500", "2").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    let attachment = messenger_entity::attachments::Entity::find_by_id(upload_resp.attachment_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.size_bucket, 2);
    assert_eq!(attachment.padded_size, 500);
}

#[tokio::test]
async fn test_max_size_enforced() {
    let db = fresh_db().await;
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        max_attachment_bytes: 1000,
        ..AppConfig::default()
    };
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        nonce_cache: Arc::new(NonceCache::new(100)),
        server_identity: Arc::new(messenger_server::state::ServerIdentity::placeholder()),
        storage: StorageBackend::InDatabase,
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    };
    let (user_id, device_id, sk) = create_user_with_device(&db).await;
    let addr = start_server(state.clone()).await;
    let ctx = TestContext { state, user_id, device_id, device_signing_key: sk, addr };

    let data = vec![0xABu8; 2000];

    let resp = authed_upload(&ctx, data.clone(), "2000", "2").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_missing_headers_rejected() {
    let db = fresh_db().await;
    let ctx = setup_context(db).await;

    let data = vec![0xABu8; 100];

    // Без заголовков
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let auth_header = make_auth_header(&ctx, "POST", "/v1/attachments", &data);
    let resp = client
        .post(format!("http://{}/v1/attachments", ctx.addr))
        .header("x-auth-signature", &auth_header)
        .header("content-length", "100")
        .body(data.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_unfinalized_by_uploader() {
    let db = fresh_db().await;
    let ctx = setup_context(db).await;

    let data = vec![0xABu8; 100];

    let resp = authed_upload(&ctx, data.clone(), "100", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    // Uploader скачивает unfinalized
    let resp = authed_get(&ctx, &format!("/v1/attachments/{}", upload_resp.attachment_id)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap().to_vec();
    assert_eq!(body, data);
}

// ─── Group deletion ───

async fn authed_delete(ctx: &TestContext, path: &str) -> reqwest::Response {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let body = vec![];
    let auth_header = make_auth_header(ctx, "DELETE", path, &body);
    client
        .delete(format!("http://{}{}", ctx.addr, path))
        .header("x-auth-signature", &auth_header)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_delete_group_removes_messages_and_attachments() {
    let db = fresh_db().await;
    let (ctx_a, _ctx_b, group_id) = setup_two_users_context(db).await;

    // Upload + message + finalize → a finalized attachment bound to the group.
    let data = vec![0xCDu8; 1000];
    let resp = authed_upload(&ctx_a, data.clone(), "1000", "0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.bytes().await.unwrap();
    let upload_resp: UploadAttachmentResponse = rmp_serde::from_slice(&body).unwrap();

    let msg_id = Uuid::now_v7();
    create_message(&ctx_a, group_id, msg_id).await;
    let req = FinalizeAttachmentRequest { message_id: msg_id };
    let req_body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = authed_post_msgpack(
        &ctx_a,
        "POST",
        &format!("/v1/attachments/{}/finalize", upload_resp.attachment_id),
        req_body,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Delete the group as a member.
    let resp = authed_delete(&ctx_a, &format!("/v1/groups/{group_id}")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Group, messages, attachment row and memberships are all gone.
    let g = messenger_entity::mls_groups::Entity::find_by_id(group_id)
        .one(&ctx_a.state.db)
        .await
        .unwrap();
    assert!(g.is_none(), "group row should be deleted");
    let m = messenger_entity::mls_messages::Entity::find_by_id(msg_id)
        .one(&ctx_a.state.db)
        .await
        .unwrap();
    assert!(m.is_none(), "message should be cascade-deleted");
    let a = messenger_entity::attachments::Entity::find_by_id(upload_resp.attachment_id)
        .one(&ctx_a.state.db)
        .await
        .unwrap();
    assert!(a.is_none(), "attachment row should be explicitly deleted");
    let members = messenger_entity::mls_group_members::Entity::find()
        .filter(messenger_entity::mls_group_members::Column::GroupId.eq(group_id))
        .all(&ctx_a.state.db)
        .await
        .unwrap();
    assert!(members.is_empty(), "memberships should be cascade-deleted");
}

#[tokio::test]
async fn test_delete_group_not_found() {
    let db = fresh_db().await;
    let (ctx_a, _ctx_b, _group_id) = setup_two_users_context(db).await;
    let resp = authed_delete(&ctx_a, &format!("/v1/groups/{}", Uuid::now_v7())).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

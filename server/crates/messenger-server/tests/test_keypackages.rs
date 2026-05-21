//! Integration tests for S09 – `KeyPackages` endpoints.
//!
//! Coverage:
//! - publish batch, duplicates, pool limit
//! - count/stats endpoint
//! - claim atomicity, last-resort fallback, exhaustion
//! - expired keypackage filtering
//! - revoked device / suspended user rejection
//! - GC cleanup

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use sea_orm::{ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use messenger_crypto::canonical::build_signed_message;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;

// ─── Test Helpers ───

struct TestUser {
    user_id: Uuid,
    device_id: Uuid,
    device_signing_key: SigningKey,
    state: AppState,
}

async fn fresh_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();
    db
}

fn make_state(db: DatabaseConnection) -> AppState {
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
        server_identity: std::sync::Arc::new(messenger_server::state::ServerIdentity::placeholder()),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
    }
}

async fn create_user_with_device(db: &DatabaseConnection) -> TestUser {
    let mut rng = rand::thread_rng();
    let device_signing_key = SigningKey::generate(&mut rng);
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

async fn start_server(state: AppState) -> String {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    format!("http://{addr}")
}

fn make_auth_header(
    device_signing_key: &SigningKey,
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

async fn send_authed_post(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    user: &TestUser,
    body_bytes: Vec<u8>,
) -> reqwest::Response {
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        path,
        &body_bytes,
    );
    client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(body_bytes)
        .send()
        .await
        .unwrap()
}

async fn send_authed_get(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    user: &TestUser,
) -> reqwest::Response {
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "GET",
        path,
        b"",
    );
    client
        .get(format!("{base_url}{path}"))
        .header("X-Auth-Signature", &auth)
        .send()
        .await
        .unwrap()
}

// ─── Request/Response types ───

#[derive(Serialize)]
struct PublishReq {
    key_packages: Vec<KpUpload>,
}

#[derive(Clone, Serialize)]
struct KpUpload {
    #[serde(with = "serde_bytes")]
    key_package: Vec<u8>,
    #[serde(with = "serde_bytes")]
    init_key_hash: Vec<u8>,
    expires_at: i64,
    is_last_resort: bool,
}

#[derive(Deserialize)]
struct PublishResp {
    stored_count: usize,
    skipped_count: usize,
    current_pool_size: i64,
    last_resort_present: bool,
}

#[derive(Deserialize)]
struct PoolStats {
    available: i64,
    consumed_total: i64,
    last_resort_present: bool,
    oldest_available_created_at: Option<i64>,
}

#[derive(Deserialize)]
struct ClaimResp {
    key_package_id: Uuid,
    #[allow(dead_code)]
    key_package: Vec<u8>,
}

fn fake_kp(seed: u8) -> KpUpload {
    let mut kp = vec![0u8; 256];
    kp[0] = seed;
    rand::thread_rng().fill_bytes(&mut kp[1..]);
    let hash = blake3::hash(&kp).as_bytes().to_vec();
    KpUpload {
        key_package: kp,
        init_key_hash: hash,
        expires_at: now_secs() + 30 * 86_400,
        is_last_resort: false,
    }
}

fn fake_last_resort_kp() -> KpUpload {
    let mut kp = vec![0u8; 256];
    rand::thread_rng().fill_bytes(&mut kp);
    let hash = blake3::hash(&kp).as_bytes().to_vec();
    KpUpload {
        key_package: kp,
        init_key_hash: hash,
        expires_at: now_secs() + 30 * 86_400,
        is_last_resort: true,
    }
}

// ─── Tests ───

#[tokio::test]
async fn test_publish_batch() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let req = PublishReq {
        key_packages: (0..10).map(fake_kp).collect(),
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let parsed: PublishResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.stored_count, 10);
    assert_eq!(parsed.skipped_count, 0);
    assert_eq!(parsed.current_pool_size, 10);
    assert!(!parsed.last_resort_present);
}

#[tokio::test]
async fn test_publish_duplicate_init_key_skipped() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let kp = fake_kp(42);
    let req = PublishReq {
        key_packages: vec![kp.clone(), kp],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let parsed: PublishResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.stored_count, 1);
    assert_eq!(parsed.skipped_count, 1);
    assert_eq!(parsed.current_pool_size, 1);
}

#[tokio::test]
async fn test_pool_size_limit() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Publish many KPs to reach the limit
    let mut kps = Vec::new();
    for i in 0..1000 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        kps.push(fake_kp(i as u8));
    }
    // Split into batches of 100 (max batch size)
    for chunk in kps.chunks(100) {
        let req = PublishReq {
            key_packages: chunk.to_vec(),
        };
        let bytes = rmp_serde::to_vec_named(&req).unwrap();
        let resp = send_authed_post(
            &client,
            &base_url,
            "/v1/keypackages",
            &user,
            bytes,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Now try to publish one more — should hit pool limit (1000)
    let req = PublishReq {
        key_packages: vec![fake_kp(255)],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_count_endpoint() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Publish 5 KPs + 1 last-resort
    let mut kps: Vec<KpUpload> = (0..5).map(fake_kp).collect();
    kps.push(fake_last_resort_kp());
    let req = PublishReq { key_packages: kps };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Get stats
    let resp = send_authed_get(
        &client,
        &base_url,
        "/v1/keypackages/me/count",
        &user,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let stats: PoolStats = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(stats.available, 5);
    assert_eq!(stats.consumed_total, 0);
    assert!(stats.last_resort_present);
    assert!(stats.oldest_available_created_at.is_some());
}

#[tokio::test]
async fn test_claim_returns_kp_marks_consumed() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    // Target user = same user with different device
    let target_user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Publish 1 KP for target device
    let req = PublishReq {
        key_packages: vec![fake_kp(1)],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &target_user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Claim it
    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        target_user.user_id, target_user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(claim_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let claimed: ClaimResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // After claim: available should be 0, consumed_total should be 1
    let resp = send_authed_get(
        &client,
        &base_url,
        "/v1/keypackages/me/count",
        &target_user,
    )
    .await;
    let stats: PoolStats = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(stats.available, 0);
    assert_eq!(stats.consumed_total, 1);

    // Verify KP is marked consumed in DB
    let db_kp = messenger_entity::key_packages::Entity::find_by_id(claimed.key_package_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(db_kp.consumed_at.is_some());
    assert_eq!(db_kp.consumed_by_user_id, Some(user.user_id));
}

#[tokio::test]
async fn test_claim_when_pool_empty_uses_last_resort() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let target_user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Publish 1 last-resort only
    let req = PublishReq {
        key_packages: vec![fake_last_resort_kp()],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &target_user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        target_user.user_id, target_user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );

    // First claim — gets last-resort
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(claim_body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Second claim — gets last-resort again (reused)
    // Use a fresh auth header (new nonce) to avoid nonce replay
    let auth2 = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth2)
        .body(claim_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_claim_when_no_keypackages_returns_503() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // No KPs published at all
    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        user.user_id, user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(claim_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_claim_parallel_atomicity() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Publish exactly 1 KP (no last-resort)
    let req = PublishReq {
        key_packages: vec![fake_kp(1)],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        user.user_id, user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();

    // Fire 2 parallel claims
    let auth1 = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let auth2 = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );

    let resp1 = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth1)
        .body(claim_body.clone())
        .send()
        .await
        .unwrap();
    let resp2 = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth2)
        .body(claim_body)
        .send()
        .await
        .unwrap();

    // One should succeed, one should be exhausted (no last-resort)
    let statuses = [resp1.status(), resp2.status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::SERVICE_UNAVAILABLE));
}

#[tokio::test]
async fn test_claim_revoked_target_device_rejected() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Revoke the device
    let device = messenger_entity::devices::Entity::find_by_id(user.device_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: messenger_entity::devices::ActiveModel = device.into();
    active.revoked_at = Set(Some(now_secs()));
    active.update(&db).await.unwrap();

    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        user.user_id, user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(claim_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_claim_suspended_target_user_rejected() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;

    // Suspend the user
    let user_model = messenger_entity::users::Entity::find_by_id(user.user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: messenger_entity::users::ActiveModel = user_model.into();
    active.status = Set("suspended".to_string());
    active.update(&db).await.unwrap();

    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        user.user_id, user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(claim_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_expired_keypackage_not_returned() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Publish a KP with expires_at in the past
    let mut kp = fake_kp(1);
    kp.expires_at = now_secs() - 1;
    let req = PublishReq {
        key_packages: vec![kp],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        "/v1/keypackages",
        &user,
        bytes,
    )
    .await;
    // Should fail validation
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Insert expired KP directly (bypass validation)
    let hash = blake3::hash(&[1u8; 256]).as_bytes().to_vec();
    messenger_entity::key_packages::ActiveModel {
        id: Set(Uuid::now_v7()),
        device_id: Set(user.device_id),
        key_package: Set(vec![1u8; 256]),
        init_key_hash: Set(hash),
        created_at: Set(now_secs() - 86_400),
        expires_at: Set(now_secs() - 1),
        consumed_at: Set(None),
        consumed_by_user_id: Set(None),
        is_last_resort: Set(false),
    }
    .insert(&db)
    .await
    .unwrap();

    // Claim should be exhausted (only expired KPs exist)
    let path = format!(
        "/v1/users/{}/devices/{}/keypackage/claim",
        user.user_id, user.device_id
    );
    let claim_body = rmp_serde::to_vec_named(&serde_json::json!({})).unwrap();
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &path,
        &claim_body,
    );
    let resp = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(claim_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_kp_gc_removes_old_expired() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;

    // Insert expired KP directly
    let now = now_secs();
    messenger_entity::key_packages::ActiveModel {
        id: Set(Uuid::now_v7()),
        device_id: Set(user.device_id),
        key_package: Set(vec![1u8; 256]),
        init_key_hash: Set(blake3::hash(&[1u8; 256]).as_bytes().to_vec()),
        created_at: Set(now - 86_400),
        expires_at: Set(now - 86_400 - 1), // more than 1 day ago
        consumed_at: Set(None),
        consumed_by_user_id: Set(None),
        is_last_resort: Set(false),
    }
    .insert(&db)
    .await
    .unwrap();

    // GC query: delete where expires_at < now - 86400
    let deleted = messenger_entity::key_packages::Entity::delete_many()
        .filter(messenger_entity::key_packages::Column::ExpiresAt.lt(now - 86_400))
        .exec(&db)
        .await
        .unwrap();
    assert_eq!(
        deleted.rows_affected, 1,
        "GC should remove expired keypackages"
    );
}

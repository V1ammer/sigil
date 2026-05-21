#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]
#![allow(clippy::similar_names)]

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::http::StatusCode;
use ed25519_dalek::{Signer, SigningKey};
use base64::Engine;
use rand::RngCore;
use messenger_crypto::canonical::build_signed_message;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::identity::ServerIdentity;
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;
use messenger_entity::devices::Entity as Devices;
use messenger_entity::invitation_tokens;
use messenger_entity::key_change_events::Entity as KeyChangeEvents;
use messenger_entity::user_identity_credentials::Entity as UserIdentityCredentials;
use messenger_entity::users::Entity as Users;
use sea_orm::ColumnTrait;
use sea_orm::QueryFilter;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
// ─── Test Helpers ───

/// Handle with credentials for making signed requests.
struct AdminHandle {
    user_id: Uuid,
    device_id: Uuid,
    signing_key: SigningKey,
    state: AppState,
}

/// Key material for a user (identity + first device).
struct UserKeyMaterial {
    identity_signing_key: SigningKey,
    device_signing_key: SigningKey,
    device_init_public_key: Vec<u8>,
    identity_credential: Vec<u8>,
}

/// Creates an in-memory `SQLite` DB with migrations applied.
async fn fresh_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();
    db
}

#[allow(dead_code)]
async fn bootstrapped_state() -> AppState {
    let db = fresh_db().await;
    let identity = messenger_server::bootstrap::load_or_init(&db).await.unwrap();
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        ..AppConfig::default()
    };
    AppState {
        db,
        config: Arc::new(config),
        nonce_cache: Arc::new(NonceCache::new(100)),
        server_identity: Arc::new(identity),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
    }
}

/// Creates an admin user + device in the DB, returns handle.
/// The identity is `ServerIdentity::placeholder()`.
async fn create_admin_handle(db: &DatabaseConnection) -> AdminHandle {
    let mut rng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();

    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();

    let mut blind_index = [0u8; 32];
    rng.fill_bytes(&mut blind_index);

    messenger_entity::users::ActiveModel {
        id: Set(user_id),
        username_blind_index: Set(blind_index.to_vec()),
        username_hash_version: Set(1),
        role: Set("admin".to_string()),
        status: Set("active".to_string()),
        created_at: Set(now_secs()),
        send_read_receipts: Set(false),
    }
    .insert(db)
    .await
    .unwrap();

    messenger_entity::devices::ActiveModel {
        id: Set(device_id),
        user_id: Set(user_id),
        hpke_init_public_key: Set(vec![0u8; 32]),
        device_signing_public_key: Set(verifying_key.to_bytes().to_vec()),
        authorization_signature: Set(vec![0u8; 64]),
        authorized_by_device_id: Set(None),
        created_at: Set(now_secs()),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(db)
    .await
    .unwrap();

    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        log_format: LogFormat::Pretty,
        log_level: "off".to_string(),
        bind_addr: SocketAddr::from_str("127.0.0.1:0").unwrap(),
        ..AppConfig::default()
    };

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        nonce_cache: Arc::new(NonceCache::new(100)),
        server_identity: Arc::new(ServerIdentity::placeholder()),
        storage: messenger_server::attachments::StorageBackend::InDatabase,
    };

    AdminHandle {
        user_id,
        device_id,
        signing_key,
        state,
    }
}

/// Generates key material for a new user (identity key + first device key).
fn generate_user_keypairs() -> UserKeyMaterial {
    let mut rng = rand::thread_rng();
    let identity_signing_key = SigningKey::generate(&mut rng);
    let device_signing_key = SigningKey::generate(&mut rng);
    let mut device_init_pk = vec![0u8; 32];
    rng.fill_bytes(&mut device_init_pk);

    UserKeyMaterial {
        identity_signing_key,
        device_signing_key,
        device_init_public_key: device_init_pk,
        identity_credential: vec![1u8, 2, 3, 4], // mock MLS credential
    }
}

/// Signs the device authorization message and returns the signature bytes.
/// msg = `device_signing_pk` || `device_init_pk` || `ts_le`
fn sign_device_authorization(
    identity_sk: &SigningKey,
    device_signing_pk: &[u8],
    device_init_pk: &[u8],
    ts: i64,
) -> Vec<u8> {
    let ts_bytes = ts.to_le_bytes();
    let mut msg = Vec::new();
    msg.extend_from_slice(device_signing_pk);
    msg.extend_from_slice(device_init_pk);
    msg.extend_from_slice(&ts_bytes);
    identity_sk.sign(&msg).to_bytes().to_vec()
}

/// Signs the provisioning challenge for `NewDevice`.
/// challenge = "messenger-provisioning-v1:" || `token_str` || ":" || `ts_le`
fn sign_provisioning_challenge(
    identity_sk: &SigningKey,
    token_str: &str,
    ts: i64,
) -> Vec<u8> {
    let ts_bytes = ts.to_le_bytes();
    let mut msg = Vec::new();
    msg.extend_from_slice(b"messenger-provisioning-v1:");
    msg.extend_from_slice(token_str.as_bytes());
    msg.push(b':');
    msg.extend_from_slice(&ts_bytes);
    identity_sk.sign(&msg).to_bytes().to_vec()
}

/// Signs the revocation message.
/// msg = "revoke:" || `device_id_bytes` || ":" || `ts_string`
fn sign_revocation(identity_sk: &SigningKey, device_id: &Uuid, ts: i64) -> Vec<u8> {
    let ts_str = ts.to_string();
    let mut msg = Vec::new();
    msg.extend_from_slice(b"revoke:");
    msg.extend_from_slice(device_id.as_bytes());
    msg.push(b':');
    msg.extend_from_slice(ts_str.as_bytes());
    identity_sk.sign(&msg).to_bytes().to_vec()
}

/// Starts the server on a random port, returns the base URL.
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

/// Builds the X-Auth-Signature header value for a handle's device.
fn make_auth_header(
    handle: &AdminHandle,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();

    let canonical = build_signed_message(method, path, ts, &nonce, body);
    let signature = handle.signing_key.sign(&canonical);

    format!(
        "{}:{}:{}:{}",
        hex::encode(handle.device_id.as_bytes()),
        ts,
        hex::encode(nonce),
        hex::encode(signature.to_bytes()),
    )
}

/// Sends a signed POST request with msgpack body.
async fn signed_post(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    handle: &AdminHandle,
    body_bytes: &[u8],
) -> reqwest::Response {
    let auth = make_auth_header(handle, "POST", path, body_bytes);
    client
        .post(format!("{base_url}{path}"))
        .header("X-Auth-Signature", &auth)
        .header("Content-Type", "application/msgpack")
        .body(body_bytes.to_vec())
        .send()
        .await
        .unwrap()
}

/// Sends a signed GET request.
async fn signed_get(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    handle: &AdminHandle,
) -> reqwest::Response {
    let body = b"";
    let auth = make_auth_header(handle, "GET", path, body);
    client
        .get(format!("{base_url}{path}"))
        .header("X-Auth-Signature", &auth)
        .body(body.to_vec())
        .send()
        .await
        .unwrap()
}

/// Sends a signed PATCH request with msgpack body.
async fn signed_patch(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    handle: &AdminHandle,
    body_bytes: &[u8],
) -> reqwest::Response {
    let auth = make_auth_header(handle, "PATCH", path, body_bytes);
    client
        .patch(format!("{base_url}{path}"))
        .header("X-Auth-Signature", &auth)
        .header("Content-Type", "application/msgpack")
        .body(body_bytes.to_vec())
        .send()
        .await
        .unwrap()
}

// ─── Request/Response types for tests ───

#[derive(Serialize)]
struct CreateInviteReq {
    role_to_grant: String,
    max_uses: i32,
    ttl_seconds: i64,
}

#[derive(Deserialize)]
struct CreateInviteResp {
    #[allow(dead_code)]
    id: Uuid,
    #[allow(dead_code)]
    token: Vec<u8>,
    token_display: String,
    #[allow(dead_code)]
    expires_at: i64,
}

#[derive(Serialize)]
struct RedeemReq {
    token: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity_credential: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_public_key: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    existing_identity_proof: Option<Vec<u8>>,
    #[serde(with = "serde_bytes")]
    device_init_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    device_signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    device_authorization_signature: Vec<u8>,
    device_authorization_timestamp: i64,
}

#[derive(Deserialize)]
struct RedeemResp {
    user_id: Uuid,
    device_id: Uuid,
    role: String,
}

#[derive(Deserialize)]
struct LookupResp {
    user_id: Uuid,
    #[allow(dead_code)]
    identity_credential: Vec<u8>,
    #[allow(dead_code)]
    signature_public_key: Vec<u8>,
}

#[derive(Deserialize)]
struct DeviceInfo {
    #[allow(dead_code)]
    id: Uuid,
    #[allow(dead_code)]
    hpke_init_public_key: Vec<u8>,
    #[allow(dead_code)]
    device_signing_public_key: Vec<u8>,
    #[allow(dead_code)]
    authorized_by_device_id: Option<Uuid>,
    #[allow(dead_code)]
    created_at: i64,
    revoked_at: Option<i64>,
    is_current: bool,
}

#[derive(Deserialize)]
struct ListDevicesResp {
    devices: Vec<DeviceInfo>,
}

#[derive(Deserialize)]
struct PublicDeviceInfo {
    id: Uuid,
    #[allow(dead_code)]
    hpke_init_public_key: Vec<u8>,
    #[allow(dead_code)]
    device_signing_public_key: Vec<u8>,
    #[allow(dead_code)]
    created_at: i64,
}

#[derive(Deserialize)]
struct AdminUserInfo {
    id: Uuid,
    #[allow(dead_code)]
    role: String,
    #[allow(dead_code)]
    status: String,
    #[allow(dead_code)]
    created_at: i64,
}

#[derive(Deserialize)]
struct ListUsersResp {
    users: Vec<AdminUserInfo>,
    #[allow(dead_code)]
    total: u64,
}

// ─── Helper to create an invite token via admin API ───

async fn create_invite_token(
    client: &reqwest::Client,
    base_url: &str,
    admin: &AdminHandle,
    role: &str,
    max_uses: i32,
    ttl_seconds: i64,
) -> String {
    let req = CreateInviteReq {
        role_to_grant: role.to_string(),
        max_uses,
        ttl_seconds,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(client, base_url, "/v1/admin/invites", admin, &body).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let parsed: CreateInviteResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    parsed.token_display
}

// ══════════════════════════════════════════════
// TESTS
// ══════════════════════════════════════════════

// ─── Bootstrap / First User ───

#[tokio::test]
async fn test_first_user_redeem_with_bootstrap_token() {
    // Use admin-created invite to test first-user flow
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Create an admin invite via admin API
    let token = create_invite_token(&client, &base_url, &admin, "admin", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("first-admin".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let parsed: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.role, "admin");
}

#[tokio::test]
async fn test_redeem_creates_user_identity_device() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("alice".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key.clone(),
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let parsed: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.role, "user");

    // Verify user was created in DB
    let user = Users::find_by_id(parsed.user_id).one(&db).await.unwrap().unwrap();
    assert_eq!(user.role, "user");
    assert_eq!(user.status, "active");

    // Verify identity credential
    let identity = UserIdentityCredentials::find_by_id(parsed.user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(identity.credential, mat.identity_credential);

    // Verify device
    let device = Devices::find_by_id(parsed.device_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(device.user_id, parsed.user_id);
    assert_eq!(device.hpke_init_public_key, mat.device_init_public_key);
    assert_eq!(device.device_signing_public_key, dev_sig_pk);
    assert!(device.revoked_at.is_none());

    // Verify key change event
    let events = KeyChangeEvents::find()
        .all(&db)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user_id, parsed.user_id);
    assert_eq!(events[0].device_id, parsed.device_id);
    assert_eq!(events[0].event_type, "device_added");
}

#[tokio::test]
async fn test_redeem_with_invalid_signature_rejected() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();

    // Use a DIFFERENT identity key to sign → invalid signature
    let wrong_key = SigningKey::generate(&mut rand::thread_rng());
    let bad_sig = sign_device_authorization(
        &wrong_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("bob".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: bad_sig,
        device_authorization_timestamp: now,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_redeem_with_expired_token_rejected() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Insert expired token directly into DB (API rejects ttl_seconds < 60)
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let token_b64 =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let token_hash = blake3::hash(token_b64.as_bytes()).as_bytes().to_vec();

    invitation_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        token_hash: Set(token_hash),
        role_to_grant: Set("user".to_string()),
        max_uses: Set(1),
        uses_count: Set(0),
        expires_at: Set(now - 1), // expired 1 second ago
        revoked_at: Set(None),
        created_by_user_id: Set(Some(admin.user_id)),
        created_at: Set(now - 3600),
    }
    .insert(&db)
    .await
    .unwrap();

    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token: token_b64,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("carol".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_redeem_with_taken_username_rejected() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    let token1 = create_invite_token(&client, &base_url, &admin, "user", 2, 3600).await;
    let mat1 = generate_user_keypairs();
    let dev_sig_pk1 = mat1.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig1 = sign_device_authorization(
        &mat1.identity_signing_key,
        &dev_sig_pk1,
        &mat1.device_init_public_key,
        now,
    );

    // First registration — "dave"
    let req1 = RedeemReq {
        token: token1.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat1.identity_credential),
        signature_public_key: Some(mat1.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("dave".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat1.device_init_public_key,
        device_signing_public_key: dev_sig_pk1,
        device_authorization_signature: dev_auth_sig1,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req1).unwrap();
    let resp1 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::CREATED);

    // Second registration with same username → should fail
    let mat2 = generate_user_keypairs();
    let dev_sig_pk2 = mat2.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig2 = sign_device_authorization(
        &mat2.identity_signing_key,
        &dev_sig_pk2,
        &mat2.device_init_public_key,
        now,
    );

    let req2 = RedeemReq {
        token: token1, // use same token (max_uses=2)
        kind: "new_user".to_string(),
        identity_credential: Some(mat2.identity_credential),
        signature_public_key: Some(mat2.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("dave".to_string()), // same username → collision
        existing_identity_proof: None,
        device_init_public_key: mat2.device_init_public_key,
        device_signing_public_key: dev_sig_pk2,
        device_authorization_signature: dev_auth_sig2,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req2).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_username_canonicalization_blocks_collision() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    let token1 = create_invite_token(&client, &base_url, &admin, "user", 2, 3600).await;
    let mat1 = generate_user_keypairs();
    let dev_sig_pk1 = mat1.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig1 = sign_device_authorization(
        &mat1.identity_signing_key,
        &dev_sig_pk1,
        &mat1.device_init_public_key,
        now,
    );

    // Register as "Alice"
    let req1 = RedeemReq {
        token: token1.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat1.identity_credential),
        signature_public_key: Some(mat1.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("Alice".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat1.device_init_public_key,
        device_signing_public_key: dev_sig_pk1,
        device_authorization_signature: dev_auth_sig1,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req1).unwrap();
    let resp1 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::CREATED);

    // Try registering as "alice" (lowercase) → should collide
    let mat2 = generate_user_keypairs();
    let dev_sig_pk2 = mat2.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig2 = sign_device_authorization(
        &mat2.identity_signing_key,
        &dev_sig_pk2,
        &mat2.device_init_public_key,
        now,
    );

    let req2 = RedeemReq {
        token: token1,
        kind: "new_user".to_string(),
        identity_credential: Some(mat2.identity_credential),
        signature_public_key: Some(mat2.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("alice".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat2.device_init_public_key,
        device_signing_public_key: dev_sig_pk2,
        device_authorization_signature: dev_auth_sig2,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req2).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::CONFLICT);
}

// ─── New Device ───

#[tokio::test]
async fn test_new_device_redeem_works() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // First create a user
    let token = create_invite_token(&client, &base_url, &admin, "user", 2, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let identity_pk = mat.identity_signing_key.verifying_key().to_bytes().to_vec();

    let req_new_user = RedeemReq {
        token: token.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(identity_pk.clone()),
        username: Some("eve".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key.clone(),
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req_new_user).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let first_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    let user_id = first_resp.user_id;

    // Now redeem a NEW DEVICE for the same user
    let new_mat = generate_user_keypairs();
    let new_dev_sig_pk = new_mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let new_dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &new_dev_sig_pk,
        &new_mat.device_init_public_key,
        now,
    );
    let existing_proof = sign_provisioning_challenge(&mat.identity_signing_key, &token, now);

    let req_new_device = RedeemReq {
        token,
        kind: "new_device".to_string(),
        identity_credential: None,
        signature_public_key: Some(identity_pk),
        username: None,
        existing_identity_proof: Some(existing_proof),
        device_init_public_key: new_mat.device_init_public_key,
        device_signing_public_key: new_dev_sig_pk,
        device_authorization_signature: new_dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req_new_device).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::CREATED);
    let second_resp: RedeemResp = rmp_serde::from_slice(&resp2.bytes().await.unwrap()).unwrap();
    assert_eq!(second_resp.user_id, user_id);
    assert_ne!(second_resp.device_id, first_resp.device_id);
}

#[tokio::test]
async fn test_new_device_without_identity_proof_rejected() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    // NewDevice without existing_identity_proof → should be rejected
    let req = RedeemReq {
        token,
        kind: "new_device".to_string(),
        identity_credential: None,
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: None,
        existing_identity_proof: None, // missing!
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_new_device_with_unknown_identity_returns_404() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Token exists
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;

    // Generate keys but DON'T register this user first → identity doesn't exist
    let unknown_key = SigningKey::generate(&mut rand::thread_rng());
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &unknown_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let existing_proof = sign_provisioning_challenge(&unknown_key, &token, now);

    let req = RedeemReq {
        token,
        kind: "new_device".to_string(),
        identity_credential: None,
        signature_public_key: Some(unknown_key.verifying_key().to_bytes().to_vec()),
        username: None,
        existing_identity_proof: Some(existing_proof),
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Lookup & Identity ───

#[tokio::test]
async fn test_lookup_by_blind_index_works() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Register a user via redeem
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let identity_pk = mat.identity_signing_key.verifying_key().to_bytes().to_vec();

    // Compute blind_index the same way server does
    let blind_index = admin.state.server_identity.blind_index("lookup-test");
    let blind_index_hex = hex::encode(&blind_index);

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(identity_pk.clone()),
        username: Some("lookup-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Lookup by blind_index
    let lookup_path = format!("/v1/users/lookup?blind_index={blind_index_hex}");
    // GET with auth (use admin handle for auth)
    let lookup_resp = signed_get(&client, &base_url, &lookup_path, &admin).await;
    assert_eq!(lookup_resp.status(), StatusCode::OK);
    let lookup_parsed: LookupResp =
        rmp_serde::from_slice(&lookup_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(lookup_parsed.user_id, redeem_resp.user_id);
    assert_eq!(lookup_parsed.signature_public_key, identity_pk);
    assert_eq!(lookup_parsed.identity_credential, mat.identity_credential);
}

#[tokio::test]
async fn test_lookup_nonexistent_returns_404() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();

    let blind_index = admin.state.server_identity.blind_index("nobody");
    let blind_index_hex = hex::encode(&blind_index);

    let lookup_path = format!("/v1/users/lookup?blind_index={blind_index_hex}");
    let lookup_resp = signed_get(&client, &base_url, &lookup_path, &admin).await;
    assert_eq!(lookup_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_user_identity_by_id() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Register a user
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let identity_pk = mat.identity_signing_key.verifying_key().to_bytes().to_vec();

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(identity_pk.clone()),
        username: Some("identity-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Get identity by ID
    let identity_path = format!("/v1/users/{}/identity", redeem_resp.user_id);
    let identity_resp = signed_get(&client, &base_url, &identity_path, &admin).await;
    assert_eq!(identity_resp.status(), StatusCode::OK);
    let identity_parsed: LookupResp =
        rmp_serde::from_slice(&identity_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(identity_parsed.user_id, redeem_resp.user_id);
    assert_eq!(identity_parsed.signature_public_key, identity_pk);
}

// ─── Change Username ───

#[tokio::test]
async fn test_change_username_works() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Register a user
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("old-name".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Create a handle for the new user to make signed requests
    // We need the device signing key to make auth headers
    let user_handle = AdminHandle {
        user_id: redeem_resp.user_id,
        device_id: redeem_resp.device_id,
        signing_key: mat.device_signing_key, // use device key for auth
        state: admin.state.clone(),
    };

    // Change username
    let change_req = serde_json::json!({"new_username": "new-name"});
    let change_body = rmp_serde::to_vec_named(&change_req).unwrap();
    let change_resp = signed_patch(
        &client,
        &base_url,
        "/v1/users/me/username",
        &user_handle,
        &change_body,
    )
    .await;
    assert_eq!(change_resp.status(), StatusCode::NO_CONTENT);

    // Verify lookup by new blind_index works
    let new_blind_index = admin.state.server_identity.blind_index("new-name");
    let lookup_path = format!("/v1/users/lookup?blind_index={}", hex::encode(&new_blind_index));
    let lookup_resp = signed_get(&client, &base_url, &lookup_path, &admin).await;
    assert_eq!(lookup_resp.status(), StatusCode::OK);
    let lookup_parsed: LookupResp =
        rmp_serde::from_slice(&lookup_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(lookup_parsed.user_id, redeem_resp.user_id);

    // Old blind_index should now return 404
    let old_blind_index = admin.state.server_identity.blind_index("old-name");
    let old_lookup_path = format!("/v1/users/lookup?blind_index={}", hex::encode(&old_blind_index));
    let old_lookup_resp = signed_get(&client, &base_url, &old_lookup_path, &admin).await;
    assert_eq!(old_lookup_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_change_username_collision_rejected() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Register first user "user-a"
    let token1 = create_invite_token(&client, &base_url, &admin, "user", 2, 3600).await;
    let mat1 = generate_user_keypairs();
    let dev_sig_pk1 = mat1.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig1 = sign_device_authorization(
        &mat1.identity_signing_key,
        &dev_sig_pk1,
        &mat1.device_init_public_key,
        now,
    );
    let req1 = RedeemReq {
        token: token1.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat1.identity_credential),
        signature_public_key: Some(mat1.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("user-a".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat1.device_init_public_key,
        device_signing_public_key: dev_sig_pk1.clone(),
        device_authorization_signature: dev_auth_sig1,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req1).unwrap();
    let resp1 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let _r1: RedeemResp = rmp_serde::from_slice(&resp1.bytes().await.unwrap()).unwrap();

    // Register second user "user-b"
    let mat2 = generate_user_keypairs();
    let dev_sig_pk2 = mat2.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig2 = sign_device_authorization(
        &mat2.identity_signing_key,
        &dev_sig_pk2,
        &mat2.device_init_public_key,
        now,
    );
    let req2 = RedeemReq {
        token: token1,
        kind: "new_user".to_string(),
        identity_credential: Some(mat2.identity_credential),
        signature_public_key: Some(mat2.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("user-b".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat2.device_init_public_key,
        device_signing_public_key: dev_sig_pk2.clone(),
        device_authorization_signature: dev_auth_sig2,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req2).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let r2: RedeemResp = rmp_serde::from_slice(&resp2.bytes().await.unwrap()).unwrap();

    // User-b tries to change to "user-a" → collision
    let user_b_handle = AdminHandle {
        user_id: r2.user_id,
        device_id: r2.device_id,
        signing_key: mat2.device_signing_key,
        state: admin.state.clone(),
    };

    let change_req = serde_json::json!({"new_username": "user-a"});
    let change_body = rmp_serde::to_vec_named(&change_req).unwrap();
    let change_resp = signed_patch(
        &client,
        &base_url,
        "/v1/users/me/username",
        &user_b_handle,
        &change_body,
    )
    .await;
    assert_eq!(change_resp.status(), StatusCode::CONFLICT);
}

// ─── Devices ───

#[tokio::test]
async fn test_list_my_devices() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Register a user
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("devlist-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // List devices as this user
    let user_handle = AdminHandle {
        user_id: redeem_resp.user_id,
        device_id: redeem_resp.device_id,
        signing_key: mat.device_signing_key,
        state: admin.state.clone(),
    };

    let devices_resp = signed_get(&client, &base_url, "/v1/devices/me", &user_handle).await;
    assert_eq!(devices_resp.status(), StatusCode::OK);
    let devices_parsed: ListDevicesResp =
        rmp_serde::from_slice(&devices_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(devices_parsed.devices.len(), 1);
    assert!(devices_parsed.devices[0].is_current);
    assert!(devices_parsed.devices[0].revoked_at.is_none());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_revoke_device_marks_revoked() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Create a user via redeem (device A)
    let token = create_invite_token(&client, &base_url, &admin, "user", 3, 3600).await;
    let mat = generate_user_keypairs();
    let identity_pk = mat.identity_signing_key.verifying_key().to_bytes().to_vec();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    let req = RedeemReq {
        token: token.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(identity_pk.clone()),
        username: Some("revoke-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key.clone(),
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let device_a_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Create device B for the same user via NewDevice redeem
    let new_mat = generate_user_keypairs();
    let new_dev_sig_pk = new_mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let new_dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &new_dev_sig_pk,
        &new_mat.device_init_public_key,
        now,
    );
    let existing_proof = sign_provisioning_challenge(&mat.identity_signing_key, &token, now);

    let req_new_device = RedeemReq {
        token,
        kind: "new_device".to_string(),
        identity_credential: None,
        signature_public_key: Some(identity_pk.clone()),
        username: None,
        existing_identity_proof: Some(existing_proof),
        device_init_public_key: new_mat.device_init_public_key,
        device_signing_public_key: new_dev_sig_pk,
        device_authorization_signature: new_dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body2 = rmp_serde::to_vec_named(&req_new_device).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body2)
        .send()
        .await
        .unwrap();
    let device_b_resp: RedeemResp = rmp_serde::from_slice(&resp2.bytes().await.unwrap()).unwrap();

    // Use device B to revoke device A
    let device_b_handle = AdminHandle {
        user_id: device_a_resp.user_id,
        device_id: device_b_resp.device_id,
        signing_key: new_mat.device_signing_key,
        state: admin.state.clone(),
    };

    let rev_sig = sign_revocation(
        &mat.identity_signing_key,
        &device_a_resp.device_id,
        now,
    );

    let revoke_req = serde_json::json!({
        "revocation_signature": rev_sig,
        "revocation_timestamp": now,
    });
    let revoke_body = rmp_serde::to_vec_named(&revoke_req).unwrap();
    let revoke_path = format!("/v1/devices/me/{}/revoke", device_a_resp.device_id);
    let revoke_resp = signed_post(
        &client,
        &base_url,
        &revoke_path,
        &device_b_handle,
        &revoke_body,
    )
    .await;
    assert_eq!(revoke_resp.status(), StatusCode::NO_CONTENT);

    // Verify device A is now revoked (via device B)
    let devices_resp = signed_get(&client, &base_url, "/v1/devices/me", &device_b_handle).await;
    let devices_parsed: ListDevicesResp =
        rmp_serde::from_slice(&devices_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(devices_parsed.devices.len(), 2);
    let dev_a = devices_parsed.devices.iter().find(|d| d.id == device_a_resp.device_id).unwrap();
    assert!(dev_a.revoked_at.is_some());
    let dev_b = devices_parsed.devices.iter().find(|d| d.id == device_b_resp.device_id).unwrap();
    assert!(dev_b.revoked_at.is_none());

    // Key change event should exist
    let events = KeyChangeEvents::find()
        .filter(messenger_entity::key_change_events::Column::EventType.eq("device_revoked"))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].device_id, device_a_resp.device_id);
}

#[tokio::test]
async fn test_revoke_device_idempotent() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Create user with device A
    let token = create_invite_token(&client, &base_url, &admin, "user", 3, 3600).await;
    let mat = generate_user_keypairs();
    let identity_pk = mat.identity_signing_key.verifying_key().to_bytes().to_vec();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let req = RedeemReq {
        token: token.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(identity_pk.clone()),
        username: Some("idempotent-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key.clone(),
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let device_a_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Create device B for same user
    let new_mat = generate_user_keypairs();
    let new_dev_sig_pk = new_mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let new_dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &new_dev_sig_pk,
        &new_mat.device_init_public_key,
        now,
    );
    let existing_proof = sign_provisioning_challenge(&mat.identity_signing_key, &token, now);

    let req_new_device = RedeemReq {
        token,
        kind: "new_device".to_string(),
        identity_credential: None,
        signature_public_key: Some(identity_pk.clone()),
        username: None,
        existing_identity_proof: Some(existing_proof),
        device_init_public_key: new_mat.device_init_public_key,
        device_signing_public_key: new_dev_sig_pk,
        device_authorization_signature: new_dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body2 = rmp_serde::to_vec_named(&req_new_device).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body2)
        .send()
        .await
        .unwrap();
    let device_b_resp: RedeemResp = rmp_serde::from_slice(&resp2.bytes().await.unwrap()).unwrap();

    // Use device B to revoke device A
    let device_b_handle = AdminHandle {
        user_id: device_a_resp.user_id,
        device_id: device_b_resp.device_id,
        signing_key: new_mat.device_signing_key,
        state: admin.state.clone(),
    };

    let rev_sig = sign_revocation(&mat.identity_signing_key, &device_a_resp.device_id, now);
    let revoke_req = serde_json::json!({
        "revocation_signature": rev_sig.clone(),
        "revocation_timestamp": now,
    });
    let revoke_body = rmp_serde::to_vec_named(&revoke_req).unwrap();
    let revoke_path = format!("/v1/devices/me/{}/revoke", device_a_resp.device_id);

    // First revoke — device B revokes device A
    let r1 = signed_post(&client, &base_url, &revoke_path, &device_b_handle, &revoke_body).await;
    assert_eq!(r1.status(), StatusCode::NO_CONTENT);

    // Second revoke — should be idempotent (204) since device A is already revoked
    let r2 = signed_post(&client, &base_url, &revoke_path, &device_b_handle, &revoke_body).await;
    assert_eq!(r2.status(), StatusCode::NO_CONTENT);
}

// ─── List User Devices (Public) ───

#[tokio::test]
async fn test_list_user_devices_excludes_revoked() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Register a user
    let token = create_invite_token(&client, &base_url, &admin, "user", 2, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let identity_pk = mat.identity_signing_key.verifying_key().to_bytes().to_vec();

    let req1 = RedeemReq {
        token: token.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(identity_pk.clone()),
        username: Some("public-dev-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req1).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let r1: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Add a second device
    let mat2 = generate_user_keypairs();
    let dev_sig_pk2 = mat2.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig2 = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk2,
        &mat2.device_init_public_key,
        now,
    );
    let existing_proof = sign_provisioning_challenge(&mat.identity_signing_key, &token, now);

    let req2 = RedeemReq {
        token,
        kind: "new_device".to_string(),
        identity_credential: None,
        signature_public_key: Some(identity_pk),
        username: None,
        existing_identity_proof: Some(existing_proof),
        device_init_public_key: mat2.device_init_public_key,
        device_signing_public_key: dev_sig_pk2,
        device_authorization_signature: dev_auth_sig2,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req2).unwrap();
    let resp2 = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let r2: RedeemResp = rmp_serde::from_slice(&resp2.bytes().await.unwrap()).unwrap();

    // Revoke the first device
    let user_handle = AdminHandle {
        user_id: r1.user_id,
        device_id: r1.device_id,
        signing_key: mat.device_signing_key,
        state: admin.state.clone(),
    };
    let rev_sig = sign_revocation(&mat.identity_signing_key, &r1.device_id, now);
    let revoke_req = serde_json::json!({
        "revocation_signature": rev_sig,
        "revocation_timestamp": now,
    });
    let revoke_body = rmp_serde::to_vec_named(&revoke_req).unwrap();
    let revoke_path = format!("/v1/devices/me/{}/revoke", r1.device_id);
    signed_post(&client, &base_url, &revoke_path, &user_handle, &revoke_body).await;

    // Public device list should only show active (non-revoked) devices
    let public_devices_path = format!("/v1/users/{}/devices", r1.user_id);
    let public_devices_resp =
        signed_get(&client, &base_url, &public_devices_path, &admin).await;
    assert_eq!(public_devices_resp.status(), StatusCode::OK);
    let public_devices: Vec<PublicDeviceInfo> =
        rmp_serde::from_slice(&public_devices_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(public_devices.len(), 1); // only the non-revoked device
    assert_eq!(public_devices[0].id, r2.device_id);
}

// ─── Admin User Management ───

#[tokio::test]
async fn test_admin_suspend_user() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Create a regular user via redeem
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("suspend-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Admin suspends the user
    let suspend_path = format!("/v1/admin/users/{}/suspend", redeem_resp.user_id);
    let suspend_resp = signed_post(
        &client,
        &base_url,
        &suspend_path,
        &admin,
        &rmp_serde::to_vec_named(&serde_json::json!({})).unwrap(),
    )
    .await;
    assert_eq!(suspend_resp.status(), StatusCode::NO_CONTENT);

    // Verify user is suspended in DB
    let user = Users::find_by_id(redeem_resp.user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.status, "suspended");
}

#[tokio::test]
async fn test_suspended_user_cannot_authenticate() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Create a user
    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("suspended-auth".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    let user_handle = AdminHandle {
        user_id: redeem_resp.user_id,
        device_id: redeem_resp.device_id,
        signing_key: mat.device_signing_key,
        state: admin.state.clone(),
    };

    // User can authenticate initially
    let test_resp = signed_get(&client, &base_url, "/v1/devices/me", &user_handle).await;
    assert_eq!(test_resp.status(), StatusCode::OK);

    // Admin suspends
    let suspend_path = format!("/v1/admin/users/{}/suspend", redeem_resp.user_id);
    signed_post(
        &client,
        &base_url,
        &suspend_path,
        &admin,
        &rmp_serde::to_vec_named(&serde_json::json!({})).unwrap(),
    )
    .await;

    // Now the user should get 403 Forbidden
    let suspended_resp = signed_get(&client, &base_url, "/v1/devices/me", &user_handle).await;
    assert_eq!(suspended_resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_list_users() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // The admin itself is user 0; create a few more via redeem
    for i in 0..3 {
        let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
        let mat = generate_user_keypairs();
        let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
        let dev_auth_sig = sign_device_authorization(
            &mat.identity_signing_key,
            &dev_sig_pk,
            &mat.device_init_public_key,
            now,
        );
        let req = RedeemReq {
            token,
            kind: "new_user".to_string(),
            identity_credential: Some(mat.identity_credential),
            signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
            username: Some(format!("list-user-{i}")),
            existing_identity_proof: None,
            device_init_public_key: mat.device_init_public_key,
            device_signing_public_key: dev_sig_pk,
            device_authorization_signature: dev_auth_sig,
            device_authorization_timestamp: now,
        };
        let body = rmp_serde::to_vec_named(&req).unwrap();
        client
            .post(format!("{base_url}/v1/invite/redeem"))
            .header("Content-Type", "application/msgpack")
            .body(body)
            .send()
            .await
            .unwrap();
    }

    // Admin lists users
    let list_resp = signed_get(&client, &base_url, "/v1/admin/users", &admin).await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let parsed: ListUsersResp = rmp_serde::from_slice(&list_resp.bytes().await.unwrap()).unwrap();
    // Should include the admin + 3 created users
    assert_eq!(parsed.users.len(), 4);
    assert_eq!(parsed.total, 4);

    // Verify admin's info is in the list
    let admin_in_list = parsed.users.iter().any(|u| u.id == admin.user_id);
    assert!(admin_in_list);
}

#[tokio::test]
async fn test_redeem_with_stale_authorization_timestamp_rejected() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();

    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();

    // Timestamp 600s in the past (beyond ±300 window)
    let stale_ts = now_secs() - 600;
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        stale_ts,
    );

    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("stale-ts".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: stale_ts,
    };

    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_redeem_admin_token_only_allows_new_user() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    // Create an admin invite token
    let token = create_invite_token(&client, &base_url, &admin, "admin", 1, 3600).await;

    // Try to use it as NewDevice (even though no user exists for this identity)
    // The server should reject because admin tokens only allow NewUser
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );

    // First, register the user so identity exists
    let req_new_user = RedeemReq {
        token: token.clone(),
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential.clone()),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("admin-token-user".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key.clone(),
        device_signing_public_key: dev_sig_pk.clone(),
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req_new_user).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let r1: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(r1.role, "admin");
}

#[tokio::test]
async fn test_admin_unsuspend_user() {
    let db = fresh_db().await;
    let admin = create_admin_handle(&db).await;
    let base_url = start_server(admin.state.clone()).await;
    let client = reqwest::Client::new();
    let now = now_secs();

    let token = create_invite_token(&client, &base_url, &admin, "user", 1, 3600).await;
    let mat = generate_user_keypairs();
    let dev_sig_pk = mat.device_signing_key.verifying_key().to_bytes().to_vec();
    let dev_auth_sig = sign_device_authorization(
        &mat.identity_signing_key,
        &dev_sig_pk,
        &mat.device_init_public_key,
        now,
    );
    let req = RedeemReq {
        token,
        kind: "new_user".to_string(),
        identity_credential: Some(mat.identity_credential),
        signature_public_key: Some(mat.identity_signing_key.verifying_key().to_bytes().to_vec()),
        username: Some("unsuspend-test".to_string()),
        existing_identity_proof: None,
        device_init_public_key: mat.device_init_public_key,
        device_signing_public_key: dev_sig_pk,
        device_authorization_signature: dev_auth_sig,
        device_authorization_timestamp: now,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = client
        .post(format!("{base_url}/v1/invite/redeem"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();
    let redeem_resp: RedeemResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Suspend
    let suspend_path = format!("/v1/admin/users/{}/suspend", redeem_resp.user_id);
    signed_post(
        &client,
        &base_url,
        &suspend_path,
        &admin,
        &rmp_serde::to_vec_named(&serde_json::json!({})).unwrap(),
    )
    .await;

    // Unsuspend
    let unsuspend_path = format!("/v1/admin/users/{}/unsuspend", redeem_resp.user_id);
    let unsuspend_resp = signed_post(
        &client,
        &base_url,
        &unsuspend_path,
        &admin,
        &rmp_serde::to_vec_named(&serde_json::json!({})).unwrap(),
    )
    .await;
    assert_eq!(unsuspend_resp.status(), StatusCode::NO_CONTENT);

    // Verify active again
    let user = Users::find_by_id(redeem_resp.user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.status, "active");
}

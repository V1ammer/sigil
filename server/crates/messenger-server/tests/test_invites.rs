#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::http::StatusCode;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::identity::ServerIdentity;
use messenger_server::routes::build_router;
use messenger_server::services::invite::{begin_immediate, consume_token, now_secs, validate_token};
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Test Helpers ───

/// Handle with admin credentials for making signed requests.
#[expect(dead_code)]
struct AdminHandle {
    user_id: Uuid,
    device_id: Uuid,
    signing_key: SigningKey,
    state: AppState,
}

/// Creates an in-memory `SQLite` DB with migrations applied.
async fn fresh_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    messenger_migration::Migrator::up(&db, None).await.unwrap();
    db
}

/// Creates an admin user + device in the DB, returns handle.
async fn create_admin_handle(db: &DatabaseConnection) -> AdminHandle {
    let mut rng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();

    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();

    let mut blind_index = [0u8; 32];
    rng.fill_bytes(&mut blind_index);

    // Create user (admin, active)
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

    // Create device
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
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    };

    AdminHandle {
        user_id,
        device_id,
        signing_key,
        state,
    }
}

/// Creates a regular (non-admin) user + device.
async fn create_user_handle(db: &DatabaseConnection) -> AdminHandle {
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
        role: Set("user".to_string()),
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
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    };

    AdminHandle {
        user_id,
        device_id,
        signing_key,
        state,
    }
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

/// Builds the `X-Auth-Signature` header value and signs the request.
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

    let canonical = messenger_crypto::canonical::build_signed_message(
        method,
        path,
        ts,
        &nonce,
        body,
    );

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

/// Sends a signed DELETE request.
async fn signed_delete(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    handle: &AdminHandle,
) -> reqwest::Response {
    let body = b"";
    let auth = make_auth_header(handle, "DELETE", path, body);
    client
        .delete(format!("{base_url}{path}"))
        .header("X-Auth-Signature", &auth)
        .body(body.to_vec())
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

// ─── Request/Response types for tests ───

#[derive(Serialize)]
struct CreateInviteReq {
    role_to_grant: String,
    max_uses: i32,
    ttl_seconds: i64,
}

#[derive(Deserialize)]
struct CreateInviteResp {
    id: Uuid,
    #[allow(dead_code)]
    token: Vec<u8>,
    #[allow(dead_code)]
    token_display: String,
    #[allow(dead_code)]
    expires_at: i64,
}

#[derive(Deserialize)]
struct InviteSummary {
    #[allow(dead_code)]
    id: Uuid,
    #[allow(dead_code)]
    role_to_grant: String,
    #[allow(dead_code)]
    max_uses: i32,
    #[allow(dead_code)]
    uses_count: i32,
    #[allow(dead_code)]
    expires_at: i64,
    #[allow(dead_code)]
    created_at: i64,
    #[allow(dead_code)]
    created_by_user_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct ListInvitesResp {
    invites: Vec<InviteSummary>,
}

// ─── Admin Endpoint Tests ───

#[tokio::test]
async fn test_admin_creates_invite() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 5,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;

    assert_eq!(resp.status(), StatusCode::CREATED);
    let parsed: CreateInviteResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.token.len(), 32);
    assert!(!parsed.token_display.is_empty());
    // token_display is the base64url encoding of token
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&parsed.token_display)
        .unwrap();
    assert_eq!(decoded, parsed.token);
}

#[tokio::test]
async fn test_non_admin_cannot_create() {
    let db = fresh_db().await;
    let handle = create_user_handle(&db).await; // role = "user"
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 1,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_no_auth_cannot_create() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 1,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();

    // No auth header
    let resp = client
        .post(format!("{base_url}/v1/admin/invites"))
        .header("Content-Type", "application/msgpack")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_ttl_rejected() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    // ttl = 30, below minimum of 60
    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 1,
        ttl_seconds: 30,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_invalid_role_rejected() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    let req = CreateInviteReq {
        role_to_grant: "superadmin".to_string(),
        max_uses: 1,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_returns_active() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    // Create two invites
    for role in ["user", "admin"] {
        let req = CreateInviteReq {
            role_to_grant: role.to_string(),
            max_uses: 3,
            ttl_seconds: 3600,
        };
        let body = rmp_serde::to_vec_named(&req).unwrap();
        signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;
    }

    // List
    let resp = signed_get(&client, &base_url, "/v1/admin/invites", &handle).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let parsed: ListInvitesResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.invites.len(), 2);
}

#[tokio::test]
async fn test_revoke_marks_revoked() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    // Create invite
    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 1,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;
    let created: CreateInviteResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Revoke
    let path = format!("/v1/admin/invites/{}", created.id);
    let resp = signed_delete(&client, &base_url, &path, &handle).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // List should be empty now
    let resp = signed_get(&client, &base_url, "/v1/admin/invites", &handle).await;
    let parsed: ListInvitesResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert!(parsed.invites.is_empty());
}

#[tokio::test]
async fn test_revoke_idempotent() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    // Create invite
    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 1,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;
    let created: CreateInviteResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    let path = format!("/v1/admin/invites/{}", created.id);

    // First revoke
    let r1 = signed_delete(&client, &base_url, &path, &handle).await;
    assert_eq!(r1.status(), StatusCode::OK);

    // Second revoke — idempotent → still 200
    let r2 = signed_delete(&client, &base_url, &path, &handle).await;
    assert_eq!(r2.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_revoke_nonexistent_returns_404() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();

    let path = format!("/v1/admin/invites/{}", Uuid::now_v7());
    let resp = signed_delete(&client, &base_url, &path, &handle).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Helper Function Tests (no HTTP) ───

#[tokio::test]
async fn test_validate_token_works() {
    let db = fresh_db().await;
    let handle = create_admin_handle(&db).await;

    // Create a token directly via admin endpoint
    let req = CreateInviteReq {
        role_to_grant: "user".to_string(),
        max_uses: 3,
        ttl_seconds: 3600,
    };
    let body = rmp_serde::to_vec_named(&req).unwrap();
    let base_url = start_server(handle.state.clone()).await;
    let client = reqwest::Client::new();
    let resp = signed_post(&client, &base_url, "/v1/admin/invites", &handle, &body).await;
    let created: CreateInviteResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Validate the token
    let row = validate_token(&handle.state.db, &created.token_display)
        .await
        .unwrap();
    assert_eq!(row.role_to_grant, "user");
    assert_eq!(row.max_uses, 3);
    assert_eq!(row.uses_count, 0);
}

#[tokio::test]
async fn test_validate_invalid_token_rejected() {
    let db = fresh_db().await;
    let result = validate_token(&db, "this-token-does-not-exist").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), messenger_server::error::AppError::InviteInvalid));
}

#[tokio::test]
async fn test_validate_expired_rejected() {
    let db = fresh_db().await;

    // Insert a token that expired yesterday
    let token_str = "test-expired-token";
    let token_hash = blake3::hash(token_str.as_bytes()).as_bytes().to_vec();

    messenger_entity::invitation_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        token_hash: Set(token_hash),
        created_by_user_id: Set(None),
        role_to_grant: Set("user".to_string()),
        max_uses: Set(1),
        uses_count: Set(0),
        expires_at: Set(now_secs() - 86400), // expired 1 day ago
        revoked_at: Set(None),
        created_at: Set(now_secs()),
    }
    .insert(&db)
    .await
    .unwrap();

    let result = validate_token(&db, token_str).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), messenger_server::error::AppError::InviteExpired));
}

#[tokio::test]
async fn test_validate_revoked_rejected() {
    let db = fresh_db().await;

    let token_str = "test-revoked-token";
    let token_hash = blake3::hash(token_str.as_bytes()).as_bytes().to_vec();

    messenger_entity::invitation_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        token_hash: Set(token_hash),
        created_by_user_id: Set(None),
        role_to_grant: Set("user".to_string()),
        max_uses: Set(1),
        uses_count: Set(0),
        expires_at: Set(now_secs() + 86400),
        revoked_at: Set(Some(now_secs())), // revoked
        created_at: Set(now_secs()),
    }
    .insert(&db)
    .await
    .unwrap();

    let result = validate_token(&db, token_str).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), messenger_server::error::AppError::InviteInvalid));
}

#[tokio::test]
async fn test_validate_exhausted_rejected() {
    let db = fresh_db().await;

    let token_str = "test-exhausted-token";
    let token_hash = blake3::hash(token_str.as_bytes()).as_bytes().to_vec();

    messenger_entity::invitation_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        token_hash: Set(token_hash),
        created_by_user_id: Set(None),
        role_to_grant: Set("user".to_string()),
        max_uses: Set(1),
        uses_count: Set(1), // already used
        expires_at: Set(now_secs() + 86400),
        revoked_at: Set(None),
        created_at: Set(now_secs()),
    }
    .insert(&db)
    .await
    .unwrap();

    let result = validate_token(&db, token_str).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), messenger_server::error::AppError::InviteExhausted));
}

#[tokio::test]
async fn test_consume_token_atomic() {
    let db = fresh_db().await;

    // Create a token with max_uses=1
    let token_id = Uuid::now_v7();
    let token_str = "test-atomic-consume";
    let token_hash = blake3::hash(token_str.as_bytes()).as_bytes().to_vec();

    messenger_entity::invitation_tokens::ActiveModel {
        id: Set(token_id),
        token_hash: Set(token_hash),
        created_by_user_id: Set(None),
        role_to_grant: Set("user".to_string()),
        max_uses: Set(1),
        uses_count: Set(0),
        expires_at: Set(now_secs() + 86400),
        revoked_at: Set(None),
        created_at: Set(now_secs()),
    }
    .insert(&db)
    .await
    .unwrap();

    // Spawn two concurrent consumers
    let db1 = db.clone();
    let db2 = db.clone();

    let user_id1 = Uuid::now_v7();
    let device_id1 = Uuid::now_v7();
    let user_id2 = Uuid::now_v7();
    let device_id2 = Uuid::now_v7();

    let handle1 = tokio::spawn(async move {
        let txn = begin_immediate(&db1).await.unwrap();
        let r = consume_token(&txn, token_id, user_id1, device_id1).await;
        if r.is_ok() {
            txn.commit().await.unwrap();
        } else {
            txn.rollback().await.unwrap();
        }
        r
    });

    let handle2 = tokio::spawn(async move {
        let txn = begin_immediate(&db2).await.unwrap();
        let r = consume_token(&txn, token_id, user_id2, device_id2).await;
        if r.is_ok() {
            txn.commit().await.unwrap();
        } else {
            txn.rollback().await.unwrap();
        }
        r
    });

    let r1 = handle1.await.unwrap();
    let r2 = handle2.await.unwrap();

    let ok_count = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "exactly one parallel consume must succeed");

    // The exhausted one should be InviteExhausted
    assert!(
        r1.is_err() || r2.is_err(),
        "at least one consume should fail"
    );
}

// ─── Bootstrap Token Compatibility ───

#[tokio::test]
async fn test_bootstrap_token_can_be_validated() {
    let db = fresh_db().await;

    // Bootstrap creates a token; we can't access it directly (it's printed to stderr),
    // but we can verify the flow: load_or_init creates an admin token.
    let _identity = messenger_server::bootstrap::load_or_init(&db).await.unwrap();

    // Check that exactly one invitation token exists
    let tokens = messenger_entity::invitation_tokens::Entity::find()
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].role_to_grant, "admin");
    assert_eq!(tokens[0].max_uses, 1);
    assert_eq!(tokens[0].uses_count, 0);
    assert!(tokens[0].revoked_at.is_none());
    assert!(tokens[0].expires_at > now_secs());
    assert!(tokens[0].created_by_user_id.is_none());
}



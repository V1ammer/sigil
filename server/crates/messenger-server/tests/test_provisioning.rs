#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::http::StatusCode;
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use sea_orm::{ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set};

use messenger_crypto::canonical::build_signed_message;
use messenger_entity::device_provisioning_requests;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::identity::ServerIdentity;
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Test Helpers ───

struct UserHandle {
    user_id: Uuid,
    device_id: Uuid,
    identity_signing_key: SigningKey,
    device_signing_key: SigningKey,
    state: AppState,
}

struct NewDeviceTempKeys {
    temp_signing_key: SigningKey,
    temp_signing_pk: Vec<u8>,
    temp_x25519_pk: Vec<u8>,
    nonce: Vec<u8>,
}

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

/// Creates a user with one device in the DB, returns a handle with signing material.
async fn create_user_with_device(db: &DatabaseConnection) -> UserHandle {
    let mut rng = rand::thread_rng();
    let identity_signing_key = SigningKey::generate(&mut rng);
    let identity_vk = identity_signing_key.verifying_key();
    let device_signing_key = SigningKey::generate(&mut rng);
    let device_vk = device_signing_key.verifying_key();

    let user_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();

    let mut blind_index = [0u8; 32];
    rng.fill_bytes(&mut blind_index);

    let now = now_secs();

    // Create user
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

    // Create identity credential
    messenger_entity::user_identity_credentials::ActiveModel {
        user_id: Set(user_id),
        signature_public_key: Set(identity_vk.to_bytes().to_vec()),
        credential: Set(vec![1u8, 2, 3, 4]),
        created_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();

    // Create device
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

    UserHandle {
        user_id,
        device_id,
        identity_signing_key,
        device_signing_key,
        state,
    }
}

/// Generates temporary keys for a "new device".
fn generate_temp_keys() -> NewDeviceTempKeys {
    let mut rng = rand::thread_rng();
    let temp_signing_key = SigningKey::generate(&mut rng);
    let temp_signing_pk = temp_signing_key.verifying_key().to_bytes().to_vec();
    let mut temp_x25519_pk = vec![0u8; 32];
    rng.fill_bytes(&mut temp_x25519_pk);
    let mut nonce = vec![0u8; 16];
    rng.fill_bytes(&mut nonce);

    NewDeviceTempKeys {
        temp_signing_key,
        temp_signing_pk,
        temp_x25519_pk,
        nonce,
    }
}

/// Builds the X-Auth-Signature header value.
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

/// Builds the X-Provisioning-Signature header value for the bootstrap polling endpoint.
fn make_provisioning_signature(
    temp_signing_key: &SigningKey,
    path: &str,
    body: &[u8],
) -> String {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 16];
    rng.fill_bytes(&mut nonce);
    let ts = now_secs();

    let canonical = build_signed_message("GET", path, ts, &nonce, body);
    let signature = temp_signing_key.sign(&canonical);

    format!(
        "{}:{}:{}",
        ts,
        hex::encode(nonce),
        hex::encode(signature.to_bytes()),
    )
}

/// Signs device authorization message: `device_signing_pk` || `device_init_pk` || `ts_le`
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

// ─── Request/Response types ───

#[derive(Serialize)]
struct CreateProvisioningReq {
    user_id: Uuid,
    #[serde(with = "serde_bytes")]
    new_device_temp_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    new_device_temp_signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    nonce: Vec<u8>,
}

#[derive(Deserialize)]
struct CreateProvisioningResp {
    #[allow(dead_code)]
    provisioning_id: Uuid,
    #[allow(dead_code)]
    expires_at: i64,
}

#[derive(Deserialize)]
struct ProvisioningDetails {
    #[allow(dead_code)]
    provisioning_id: Uuid,
    #[allow(dead_code)]
    user_id: Uuid,
    #[allow(dead_code)]
    status: String,
}

#[derive(Serialize)]
struct ApproveProvisioningReq {
    #[serde(with = "serde_bytes")]
    encrypted_bootstrap_blob: Vec<u8>,
    #[serde(with = "serde_bytes")]
    new_device_hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    new_device_signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    device_authorization_signature: Vec<u8>,
    device_authorization_timestamp: i64,
}

#[derive(Deserialize)]
struct BootstrapResp {
    #[allow(dead_code)]
    new_device_id: Uuid,
    #[allow(dead_code)]
    encrypted_bootstrap_blob: Vec<u8>,
}

#[derive(Deserialize)]
struct DeviceInfo {
    #[allow(dead_code)]
    id: Uuid,
    #[allow(dead_code)]
    is_current: bool,
}

#[derive(Deserialize)]
struct ListDevicesResp {
    #[allow(dead_code)]
    devices: Vec<DeviceInfo>,
}

// ─── Tests ───

async fn send_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    body_bytes: Option<Vec<u8>>,
    auth_header: Option<&str>,
    provisioning_auth_header: Option<&str>,
) -> reqwest::Response {
    let mut req = client.request(method, url);
    if let Some(body) = body_bytes {
        req = req
            .header("Content-Type", "application/msgpack")
            .body(body);
    }
    if let Some(auth) = auth_header {
        req = req.header("X-Auth-Signature", auth);
    }
    if let Some(prov_auth) = provisioning_auth_header {
        req = req.header("X-Provisioning-Signature", prov_auth);
    }
    req.send().await.unwrap()
}

/// Helper: creates a provisioning request and returns (`provisioning_id`, `temp_keys`)
async fn create_provisioning_request(
    client: &reqwest::Client,
    base_url: &str,
    user_handle: &UserHandle,
) -> (Uuid, NewDeviceTempKeys) {
    let temp_keys = generate_temp_keys();
    let req_body = CreateProvisioningReq {
        user_id: user_handle.user_id,
        new_device_temp_public_key: temp_keys.temp_x25519_pk.clone(),
        new_device_temp_signing_public_key: temp_keys.temp_signing_pk.clone(),
        nonce: temp_keys.nonce.clone(),
    };
    let req_bytes = rmp_serde::to_vec_named(&req_body).unwrap();
    let resp = send_request(
        client,
        reqwest::Method::POST,
        &format!("{base_url}/v1/provisioning/requests"),
        Some(req_bytes),
        None,
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let create_resp: CreateProvisioningResp =
        rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    (create_resp.provisioning_id, temp_keys)
}

#[tokio::test]
async fn test_full_provisioning_flow() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();
    let mut rng = rand::thread_rng();

    // 1. New device generates temp keys and creates provisioning request
    let (provisioning_id, temp_keys) = create_provisioning_request(&client, &base_url, &user).await;

    // 2. Old device views the provisioning request
    let path = format!("/v1/provisioning/requests/{provisioning_id}");
    let auth = make_auth_header(&user.device_signing_key, &user.device_id, "GET", &path, b"");
    let resp = send_request(
        &client,
        reqwest::Method::GET,
        &format!("{base_url}{path}"),
        None,
        Some(&auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let details: ProvisioningDetails =
        rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(details.status, "pending");
    assert_eq!(details.user_id, user.user_id);

    // 3. Old device approves
    let new_device_signing_key = SigningKey::generate(&mut rng);
    let new_device_signing_pk = new_device_signing_key.verifying_key().to_bytes().to_vec();
    let mut new_device_hpke_pk = vec![0u8; 32];
    rng.fill_bytes(&mut new_device_hpke_pk);

    let auth_ts = now_secs();
    let auth_sig = sign_device_authorization(
        &user.identity_signing_key,
        &new_device_signing_pk,
        &new_device_hpke_pk,
        auth_ts,
    );

    let bootstrap_blob = vec![1u8, 2, 3, 4, 5]; // mock encrypted blob

    let approve_req = ApproveProvisioningReq {
        encrypted_bootstrap_blob: bootstrap_blob.clone(),
        new_device_hpke_public_key: new_device_hpke_pk.clone(),
        new_device_signing_public_key: new_device_signing_pk.clone(),
        device_authorization_signature: auth_sig,
        device_authorization_timestamp: auth_ts,
    };
    let approve_bytes = rmp_serde::to_vec_named(&approve_req).unwrap();
    let approve_path = format!("/v1/provisioning/requests/{provisioning_id}/approve");
    let approve_auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &approve_path,
        &approve_bytes,
    );
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}{approve_path}"),
        Some(approve_bytes),
        Some(&approve_auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 4. New device polls and gets the blob
    let bootstrap_path = format!("/v1/provisioning/requests/{provisioning_id}/bootstrap");
    let prov_sig = make_provisioning_signature(&temp_keys.temp_signing_key, &bootstrap_path, b"");
    let resp = send_request(
        &client,
        reqwest::Method::GET,
        &format!("{base_url}{bootstrap_path}"),
        None,
        None,
        Some(&prov_sig),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bootstrap: BootstrapResp =
        rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(bootstrap.encrypted_bootstrap_blob, vec![1u8, 2, 3, 4, 5]);

    // 5. Verify the user now has 2 devices
    let devices_path = "/v1/devices/me".to_string();
    let devices_auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "GET",
        &devices_path,
        b"",
    );
    let resp = send_request(
        &client,
        reqwest::Method::GET,
        &format!("{base_url}{devices_path}"),
        None,
        Some(&devices_auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let devices_list: ListDevicesResp =
        rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(devices_list.devices.len(), 2);
}

#[tokio::test]
async fn test_polling_before_approve_returns_202() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let (provisioning_id, temp_keys) = create_provisioning_request(&client, &base_url, &user).await;

    // Poll before approve → 202 Accepted
    let bootstrap_path = format!("/v1/provisioning/requests/{provisioning_id}/bootstrap");
    let prov_sig = make_provisioning_signature(&temp_keys.temp_signing_key, &bootstrap_path, b"");
    let resp = send_request(
        &client,
        reqwest::Method::GET,
        &format!("{base_url}{bootstrap_path}"),
        None,
        None,
        Some(&prov_sig),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_polling_after_consumed_returns_error() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();
    let mut rng = rand::thread_rng();

    let (provisioning_id, temp_keys) = create_provisioning_request(&client, &base_url, &user).await;

    // Approve
    let new_device_signing_key = SigningKey::generate(&mut rng);
    let new_device_signing_pk = new_device_signing_key.verifying_key().to_bytes().to_vec();
    let mut new_device_hpke_pk = vec![0u8; 32];
    rng.fill_bytes(&mut new_device_hpke_pk);

    let auth_ts = now_secs();
    let auth_sig = sign_device_authorization(
        &user.identity_signing_key,
        &new_device_signing_pk,
        &new_device_hpke_pk,
        auth_ts,
    );

    let approve_req = ApproveProvisioningReq {
        encrypted_bootstrap_blob: vec![1u8, 2, 3],
        new_device_hpke_public_key: new_device_hpke_pk,
        new_device_signing_public_key: new_device_signing_pk,
        device_authorization_signature: auth_sig,
        device_authorization_timestamp: auth_ts,
    };
    let approve_bytes = rmp_serde::to_vec_named(&approve_req).unwrap();
    let approve_path = format!("/v1/provisioning/requests/{provisioning_id}/approve");
    let approve_auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &approve_path,
        &approve_bytes,
    );
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}{approve_path}"),
        Some(approve_bytes),
        Some(&approve_auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // First poll → OK
    let bootstrap_path = format!("/v1/provisioning/requests/{provisioning_id}/bootstrap");
    let prov_sig = make_provisioning_signature(&temp_keys.temp_signing_key, &bootstrap_path, b"");
    let resp = send_request(
        &client,
        reqwest::Method::GET,
        &format!("{base_url}{bootstrap_path}"),
        None,
        None,
        Some(&prov_sig),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Second poll → 410 Gone (status is now consumed)
    let prov_sig2 =
        make_provisioning_signature(&temp_keys.temp_signing_key, &bootstrap_path, b"");
    let resp = send_request(
        &client,
        reqwest::Method::GET,
        &format!("{base_url}{bootstrap_path}"),
        None,
        None,
        Some(&prov_sig2),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_approve_by_wrong_user_rejected() {
    let db = fresh_db().await;
    let user_a = create_user_with_device(&db).await;
    let user_b = create_user_with_device(&db).await;
    let base_url = start_server(user_a.state.clone()).await;
    let client = reqwest::Client::new();
    let mut rng = rand::thread_rng();

    // Create provisioning request for user A
    let (provisioning_id, _temp_keys) =
        create_provisioning_request(&client, &base_url, &user_a).await;

    // User B tries to approve → 403
    let new_device_signing_key = SigningKey::generate(&mut rng);
    let new_device_signing_pk = new_device_signing_key.verifying_key().to_bytes().to_vec();
    let mut new_device_hpke_pk = vec![0u8; 32];
    rng.fill_bytes(&mut new_device_hpke_pk);

    let auth_ts = now_secs();
    let auth_sig = sign_device_authorization(
        &user_b.identity_signing_key,
        &new_device_signing_pk,
        &new_device_hpke_pk,
        auth_ts,
    );

    let approve_req = ApproveProvisioningReq {
        encrypted_bootstrap_blob: vec![1u8, 2, 3],
        new_device_hpke_public_key: new_device_hpke_pk,
        new_device_signing_public_key: new_device_signing_pk,
        device_authorization_signature: auth_sig,
        device_authorization_timestamp: auth_ts,
    };
    let approve_bytes = rmp_serde::to_vec_named(&approve_req).unwrap();
    let approve_path = format!("/v1/provisioning/requests/{provisioning_id}/approve");
    let approve_auth = make_auth_header(
        &user_b.device_signing_key,
        &user_b.device_id,
        "POST",
        &approve_path,
        &approve_bytes,
    );
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}{approve_path}"),
        Some(approve_bytes),
        Some(&approve_auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_approve_with_invalid_signature_rejected() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let (provisioning_id, _temp_keys) =
        create_provisioning_request(&client, &base_url, &user).await;

    let approve_req = ApproveProvisioningReq {
        encrypted_bootstrap_blob: vec![1u8, 2, 3],
        new_device_hpke_public_key: vec![0u8; 32],
        new_device_signing_public_key: vec![0u8; 32],
        device_authorization_signature: vec![0u8; 64], // invalid signature
        device_authorization_timestamp: now_secs(),
    };
    let approve_bytes = rmp_serde::to_vec_named(&approve_req).unwrap();
    let approve_path = format!("/v1/provisioning/requests/{provisioning_id}/approve");
    let approve_auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &approve_path,
        &approve_bytes,
    );
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}{approve_path}"),
        Some(approve_bytes),
        Some(&approve_auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_expired_provisioning_request_rejected() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let temp_keys = generate_temp_keys();
    let req_body = CreateProvisioningReq {
        user_id: user.user_id,
        new_device_temp_public_key: temp_keys.temp_x25519_pk,
        new_device_temp_signing_public_key: temp_keys.temp_signing_pk,
        nonce: temp_keys.nonce,
    };
    let req_bytes = rmp_serde::to_vec_named(&req_body).unwrap();
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}/v1/provisioning/requests"),
        Some(req_bytes),
        None,
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let create_resp: CreateProvisioningResp =
        rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    let provisioning_id = create_resp.provisioning_id;

    // Manually set expires_at to past
    let row = device_provisioning_requests::Entity::find_by_id(provisioning_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: device_provisioning_requests::ActiveModel = row.into();
    active.expires_at = Set(now_secs() - 100);
    active.update(&db).await.unwrap();

    // Approve should fail with ProvisioningExpired
    let approve_req = ApproveProvisioningReq {
        encrypted_bootstrap_blob: vec![1u8, 2, 3],
        new_device_hpke_public_key: vec![0u8; 32],
        new_device_signing_public_key: vec![0u8; 32],
        device_authorization_signature: vec![0u8; 64],
        device_authorization_timestamp: now_secs(),
    };
    let approve_bytes = rmp_serde::to_vec_named(&approve_req).unwrap();
    let approve_path = format!("/v1/provisioning/requests/{provisioning_id}/approve");
    let approve_auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "POST",
        &approve_path,
        &approve_bytes,
    );
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}{approve_path}"),
        Some(approve_bytes),
        Some(&approve_auth),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_provisioning_gc_removes_expired() {
    let db = fresh_db().await;

    let expired_id = Uuid::now_v7();
    let now = now_secs();

    // Insert an expired pending request
    device_provisioning_requests::ActiveModel {
        id: Set(expired_id),
        user_id: Set(Uuid::now_v7()),
        new_device_temp_public_key: Set(vec![0u8; 32]),
        new_device_temp_signing_public_key: Set(vec![0u8; 32]),
        nonce: Set(vec![0u8; 16]),
        status: Set("pending".to_string()),
        expires_at: Set(now - 100),
        encrypted_bootstrap_blob: Set(None),
        approved_by_device_id: Set(None),
        new_device_id: Set(None),
        created_at: Set(now - 200),
    }
    .insert(&db)
    .await
    .unwrap();

    // GC query — проверяем что логика удаления работает.
    // Функция `run_provisioning_gc` запускает бесконечный цикл с тиком 60s,
    // поэтому в тесте вызываем запрос напрямую.
    let deleted = device_provisioning_requests::Entity::delete_many()
        .filter(
            sea_orm::Condition::all()
                .add(device_provisioning_requests::Column::ExpiresAt.lt(now))
                .add(
                    sea_orm::Condition::any()
                        .add(device_provisioning_requests::Column::Status.eq("pending"))
                        .add(device_provisioning_requests::Column::Status.eq("expired"))
                        .add(device_provisioning_requests::Column::Status.eq("consumed")),
                ),
        )
        .exec(&db)
        .await
        .unwrap();

    assert_eq!(deleted.rows_affected, 1, "GC should remove expired provisioning requests");
}

#[tokio::test]
async fn test_create_provisioning_request_invalid_keys() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Too short temp signing key
    let req_body = CreateProvisioningReq {
        user_id: user.user_id,
        new_device_temp_public_key: vec![0u8; 32],
        new_device_temp_signing_public_key: vec![0u8; 16], // too short
        nonce: vec![0u8; 16],
    };
    let req_bytes = rmp_serde::to_vec_named(&req_body).unwrap();
    let resp = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}/v1/provisioning/requests"),
        Some(req_bytes),
        None,
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Too short nonce
    let req_body2 = CreateProvisioningReq {
        user_id: user.user_id,
        new_device_temp_public_key: vec![0u8; 32],
        new_device_temp_signing_public_key: vec![0u8; 32],
        nonce: vec![0u8; 4], // too short
    };
    let req_bytes2 = rmp_serde::to_vec_named(&req_body2).unwrap();
    let resp2 = send_request(
        &client,
        reqwest::Method::POST,
        &format!("{base_url}/v1/provisioning/requests"),
        Some(req_bytes2),
        None,
        None,
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
}

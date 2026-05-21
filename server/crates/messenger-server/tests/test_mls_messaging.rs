//! Integration tests for S10 – MLS Messaging endpoints.
//!
//! Coverage:
//! - Create group (direct, with members)
//! - List my groups
//! - Get group members
//! - Post application message (idempotent)
//! - Pull messages with auto delivery receipts
//! - Commit with epoch advance
//! - Commit with wrong epoch rejected
//! - Concurrent commits (only one wins)
//! - Add member via commit
//! - Remove member via commit
//! - Non-admin cannot add others
//! - Self device add allowed
//! - Welcomes delivered to new device
//! - Welcome ack consumes
//! - Delivery status endpoint
//! - Edit/delete message state
//! - Only sender can edit
//! - Reactions add/dedup/remove
//! - Non-member cannot pull/post

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use ed25519_dalek::Signer;
use rand::RngCore;
use sea_orm::{
    ActiveModelTrait, Database, DatabaseConnection, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use messenger_crypto::canonical::build_signed_message;
use messenger_server::config::{AppConfig, LogFormat};
use messenger_server::routes::build_router;
use messenger_server::services::invite::now_secs;
use messenger_server::state::{AppState, NonceCache};
use messenger_migration::MigratorTrait;

// ─── Test Helpers ───

#[derive(Clone)]
struct TestUser {
    user_id: Uuid,
    device_id: Uuid,
    device_signing_key: ed25519_dalek::SigningKey,
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
        ws_registry: messenger_server::ws_registry::WsRegistry::new(),
    }
}

async fn create_user_with_device(db: &DatabaseConnection) -> TestUser {
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

async fn create_second_user(db: &DatabaseConnection, first_user: &TestUser) -> TestUser {
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

async fn send_authed_delete(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    user: &TestUser,
    body_bytes: Vec<u8>,
) -> reqwest::Response {
    let auth = make_auth_header(
        &user.device_signing_key,
        &user.device_id,
        "DELETE",
        path,
        &body_bytes,
    );
    client
        .delete(format!("{base_url}{path}"))
        .header("Content-Type", "application/msgpack")
        .header("X-Auth-Signature", &auth)
        .body(body_bytes)
        .send()
        .await
        .unwrap()
}

// ─── Request/Response types ───

#[derive(Serialize)]
struct CreateGroupReq {
    group_type: String,
    #[serde(with = "serde_bytes")]
    initial_commit: Vec<u8>,
    welcomes: Vec<WelcomeForDevice>,
    member_devices: Vec<MemberDeviceInit>,
}

#[derive(Clone, Serialize)]
struct WelcomeForDevice {
    recipient_device_id: Uuid,
    #[serde(with = "serde_bytes")]
    welcome_ciphertext: Vec<u8>,
}

#[derive(Clone, Serialize)]
struct MemberDeviceInit {
    user_id: Uuid,
    device_id: Uuid,
    leaf_index: i32,
    role_in_chat: String,
}

#[derive(Deserialize)]
struct CreateGroupResp {
    group_id: Uuid,
    epoch: i64,
    created_at: i64,
}

#[derive(Deserialize)]
struct GroupSummary {
    id: Uuid,
    group_type: String,
    current_epoch: i64,
    created_at: i64,
    role_in_chat: String,
    joined_at: i64,
}

#[derive(Deserialize)]
struct ListMyGroupsResp {
    groups: Vec<GroupSummary>,
    has_more: bool,
}

#[derive(Deserialize)]
struct GroupMembersResp {
    members: Vec<GroupMemberResp>,
    devices: Vec<GroupDeviceResp>,
}

#[derive(Deserialize)]
struct GroupMemberResp {
    user_id: Uuid,
    role_in_chat: String,
    joined_at_epoch: i64,
    left_at_epoch: Option<i64>,
}

#[derive(Deserialize)]
struct GroupDeviceResp {
    device_id: Uuid,
    user_id: Uuid,
    leaf_index: Option<i32>,
    added_at_epoch: i64,
    removed_at_epoch: Option<i64>,
}

#[derive(Serialize)]
struct PostCommitReq {
    expected_epoch: i64,
    #[serde(with = "serde_bytes")]
    commit: Vec<u8>,
    welcomes: Vec<WelcomeForDevice>,
    member_changes: Vec<MemberChange>,
}

#[derive(Serialize)]
struct MemberChange {
    kind: String,
    user_id: Uuid,
    device_id: Uuid,
    leaf_index: Option<i32>,
    role_in_chat: Option<String>,
}

#[derive(Deserialize)]
struct PostCommitResp {
    message_id: Uuid,
    new_epoch: i64,
}

#[derive(Serialize)]
struct PostMessageReq {
    expected_epoch: i64,
    #[serde(with = "serde_bytes")]
    mls_ciphertext: Vec<u8>,
    parent_message_id: Option<Uuid>,
    reply_to_message_id: Option<Uuid>,
    thread_root_id: Option<Uuid>,
    client_message_id: Uuid,
}

#[derive(Deserialize)]
struct PostMessageResp {
    message_id: Uuid,
    created_at: i64,
}

#[derive(Deserialize)]
struct StoredMessage {
    id: Uuid,
    group_id: Uuid,
    epoch: i64,
    sender_user_id: Uuid,
    sender_device_id: Uuid,
    wire_format: String,
    #[allow(dead_code)]
    mls_ciphertext: Vec<u8>,
    parent_message_id: Option<Uuid>,
    thread_root_id: Option<Uuid>,
    reply_to_message_id: Option<Uuid>,
    created_at: i64,
    #[allow(dead_code)]
    state: Option<MessageStateResp>,
}

#[derive(Deserialize)]
struct MessageStateResp {
    edited_at: Option<i64>,
    deleted_at: Option<i64>,
    replacement_message_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct PullMessagesResp {
    messages: Vec<StoredMessage>,
    has_more: bool,
}

#[derive(Deserialize)]
struct DeliveryStatusResp {
    message_id: Uuid,
    total_devices: i64,
    delivered_count: i64,
    #[allow(dead_code)]
    per_device: Vec<PerDeviceDeliveryResp>,
}

#[derive(Deserialize)]
struct PerDeviceDeliveryResp {
    device_id: Uuid,
    delivered_at: Option<i64>,
}

#[derive(Deserialize)]
struct PendingWelcome {
    id: Uuid,
    group_id: Uuid,
    recipient_device_id: Uuid,
    epoch: i64,
    #[allow(dead_code)]
    welcome_ciphertext: Vec<u8>,
    created_at: i64,
}

#[derive(Deserialize)]
struct ListWelcomesResp {
    welcomes: Vec<PendingWelcome>,
}

#[derive(Serialize)]
struct UpdateStateReq {
    kind: String,
    replacement_message_id: Option<Uuid>,
}

#[derive(Serialize)]
struct AddReactionReq {
    #[serde(with = "serde_bytes")]
    reaction_blind_index: Vec<u8>,
    applied_at_epoch: i64,
}

// ─── Helpers ───

fn fake_commit() -> Vec<u8> {
    vec![0u8; 64]
}

fn fake_welcome() -> Vec<u8> {
    vec![1u8; 128]
}

fn fake_ciphertext() -> Vec<u8> {
    vec![2u8; 256]
}

/// Create a simple direct group with the user themselves (single member).
async fn create_direct_group(
    client: &reqwest::Client,
    base_url: &str,
    user: &TestUser,
) -> CreateGroupResp {
    let req = CreateGroupReq {
        group_type: "direct".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![],
        member_devices: vec![MemberDeviceInit {
            user_id: user.user_id,
            device_id: user.device_id,
            leaf_index: 0,
            role_in_chat: "owner".to_string(),
        }],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(client, base_url, "/v1/groups", user, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED, "create group failed");
    rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap()
}

// ─── Tests ───

#[tokio::test]
async fn test_create_direct_group() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let resp = create_direct_group(&client, &base_url, &user).await;

    assert!(!resp.group_id.is_nil());
    assert_eq!(resp.epoch, 0);
    assert!(resp.created_at > 0);
}

#[tokio::test]
async fn test_create_group_with_3_members() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;
    let user3 = create_second_user(&db, &user1).await;

    let req = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![
            WelcomeForDevice {
                recipient_device_id: user2.device_id,
                welcome_ciphertext: fake_welcome(),
            },
            WelcomeForDevice {
                recipient_device_id: user3.device_id,
                welcome_ciphertext: fake_welcome(),
            },
        ],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
            MemberDeviceInit {
                user_id: user3.user_id,
                device_id: user3.device_id,
                leaf_index: 2,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let parsed: CreateGroupResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert!(!parsed.group_id.is_nil());
}

#[tokio::test]
async fn test_creator_must_be_member() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let other = create_second_user(&db, &user).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Creator (auth user) is NOT in member_devices
    let req = CreateGroupReq {
        group_type: "direct".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![],
        member_devices: vec![MemberDeviceInit {
            user_id: other.user_id,
            device_id: other.device_id,
            leaf_index: 0,
            role_in_chat: "member".to_string(),
        }],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user, bytes).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_my_groups() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    // Create 2 groups
    create_direct_group(&client, &base_url, &user).await;
    create_direct_group(&client, &base_url, &user).await;

    let resp = send_authed_get(&client, &base_url, "/v1/groups/me", &user).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let parsed: ListMyGroupsResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.groups.len(), 2);
    assert!(!parsed.has_more);
}

#[tokio::test]
async fn test_get_group_members() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user1).await;

    let resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/members", group.group_id),
        &user1,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let parsed: GroupMembersResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.members.len(), 1);
    assert_eq!(parsed.members[0].user_id, user1.user_id);
    assert_eq!(parsed.members[0].role_in_chat, "owner");
    assert_eq!(parsed.devices.len(), 1);
    assert_eq!(parsed.devices[0].device_id, user1.device_id);
}

#[tokio::test]
async fn test_non_member_cannot_get_members() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let user2 = create_second_user(&db, &user1).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user1).await;

    // user2 is not in the group
    let resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/members", group.group_id),
        &user2,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_post_application_message() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    let req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let parsed: PostMessageResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert!(!parsed.message_id.is_nil());
}

#[tokio::test]
async fn test_idempotent_message_post() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    let client_message_id = Uuid::now_v7();
    let req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id,
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let path = format!("/v1/groups/{}/messages", group.group_id);

    // First post
    let resp1 = send_authed_post(&client, &base_url, &path, &user, bytes.clone()).await;
    assert_eq!(resp1.status(), StatusCode::CREATED);
    let parsed1: PostMessageResp = rmp_serde::from_slice(&resp1.bytes().await.unwrap()).unwrap();

    // Second post with same client_message_id
    let resp2 = send_authed_post(&client, &base_url, &path, &user, bytes).await;
    assert_eq!(resp2.status(), StatusCode::OK);
    let parsed2: PostMessageResp = rmp_serde::from_slice(&resp2.bytes().await.unwrap()).unwrap();

    // Should return same message_id
    assert_eq!(parsed1.message_id, parsed2.message_id);
}

#[tokio::test]
async fn test_pull_messages_marks_delivery() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Post a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::CREATED);
    let posted: PostMessageResp = rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // Pull messages (should create delivery receipt)
    let pull_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages?limit=100", group.group_id),
        &user,
    )
    .await;
    assert_eq!(pull_resp.status(), StatusCode::OK);
    let parsed: PullMessagesResp = rmp_serde::from_slice(&pull_resp.bytes().await.unwrap()).unwrap();
    assert!(!parsed.messages.is_empty());

    // Check delivery status
    let delivery_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/messages/{}/delivery", posted.message_id),
        &user,
    )
    .await;
    assert_eq!(delivery_resp.status(), StatusCode::OK);
    let delivery: DeliveryStatusResp =
        rmp_serde::from_slice(&delivery_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(delivery.message_id, posted.message_id);
    // At least 1 device should have delivered
    assert!(delivery.delivered_count >= 1);
}

#[tokio::test]
async fn test_pull_with_since_id() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Post 2 messages
    let mut last_id = Uuid::default();
    for _ in 0..2 {
        let req = PostMessageReq {
            expected_epoch: 0,
            mls_ciphertext: fake_ciphertext(),
            parent_message_id: None,
            reply_to_message_id: None,
            thread_root_id: None,
            client_message_id: Uuid::now_v7(),
        };
        let bytes = rmp_serde::to_vec_named(&req).unwrap();
        let resp = send_authed_post(
            &client,
            &base_url,
            &format!("/v1/groups/{}/messages", group.group_id),
            &user,
            bytes,
        )
        .await;
        let parsed: PostMessageResp =
            rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
        last_id = parsed.message_id;
    }

    // Pull with since_id = first message (excluding the commit message)
    // First pull all to find first message id
    let all = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages?limit=100", group.group_id),
        &user,
    )
    .await;
    let parsed_all: PullMessagesResp =
        rmp_serde::from_slice(&all.bytes().await.unwrap()).unwrap();
    // We have commit + 2 messages = 3 total
    assert_eq!(parsed_all.messages.len(), 3);

    // Pull with since_id = first message's id (should return newer ones)
    let first_app_msg = &parsed_all.messages[1]; // skip commit (index 0)
    let since_resp = send_authed_get(
        &client,
        &base_url,
        &format!(
            "/v1/groups/{}/messages?since_id={}",
            group.group_id, first_app_msg.id
        ),
        &user,
    )
    .await;
    let parsed_since: PullMessagesResp =
        rmp_serde::from_slice(&since_resp.bytes().await.unwrap()).unwrap();
    // Should include the second message (but not the first)
    assert!(!parsed_since.messages.is_empty());
    assert_eq!(parsed_since.messages[0].id, last_id);
}

#[tokio::test]
async fn test_commit_advances_epoch() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    let req = PostCommitReq {
        expected_epoch: 0,
        commit: fake_commit(),
        welcomes: vec![],
        member_changes: vec![],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/commit", group.group_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let parsed: PostCommitResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();
    assert_eq!(parsed.new_epoch, 1);
    assert!(!parsed.message_id.is_nil());
}

#[tokio::test]
async fn test_commit_with_wrong_epoch_rejected() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Try commit with wrong epoch
    let req = PostCommitReq {
        expected_epoch: 42,
        commit: fake_commit(),
        welcomes: vec![],
        member_changes: vec![],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/commit", group.group_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_concurrent_commits_only_one_wins() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Spawn 2 concurrent commits on epoch 0
    let req1 = PostCommitReq {
        expected_epoch: 0,
        commit: vec![1u8; 64],
        welcomes: vec![],
        member_changes: vec![],
    };
    let req2 = PostCommitReq {
        expected_epoch: 0,
        commit: vec![2u8; 64],
        welcomes: vec![],
        member_changes: vec![],
    };

    let base_url2 = base_url.clone();
    let user2 = user.clone();

    let j1 = tokio::spawn(async move {
        let client = reqwest::Client::new();
        let bytes = rmp_serde::to_vec_named(&req1).unwrap();
        send_authed_post(
            &client,
            &base_url2,
            &format!("/v1/groups/{}/commit", group.group_id),
            &user2,
            bytes,
        )
        .await
    });

    let j2 = tokio::spawn(async move {
        let client = reqwest::Client::new();
        let bytes = rmp_serde::to_vec_named(&req2).unwrap();
        send_authed_post(
            &client,
            &base_url,
            &format!("/v1/groups/{}/commit", group.group_id),
            &user,
            bytes,
        )
        .await
    });

    let (r1, r2) = tokio::join!(j1, j2);
    let resp1 = r1.unwrap();
    let resp2 = r2.unwrap();

    // Exactly one should succeed, one should get EpochOutdated
    let statuses = [resp1.status(), resp2.status()];
    let ok_count = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();

    assert_eq!(ok_count, 1, "exactly one commit should succeed");
    assert_eq!(conflict_count, 1, "exactly one commit should be EpochOutdated");
}

#[tokio::test]
async fn test_add_member_via_commit() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user1).await;

    let user2 = create_second_user(&db, &user1).await;

    // Add user2 via commit
    let req = PostCommitReq {
        expected_epoch: 0,
        commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user2.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_changes: vec![MemberChange {
            kind: "add".to_string(),
            user_id: user2.user_id,
            device_id: user2.device_id,
            leaf_index: Some(1),
            role_in_chat: Some("member".to_string()),
        }],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/commit", group.group_id),
        &user1,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Check members now include user2
    let members_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/members", group.group_id),
        &user1,
    )
    .await;
    let members: GroupMembersResp =
        rmp_serde::from_slice(&members_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(members.members.len(), 2);
    assert!(members.members.iter().any(|m| m.user_id == user2.user_id));
    assert_eq!(members.devices.len(), 2);
}

#[tokio::test]
async fn test_remove_member_via_commit() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    // Create group with both users
    let req = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user2.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let group: CreateGroupResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // Remove user2 via commit
    let commit_req = PostCommitReq {
        expected_epoch: 0,
        commit: fake_commit(),
        welcomes: vec![],
        member_changes: vec![MemberChange {
            kind: "remove".to_string(),
            user_id: user2.user_id,
            device_id: user2.device_id,
            leaf_index: None,
            role_in_chat: None,
        }],
    };
    let bytes = rmp_serde::to_vec_named(&commit_req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/commit", group.group_id),
        &user1,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Check members — only user1 left
    let members_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/members", group.group_id),
        &user1,
    )
    .await;
    let members: GroupMembersResp =
        rmp_serde::from_slice(&members_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(members.members[0].user_id, user1.user_id);
    // user2 should be gone (either not in list or left)
    assert!(
        members.members.len() == 1
            || members
                .members
                .iter()
                .find(|m| m.user_id == user2.user_id)
                .map_or(true, |m| m.left_at_epoch.is_some())
    );
}

#[tokio::test]
async fn test_non_admin_cannot_add_others() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    // Create user2 with member role
    let user2 = create_second_user(&db, &user1).await;

    let group = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&group).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let group_resp: CreateGroupResp =
        rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // user3
    let user3 = create_second_user(&db, &user1).await;

    // user2 (member, not admin) tries to add user3
    let commit_req = PostCommitReq {
        expected_epoch: 0,
        commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user3.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_changes: vec![MemberChange {
            kind: "add".to_string(),
            user_id: user3.user_id,
            device_id: user3.device_id,
            leaf_index: Some(2),
            role_in_chat: Some("member".to_string()),
        }],
    };
    let bytes = rmp_serde::to_vec_named(&commit_req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/commit", group_resp.group_id),
        &user2,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_self_device_add_allowed() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user1).await;

    // Add a second device for the same user (self-add)
    let mut rng = rand::thread_rng();
    let new_device_key = ed25519_dalek::SigningKey::generate(&mut rng);
    let new_device_vk = new_device_key.verifying_key();
    let new_device_id = Uuid::now_v7();
    let now = now_secs();

    messenger_entity::devices::ActiveModel {
        id: Set(new_device_id),
        user_id: Set(user1.user_id),
        hpke_init_public_key: Set(vec![0u8; 32]),
        device_signing_public_key: Set(new_device_vk.to_bytes().to_vec()),
        authorization_signature: Set(vec![0u8; 64]),
        authorized_by_device_id: Set(None),
        created_at: Set(now),
        revoked_at: Set(None),
        revoked_by_device_id: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // Commit to add self device (no admin needed for self-add)
    let commit_req = PostCommitReq {
        expected_epoch: 0,
        commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: new_device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_changes: vec![MemberChange {
            kind: "add".to_string(),
            user_id: user1.user_id,
            device_id: new_device_id,
            leaf_index: Some(1),
            role_in_chat: Some("owner".to_string()),
        }],
    };
    let bytes = rmp_serde::to_vec_named(&commit_req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/commit", group.group_id),
        &user1,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Check devices in group
    let members_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/members", group.group_id),
        &user1,
    )
    .await;
    let members: GroupMembersResp =
        rmp_serde::from_slice(&members_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(members.devices.len(), 2);
}

#[tokio::test]
async fn test_welcomes_delivered_to_new_device() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    // Create group with user2 — should generate a welcome for user2
    let req = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user2.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // user2 should see a pending welcome
    let welcomes_resp = send_authed_get(&client, &base_url, "/v1/welcomes/me", &user2).await;
    assert_eq!(welcomes_resp.status(), StatusCode::OK);
    let welcomes: ListWelcomesResp =
        rmp_serde::from_slice(&welcomes_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(welcomes.welcomes.len(), 1);
    assert_eq!(welcomes.welcomes[0].recipient_device_id, user2.device_id);
    assert_eq!(welcomes.welcomes[0].epoch, 0);
}

#[tokio::test]
async fn test_welcome_ack_consumes() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    // Create group with user2
    let req = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user2.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Get welcome id
    let welcomes_resp = send_authed_get(&client, &base_url, "/v1/welcomes/me", &user2).await;
    let welcomes: ListWelcomesResp =
        rmp_serde::from_slice(&welcomes_resp.bytes().await.unwrap()).unwrap();
    let welcome_id = welcomes.welcomes[0].id;

    // Ack the welcome
    let ack_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/welcomes/{welcome_id}/ack"),
        &user2,
        vec![],
    )
    .await;
    assert_eq!(ack_resp.status(), StatusCode::NO_CONTENT);

    // Verify welcome no longer appears
    let welcomes_resp2 = send_authed_get(&client, &base_url, "/v1/welcomes/me", &user2).await;
    let welcomes2: ListWelcomesResp =
        rmp_serde::from_slice(&welcomes_resp2.bytes().await.unwrap()).unwrap();
    assert!(welcomes2.welcomes.is_empty());
}

#[tokio::test]
async fn test_delivery_status_endpoint() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user1).await;

    // Post a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user1,
        bytes,
    )
    .await;
    let posted: PostMessageResp = rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // Pull (marks delivery)
    let _ = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages?limit=100", group.group_id),
        &user1,
    )
    .await;

    // Check delivery status
    let delivery_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/messages/{}/delivery", posted.message_id),
        &user1,
    )
    .await;
    assert_eq!(delivery_resp.status(), StatusCode::OK);
    let delivery: DeliveryStatusResp =
        rmp_serde::from_slice(&delivery_resp.bytes().await.unwrap()).unwrap();
    assert_eq!(delivery.message_id, posted.message_id);
    assert!(delivery.total_devices >= 1);
}

#[tokio::test]
async fn test_non_sender_cannot_view_delivery() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    // Create group with both
    let req = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user2.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let group: CreateGroupResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // user1 posts a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user1,
        bytes,
    )
    .await;
    let posted: PostMessageResp = rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // user2 (not sender) tries to view delivery — should be forbidden
    let delivery_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/messages/{}/delivery", posted.message_id),
        &user2,
    )
    .await;
    assert_eq!(delivery_resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_edit_message_state() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Post original message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    let original: PostMessageResp =
        rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // Post replacement message
    let replacement_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&replacement_req).unwrap();
    let replacement_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    let replacement: PostMessageResp =
        rmp_serde::from_slice(&replacement_resp.bytes().await.unwrap()).unwrap();

    // Mark original as edited
    let state_req = UpdateStateReq {
        kind: "edit".to_string(),
        replacement_message_id: Some(replacement.message_id),
    };
    let bytes = rmp_serde::to_vec_named(&state_req).unwrap();
    let state_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/messages/{}/state", original.message_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(state_resp.status(), StatusCode::NO_CONTENT);

    // Pull and verify state
    let pull_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages?limit=100", group.group_id),
        &user,
    )
    .await;
    let pulled: PullMessagesResp =
        rmp_serde::from_slice(&pull_resp.bytes().await.unwrap()).unwrap();
    let original_msg = pulled
        .messages
        .iter()
        .find(|m| m.id == original.message_id)
        .unwrap();
    assert!(original_msg.state.is_some());
    let state = original_msg.state.as_ref().unwrap();
    assert!(state.edited_at.is_some());
    assert_eq!(
        state.replacement_message_id,
        Some(replacement.message_id)
    );
}

#[tokio::test]
async fn test_delete_message_state() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Post a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    let posted: PostMessageResp =
        rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // Mark as deleted
    let state_req = UpdateStateReq {
        kind: "delete".to_string(),
        replacement_message_id: None,
    };
    let bytes = rmp_serde::to_vec_named(&state_req).unwrap();
    let state_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/messages/{}/state", posted.message_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(state_resp.status(), StatusCode::NO_CONTENT);

    // Pull and verify
    let pull_resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages?limit=100", group.group_id),
        &user,
    )
    .await;
    let pulled: PullMessagesResp =
        rmp_serde::from_slice(&pull_resp.bytes().await.unwrap()).unwrap();
    let msg = pulled
        .messages
        .iter()
        .find(|m| m.id == posted.message_id)
        .unwrap();
    assert!(msg.state.is_some());
    assert!(msg.state.as_ref().unwrap().deleted_at.is_some());
}

#[tokio::test]
async fn test_only_sender_can_edit() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    // Create group with both
    let req = CreateGroupReq {
        group_type: "group".to_string(),
        initial_commit: fake_commit(),
        welcomes: vec![WelcomeForDevice {
            recipient_device_id: user2.device_id,
            welcome_ciphertext: fake_welcome(),
        }],
        member_devices: vec![
            MemberDeviceInit {
                user_id: user1.user_id,
                device_id: user1.device_id,
                leaf_index: 0,
                role_in_chat: "owner".to_string(),
            },
            MemberDeviceInit {
                user_id: user2.user_id,
                device_id: user2.device_id,
                leaf_index: 1,
                role_in_chat: "member".to_string(),
            },
        ],
    };
    let bytes = rmp_serde::to_vec_named(&req).unwrap();
    let resp = send_authed_post(&client, &base_url, "/v1/groups", &user1, bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let group: CreateGroupResp = rmp_serde::from_slice(&resp.bytes().await.unwrap()).unwrap();

    // user1 posts a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user1,
        bytes,
    )
    .await;
    let posted: PostMessageResp =
        rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // user2 tries to edit user1's message — should be forbidden
    let state_req = UpdateStateReq {
        kind: "edit".to_string(),
        replacement_message_id: None,
    };
    let bytes = rmp_serde::to_vec_named(&state_req).unwrap();
    let state_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/messages/{}/state", posted.message_id),
        &user2,
        bytes,
    )
    .await;
    assert_eq!(state_resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_reaction_add_and_dedup() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Post a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    let posted: PostMessageResp =
        rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // Add reaction
    let react_req = AddReactionReq {
        reaction_blind_index: vec![0x01, 0x02, 0x03],
        applied_at_epoch: 0,
    };
    let bytes = rmp_serde::to_vec_named(&react_req).unwrap();
    let react_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/messages/{}/reactions", posted.message_id),
        &user,
        bytes.clone(),
    )
    .await;
    assert_eq!(react_resp.status(), StatusCode::NO_CONTENT);

    // Add same reaction again (dedup — should still be 204)
    let react_resp2 = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/messages/{}/reactions", posted.message_id),
        &user,
        bytes,
    )
    .await;
    assert_eq!(react_resp2.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_reaction_remove() {
    let db = fresh_db().await;
    let user = create_user_with_device(&db).await;
    let base_url = start_server(user.state.clone()).await;
    let client = reqwest::Client::new();

    let group = create_direct_group(&client, &base_url, &user).await;

    // Post a message
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let msg_resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user,
        bytes,
    )
    .await;
    let posted: PostMessageResp =
        rmp_serde::from_slice(&msg_resp.bytes().await.unwrap()).unwrap();

    // Add reaction
    let blind_index = vec![0xAA, 0xBB, 0xCC];
    let react_req = AddReactionReq {
        reaction_blind_index: blind_index.clone(),
        applied_at_epoch: 0,
    };
    let bytes = rmp_serde::to_vec_named(&react_req).unwrap();
    let _ = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/messages/{}/reactions", posted.message_id),
        &user,
        bytes,
    )
    .await;

    // Remove reaction
    let blind_hex = hex::encode(&blind_index);
    let remove_resp = send_authed_delete(
        &client,
        &base_url,
        &format!(
            "/v1/messages/{}/reactions/{blind_hex}",
            posted.message_id
        ),
        &user,
        vec![],
    )
    .await;
    assert_eq!(remove_resp.status(), StatusCode::NO_CONTENT);

    // Removing again should also be 204 (idempotent)
    let remove_resp2 = send_authed_delete(
        &client,
        &base_url,
        &format!(
            "/v1/messages/{}/reactions/{blind_hex}",
            posted.message_id
        ),
        &user,
        vec![],
    )
    .await;
    assert_eq!(remove_resp2.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_non_member_cannot_pull() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    let group = create_direct_group(&client, &base_url, &user1).await;

    // user2 (not in group) tries to pull
    let resp = send_authed_get(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user2,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_non_member_cannot_post() {
    let db = fresh_db().await;
    let user1 = create_user_with_device(&db).await;
    let base_url = start_server(user1.state.clone()).await;
    let client = reqwest::Client::new();

    let user2 = create_second_user(&db, &user1).await;

    let group = create_direct_group(&client, &base_url, &user1).await;

    // user2 (not in group) tries to post
    let msg_req = PostMessageReq {
        expected_epoch: 0,
        mls_ciphertext: fake_ciphertext(),
        parent_message_id: None,
        reply_to_message_id: None,
        thread_root_id: None,
        client_message_id: Uuid::now_v7(),
    };
    let bytes = rmp_serde::to_vec_named(&msg_req).unwrap();
    let resp = send_authed_post(
        &client,
        &base_url,
        &format!("/v1/groups/{}/messages", group.group_id),
        &user2,
        bytes,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

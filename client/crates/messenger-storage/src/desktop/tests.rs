//! Desktop storage tests.

use super::*;
use crate::traits::{MessengerLocalStore, SecretStore, StorageValue};
use crate::types::*;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires running secret-service (desktop only)"]
async fn test_keyring_round_trip() {
    let store = KeyringSecretStore::new("test_user");
    let key = "test_secret";
    let value = b"hello world";

    store.set(key, value).await.unwrap();
    let got = store.get(key).await.unwrap();
    assert_eq!(got.unwrap(), value);

    store.delete(key).await.unwrap();
    let got = store.get(key).await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn test_sqlcipher_wrong_key_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let key1 = [1u8; 32];
    let key2 = [2u8; 32];

    // Create with key1
    let db = SqlcipherDatabase::open(path.clone(), &key1).unwrap();
    db.execute("CREATE TABLE t (a INTEGER)", &[]).await.unwrap();
    drop(db);

    // Open with key2 should fail
    let err = SqlcipherDatabase::open(path, &key2).unwrap_err();
    assert!(matches!(err, StorageError::AccessDenied));
}

#[tokio::test]
async fn test_sqlcipher_correct_key_works() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let key = [3u8; 32];

    let db = SqlcipherDatabase::open(path.clone(), &key).unwrap();
    db.execute("CREATE TABLE t (a INTEGER)", &[]).await.unwrap();
    db.execute("INSERT INTO t (a) VALUES (?)", &[StorageValue::Int(42)])
        .await
        .unwrap();
    drop(db);

    let db = SqlcipherDatabase::open(path, &key).unwrap();
    let rows = db.query("SELECT a FROM t", &[]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns[0].1, StorageValue::Int(42));
}

#[tokio::test]
async fn test_rekey_works() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let key1 = [4u8; 32];
    let key2 = [5u8; 32];

    let db = SqlcipherDatabase::open(path.clone(), &key1).unwrap();
    db.execute("CREATE TABLE t (a INTEGER)", &[]).await.unwrap();
    db.execute("INSERT INTO t (a) VALUES (?)", &[StorageValue::Int(99)])
        .await
        .unwrap();
    db.change_key(&key2).unwrap();
    drop(db);

    let db = SqlcipherDatabase::open(path, &key2).unwrap();
    let rows = db.query("SELECT a FROM t", &[]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns[0].1, StorageValue::Int(99));
}

#[tokio::test]
async fn test_high_level_store_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let key = [6u8; 32];

    let db = SqlcipherDatabase::open(path, &key).unwrap();
    for sql in crate::migrations::schema_v1() {
        db.execute(sql, &[]).await.unwrap();
    }
    let store = SqlcipherMessengerStore::new(db);

    let user_id = Uuid::new_v7(uuid::Timestamp::from_unix_time(0, 0, 0, 0));
    let identity = EncryptedIdentity {
        identity_secret_key_wrapped: vec![1, 2, 3],
        identity_public_key: vec![4, 5, 6],
        device_signing_secret_key_wrapped: vec![7, 8, 9],
        device_signing_public_key: vec![10, 11, 12],
        device_hpke_secret_key_wrapped: vec![13, 14, 15],
        device_hpke_public_key: vec![16, 17, 18],
    };

    store.save_identity(user_id, &identity).await.unwrap();
    let loaded = store.load_identity(user_id).await.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.identity_secret_key_wrapped, identity.identity_secret_key_wrapped);
}

#[tokio::test]
async fn test_high_level_store_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let key = [7u8; 32];

    let db = SqlcipherDatabase::open(path, &key).unwrap();
    for sql in crate::migrations::schema_v1() {
        db.execute(sql, &[]).await.unwrap();
    }
    let store = SqlcipherMessengerStore::new(db);

    let group_id = Uuid::new_v7(uuid::Timestamp::from_unix_time(0, 0, 0, 0));
    let msg = CachedMessage {
        id: Uuid::new_v7(uuid::Timestamp::from_unix_time(0, 0, 0, 0)),
        group_id,
        sender_user_id: Uuid::new_v7(uuid::Timestamp::from_unix_time(0, 0, 0, 0)),
        sender_device_id: Uuid::new_v7(uuid::Timestamp::from_unix_time(0, 0, 0, 0)),
        wire_format: "mls_ciphertext".into(),
        ciphertext: vec![1, 2, 3],
        plaintext: Some(vec![4, 5, 6]),
        content_type: Some("text".into()),
        reply_to_message_id: None,
        thread_root_id: None,
        edited_at: None,
        deleted_at: None,
        created_at: 1234567890,
    };

    store.save_message(&msg).await.unwrap();
    let msgs = store.list_messages(group_id, 10, None).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].ciphertext, msg.ciphertext);
}

#[tokio::test]
async fn test_migrations_apply() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let key = [8u8; 32];

    let db = SqlcipherDatabase::open(path, &key).unwrap();
    for sql in crate::migrations::schema_v1() {
        db.execute(sql, &[]).await.unwrap();
    }

    // Verify tables exist
    let rows = db
        .query(
            "SELECT name FROM sqlite_master WHERE type='table'",
            &[],
        )
        .await
        .unwrap();
    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            r.columns.get(0).and_then(|(_, v)| match v {
                StorageValue::Text(s) => Some(s.clone()),
                _ => None,
            })
        })
        .collect();
    assert!(names.contains(&"identity".to_string()));
    assert!(names.contains(&"messages".to_string()));
}

use messenger_migration::Migrator;
use sea_orm_migration::prelude::*;

const EXPECTED_TABLES: &[&str] = &[
    "server_config",
    "users",
    "user_identity_credentials",
    "devices",
    "key_packages",
    "device_provisioning_requests",
    "key_change_events",
    "invitation_tokens",
    "invitation_token_redemptions",
    "mls_groups",
    "mls_group_members",
    "mls_group_devices",
    "mls_messages",
    "mls_message_states",
    "mls_welcomes",
    "attachments",
    "reactions",
    "message_delivery_receipts",
];

#[tokio::test]
async fn test_migrations_apply() {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();

    let builder = db.get_database_backend();
    let tables = db
        .query_all(sea_orm::Statement::from_string(
            builder,
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name".to_owned(),
        ))
        .await
        .unwrap();

    let table_names: Vec<String> = tables
        .iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect();

    for expected in EXPECTED_TABLES {
        assert!(
            table_names.contains(&ToString::to_string(expected)),
            "Table {expected} should exist"
        );
    }
}

#[tokio::test]
async fn test_migrations_rollback() {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    Migrator::reset(&db).await.unwrap();

    let builder = db.get_database_backend();
    let tables = db
        .query_all(sea_orm::Statement::from_string(
            builder,
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name".to_owned(),
        ))
        .await
        .unwrap();

    let table_names: Vec<String> = tables
        .iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect();

    for expected in EXPECTED_TABLES {
        assert!(
            !table_names.contains(&ToString::to_string(expected)),
            "Table {expected} should have been dropped after rollback"
        );
    }
}

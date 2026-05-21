use sea_orm_migration::prelude::*;
use sea_orm::{Statement, DbBackend};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(KeyPackages::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(KeyPackages::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(KeyPackages::DeviceId).uuid().not_null())
                    .col(ColumnDef::new(KeyPackages::KeyPackage).blob().not_null())
                    .col(ColumnDef::new(KeyPackages::InitKeyHash).blob().not_null().unique_key())
                    .col(ColumnDef::new(KeyPackages::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(KeyPackages::ExpiresAt).big_integer().not_null())
                    .col(ColumnDef::new(KeyPackages::ConsumedAt).big_integer())
                    .col(ColumnDef::new(KeyPackages::ConsumedByUserId).uuid())
                    .col(ColumnDef::new(KeyPackages::IsLastResort).boolean().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_key_packages_device_id")
                            .from(KeyPackages::Table, KeyPackages::DeviceId)
                            .to(Devices::Table, Devices::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Partial index: неконсюмированные keypackages для быстрого claim
        let partial_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS key_packages_available \
                 ON key_packages(device_id, created_at) \
                 WHERE consumed_at IS NULL AND is_last_resort = false"
            }
            DbBackend::MySql => unreachable!(),
        };
        manager
            .get_connection()
            .execute(Statement::from_string(backend, partial_idx.to_owned()))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS key_packages_device_id ON key_packages(device_id)"
                    .to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for index in ["key_packages_available", "key_packages_device_id"] {
            let _ = manager
                .get_connection()
                .execute(Statement::from_string(
                    backend,
                    format!("DROP INDEX IF EXISTS {index}"),
                ))
                .await;
        }
        manager
            .drop_table(Table::drop().table(KeyPackages::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum KeyPackages {
    Table,
    Id,
    DeviceId,
    KeyPackage,
    InitKeyHash,
    CreatedAt,
    ExpiresAt,
    ConsumedAt,
    ConsumedByUserId,
    IsLastResort,
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
}

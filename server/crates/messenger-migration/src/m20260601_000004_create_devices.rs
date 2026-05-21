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
                    .table(Devices::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Devices::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Devices::UserId).uuid().not_null())
                    .col(ColumnDef::new(Devices::HpkeInitPublicKey).blob().not_null())
                    .col(ColumnDef::new(Devices::DeviceSigningPublicKey).blob().not_null())
                    .col(ColumnDef::new(Devices::AuthorizationSignature).blob().not_null())
                    .col(ColumnDef::new(Devices::AuthorizedByDeviceId).uuid())
                    .col(ColumnDef::new(Devices::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(Devices::RevokedAt).big_integer())
                    .col(ColumnDef::new(Devices::RevokedByDeviceId).uuid())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_devices_user_id")
                            .from(Devices::Table, Devices::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Partial index: активные устройства пользователя
        let create_active_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS devices_active_by_user \
                 ON devices(user_id) WHERE revoked_at IS NULL"
            }
            DbBackend::MySql => unreachable!(),
        };
        manager
            .get_connection()
            .execute(Statement::from_string(backend, create_active_idx.to_owned()))
            .await?;

        // Index на user_id (для всех, не только активных)
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS devices_user_id ON devices(user_id)".to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for index in ["devices_active_by_user", "devices_user_id"] {
            let _ = manager
                .get_connection()
                .execute(Statement::from_string(
                    backend,
                    format!("DROP INDEX IF EXISTS {index}"),
                ))
                .await;
        }
        manager
            .drop_table(Table::drop().table(Devices::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
    UserId,
    HpkeInitPublicKey,
    DeviceSigningPublicKey,
    AuthorizationSignature,
    AuthorizedByDeviceId,
    CreatedAt,
    RevokedAt,
    RevokedByDeviceId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

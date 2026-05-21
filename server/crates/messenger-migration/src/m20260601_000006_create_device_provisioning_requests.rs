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
                    .table(DeviceProvisioningRequests::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DeviceProvisioningRequests::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DeviceProvisioningRequests::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(DeviceProvisioningRequests::NewDeviceTempPublicKey)
                            .blob()
                            .not_null(),
                    )
                    .col(ColumnDef::new(DeviceProvisioningRequests::Nonce).blob().not_null())
                    .col(ColumnDef::new(DeviceProvisioningRequests::Status).string().not_null())
                    .col(
                        ColumnDef::new(DeviceProvisioningRequests::ExpiresAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(DeviceProvisioningRequests::EncryptedBootstrapBlob).blob())
                    .col(ColumnDef::new(DeviceProvisioningRequests::ApprovedByDeviceId).uuid())
                    .col(ColumnDef::new(DeviceProvisioningRequests::NewDeviceId).uuid())
                    .col(
                        ColumnDef::new(DeviceProvisioningRequests::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Partial index для GC просроченных запросов
        let gc_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS device_provisioning_requests_expires \
                 ON device_provisioning_requests(expires_at) \
                 WHERE status IN ('pending', 'expired')"
            }
            DbBackend::MySql => unreachable!(),
        };
        manager
            .get_connection()
            .execute(Statement::from_string(backend, gc_idx.to_owned()))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        let _ = manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "DROP INDEX IF EXISTS device_provisioning_requests_expires".to_owned(),
            ))
            .await;
        manager
            .drop_table(
                Table::drop()
                    .table(DeviceProvisioningRequests::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum DeviceProvisioningRequests {
    Table,
    Id,
    UserId,
    NewDeviceTempPublicKey,
    Nonce,
    Status,
    ExpiresAt,
    EncryptedBootstrapBlob,
    ApprovedByDeviceId,
    NewDeviceId,
    CreatedAt,
}

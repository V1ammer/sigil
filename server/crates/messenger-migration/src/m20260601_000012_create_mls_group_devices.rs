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
                    .table(MlsGroupDevices::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MlsGroupDevices::GroupId).uuid().not_null())
                    .col(ColumnDef::new(MlsGroupDevices::DeviceId).uuid().not_null())
                    .col(ColumnDef::new(MlsGroupDevices::LeafIndex).integer())
                    .col(
                        ColumnDef::new(MlsGroupDevices::AddedAtEpoch)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(MlsGroupDevices::RemovedAtEpoch).big_integer())
                    .primary_key(
                        Index::create()
                            .col(MlsGroupDevices::GroupId)
                            .col(MlsGroupDevices::DeviceId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_group_devices_group_id")
                            .from(MlsGroupDevices::Table, MlsGroupDevices::GroupId)
                            .to(MlsGroups::Table, MlsGroups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_group_devices_device_id")
                            .from(MlsGroupDevices::Table, MlsGroupDevices::DeviceId)
                            .to(Devices::Table, Devices::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Partial index: группы где устройство активно
        let active_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS mls_group_devices_active_device \
                 ON mls_group_devices(device_id) WHERE removed_at_epoch IS NULL"
            }
            DbBackend::MySql => unreachable!(),
        };
        manager
            .get_connection()
            .execute(Statement::from_string(backend, active_idx.to_owned()))
            .await?;

        // Index на group_id
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS mls_group_devices_group_id \
                 ON mls_group_devices(group_id)"
                    .to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for index in ["mls_group_devices_active_device", "mls_group_devices_group_id"] {
            let _ = manager
                .get_connection()
                .execute(Statement::from_string(
                    backend,
                    format!("DROP INDEX IF EXISTS {index}"),
                ))
                .await;
        }
        manager
            .drop_table(Table::drop().table(MlsGroupDevices::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MlsGroupDevices {
    Table,
    GroupId,
    DeviceId,
    LeafIndex,
    AddedAtEpoch,
    RemovedAtEpoch,
}

#[derive(DeriveIden)]
enum MlsGroups {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
}

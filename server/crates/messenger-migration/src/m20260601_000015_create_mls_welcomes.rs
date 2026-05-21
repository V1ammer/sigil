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
                    .table(MlsWelcomes::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MlsWelcomes::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(MlsWelcomes::GroupId).uuid().not_null())
                    .col(
                        ColumnDef::new(MlsWelcomes::RecipientDeviceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(MlsWelcomes::Epoch).big_integer().not_null())
                    .col(ColumnDef::new(MlsWelcomes::WelcomeCiphertext).blob().not_null())
                    .col(ColumnDef::new(MlsWelcomes::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(MlsWelcomes::ConsumedAt).big_integer())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_welcomes_group_id")
                            .from(MlsWelcomes::Table, MlsWelcomes::GroupId)
                            .to(MlsGroups::Table, MlsGroups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Partial index: невостребованные welcome'ы для устройства
        let uncons_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS mls_welcomes_unconsumed \
                 ON mls_welcomes(recipient_device_id, id) WHERE consumed_at IS NULL"
            }
            DbBackend::MySql => unreachable!(),
        };
        manager
            .get_connection()
            .execute(Statement::from_string(backend, uncons_idx.to_owned()))
            .await?;

        // Index на recipient_device_id
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS mls_welcomes_recipient \
                 ON mls_welcomes(recipient_device_id)"
                    .to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for index in ["mls_welcomes_unconsumed", "mls_welcomes_recipient"] {
            let _ = manager
                .get_connection()
                .execute(Statement::from_string(
                    backend,
                    format!("DROP INDEX IF EXISTS {index}"),
                ))
                .await;
        }
        manager
            .drop_table(Table::drop().table(MlsWelcomes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MlsWelcomes {
    Table,
    Id,
    GroupId,
    RecipientDeviceId,
    Epoch,
    WelcomeCiphertext,
    CreatedAt,
    ConsumedAt,
}

#[derive(DeriveIden)]
enum MlsGroups {
    Table,
    Id,
}

use sea_orm_migration::prelude::*;
use sea_orm::Statement;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PushTokens::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(PushTokens::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(PushTokens::DeviceId).uuid().not_null())
                    .col(ColumnDef::new(PushTokens::Platform).string().not_null())
                    .col(ColumnDef::new(PushTokens::OpaqueToken).blob().not_null())
                    .col(ColumnDef::new(PushTokens::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(PushTokens::LastUsedAt).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_push_tokens_device_id")
                            .from(PushTokens::Table, PushTokens::DeviceId)
                            .to(Devices::Table, Devices::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS push_tokens_device_id \
                 ON push_tokens(device_id)"
                    .to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        let _ = manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "DROP INDEX IF EXISTS push_tokens_device_id".to_owned(),
            ))
            .await;
        manager
            .drop_table(Table::drop().table(PushTokens::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PushTokens {
    Table,
    Id,
    DeviceId,
    Platform,
    OpaqueToken,
    CreatedAt,
    LastUsedAt,
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
}

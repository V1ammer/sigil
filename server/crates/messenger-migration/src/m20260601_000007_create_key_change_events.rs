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
                    .table(KeyChangeEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(KeyChangeEvents::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(KeyChangeEvents::UserId).uuid().not_null())
                    .col(ColumnDef::new(KeyChangeEvents::DeviceId).uuid().not_null())
                    .col(ColumnDef::new(KeyChangeEvents::EventType).string().not_null())
                    .col(ColumnDef::new(KeyChangeEvents::CreatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS key_change_events_user_id \
                 ON key_change_events(user_id)"
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
                "DROP INDEX IF EXISTS key_change_events_user_id".to_owned(),
            ))
            .await;
        manager
            .drop_table(Table::drop().table(KeyChangeEvents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum KeyChangeEvents {
    Table,
    Id,
    UserId,
    DeviceId,
    EventType,
    CreatedAt,
}

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
                    .table(Reactions::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Reactions::MessageId).uuid().not_null())
                    .col(ColumnDef::new(Reactions::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(Reactions::ReactionBlindIndex)
                            .blob()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Reactions::SenderDeviceId).uuid().not_null())
                    .col(ColumnDef::new(Reactions::AppliedAtEpoch).big_integer().not_null())
                    .col(ColumnDef::new(Reactions::CreatedAt).big_integer().not_null())
                    .primary_key(
                        Index::create()
                            .col(Reactions::MessageId)
                            .col(Reactions::UserId)
                            .col(Reactions::ReactionBlindIndex),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reactions_message_id")
                            .from(Reactions::Table, Reactions::MessageId)
                            .to(MlsMessages::Table, MlsMessages::Id)
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
                "CREATE INDEX IF NOT EXISTS reactions_message_id \
                 ON reactions(message_id)"
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
                "DROP INDEX IF EXISTS reactions_message_id".to_owned(),
            ))
            .await;
        manager
            .drop_table(Table::drop().table(Reactions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Reactions {
    Table,
    MessageId,
    UserId,
    ReactionBlindIndex,
    SenderDeviceId,
    AppliedAtEpoch,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MlsMessages {
    Table,
    Id,
}

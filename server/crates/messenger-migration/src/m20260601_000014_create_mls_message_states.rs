use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MlsMessageStates::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MlsMessageStates::MessageId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(MlsMessageStates::EditedAt).big_integer())
                    .col(ColumnDef::new(MlsMessageStates::DeletedAt).big_integer())
                    .col(ColumnDef::new(MlsMessageStates::ReplacementMessageId).uuid())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_message_states_message_id")
                            .from(MlsMessageStates::Table, MlsMessageStates::MessageId)
                            .to(MlsMessages::Table, MlsMessages::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MlsMessageStates::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MlsMessageStates {
    Table,
    MessageId,
    EditedAt,
    DeletedAt,
    ReplacementMessageId,
}

#[derive(DeriveIden)]
enum MlsMessages {
    Table,
    Id,
}

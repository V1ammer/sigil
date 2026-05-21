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
                    .table(Attachments::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Attachments::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Attachments::MessageId).uuid())
                    .col(ColumnDef::new(Attachments::UploaderDeviceId).uuid().not_null())
                    .col(ColumnDef::new(Attachments::PayloadCiphertext).blob())
                    .col(ColumnDef::new(Attachments::StorageRef).string())
                    .col(ColumnDef::new(Attachments::PaddedSize).big_integer().not_null())
                    .col(ColumnDef::new(Attachments::SizeBucket).integer().not_null())
                    .col(ColumnDef::new(Attachments::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(Attachments::ExpiresAt).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_attachments_message_id")
                            .from(Attachments::Table, Attachments::MessageId)
                            .to(MlsMessages::Table, MlsMessages::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS attachments_message_id \
                 ON attachments(message_id)"
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
                "DROP INDEX IF EXISTS attachments_message_id".to_owned(),
            ))
            .await;
        manager
            .drop_table(Table::drop().table(Attachments::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Attachments {
    Table,
    Id,
    MessageId,
    UploaderDeviceId,
    PayloadCiphertext,
    StorageRef,
    PaddedSize,
    SizeBucket,
    CreatedAt,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum MlsMessages {
    Table,
    Id,
}

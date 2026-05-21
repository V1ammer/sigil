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
                    .table(MessageDeliveryReceipts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MessageDeliveryReceipts::MessageId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MessageDeliveryReceipts::RecipientDeviceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MessageDeliveryReceipts::DeliveredAt)
                            .big_integer()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(MessageDeliveryReceipts::MessageId)
                            .col(MessageDeliveryReceipts::RecipientDeviceId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_message_delivery_receipts_message_id")
                            .from(
                                MessageDeliveryReceipts::Table,
                                MessageDeliveryReceipts::MessageId,
                            )
                            .to(MlsMessages::Table, MlsMessages::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Index для запросов "все сообщения, которые доставлены этому устройству"
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS message_delivery_receipts_recipient \
                 ON message_delivery_receipts(recipient_device_id, message_id)"
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
                "DROP INDEX IF EXISTS message_delivery_receipts_recipient".to_owned(),
            ))
            .await;
        manager
            .drop_table(
                Table::drop()
                    .table(MessageDeliveryReceipts::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum MessageDeliveryReceipts {
    Table,
    MessageId,
    RecipientDeviceId,
    DeliveredAt,
}

#[derive(DeriveIden)]
enum MlsMessages {
    Table,
    Id,
}

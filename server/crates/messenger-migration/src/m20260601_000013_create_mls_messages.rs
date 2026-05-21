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
                    .table(MlsMessages::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MlsMessages::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(MlsMessages::GroupId).uuid().not_null())
                    .col(ColumnDef::new(MlsMessages::Epoch).big_integer().not_null())
                    .col(ColumnDef::new(MlsMessages::SenderUserId).uuid().not_null())
                    .col(ColumnDef::new(MlsMessages::SenderDeviceId).uuid().not_null())
                    .col(ColumnDef::new(MlsMessages::WireFormat).string().not_null())
                    .col(ColumnDef::new(MlsMessages::MlsCiphertext).blob().not_null())
                    .col(ColumnDef::new(MlsMessages::ParentMessageId).uuid())
                    .col(ColumnDef::new(MlsMessages::ThreadRootId).uuid())
                    .col(ColumnDef::new(MlsMessages::ReplyToMessageId).uuid())
                    .col(ColumnDef::new(MlsMessages::ClientMessageId).uuid().not_null())
                    .col(ColumnDef::new(MlsMessages::CreatedAt).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_messages_group_id")
                            .from(MlsMessages::Table, MlsMessages::GroupId)
                            .to(MlsGroups::Table, MlsGroups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();

        // (group_id, id) — основной для pull'а
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS mls_messages_group_id_id \
                 ON mls_messages(group_id, id)"
                    .to_owned(),
            ))
            .await?;

        // (thread_root_id, id) WHERE thread_root_id IS NOT NULL — треды
        let thread_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS mls_messages_thread_root \
                 ON mls_messages(thread_root_id, id) WHERE thread_root_id IS NOT NULL"
            }
            DbBackend::MySql => unreachable!(),
        };
        manager
            .get_connection()
            .execute(Statement::from_string(backend, thread_idx.to_owned()))
            .await?;

        // (group_id, epoch, wire_format) — поиск commit'ов
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE INDEX IF NOT EXISTS mls_messages_group_epoch_wire \
                 ON mls_messages(group_id, epoch, wire_format)"
                    .to_owned(),
            ))
            .await?;

        // UNIQUE (sender_device_id, client_message_id) — идемпотентность
        manager
            .get_connection()
            .execute(Statement::from_string(
                backend,
                "CREATE UNIQUE INDEX IF NOT EXISTS mls_messages_sender_client \
                 ON mls_messages(sender_device_id, client_message_id)"
                    .to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for index in [
            "mls_messages_group_id_id",
            "mls_messages_thread_root",
            "mls_messages_group_epoch_wire",
            "mls_messages_sender_client",
        ] {
            let _ = manager
                .get_connection()
                .execute(Statement::from_string(
                    backend,
                    format!("DROP INDEX IF EXISTS {index}"),
                ))
                .await;
        }
        manager
            .drop_table(Table::drop().table(MlsMessages::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MlsMessages {
    Table,
    Id,
    GroupId,
    Epoch,
    SenderUserId,
    SenderDeviceId,
    WireFormat,
    MlsCiphertext,
    ParentMessageId,
    ThreadRootId,
    ReplyToMessageId,
    ClientMessageId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MlsGroups {
    Table,
    Id,
}

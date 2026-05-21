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
                    .table(MlsGroupMembers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MlsGroupMembers::GroupId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MlsGroupMembers::UserId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(MlsGroupMembers::RoleInChat).string().not_null())
                    .col(
                        ColumnDef::new(MlsGroupMembers::JoinedAtEpoch)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(MlsGroupMembers::LeftAtEpoch).big_integer())
                    .col(ColumnDef::new(MlsGroupMembers::JoinedAt).big_integer().not_null())
                    .primary_key(
                        Index::create()
                            .col(MlsGroupMembers::GroupId)
                            .col(MlsGroupMembers::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_group_members_group_id")
                            .from(MlsGroupMembers::Table, MlsGroupMembers::GroupId)
                            .to(MlsGroups::Table, MlsGroups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mls_group_members_user_id")
                            .from(MlsGroupMembers::Table, MlsGroupMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        // Partial index: активные членства пользователя
        let active_idx = match backend {
            DbBackend::Sqlite | DbBackend::Postgres => {
                "CREATE INDEX IF NOT EXISTS mls_group_members_active_user \
                 ON mls_group_members(user_id) WHERE left_at_epoch IS NULL"
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
                "CREATE INDEX IF NOT EXISTS mls_group_members_group_id \
                 ON mls_group_members(group_id)"
                    .to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for index in ["mls_group_members_active_user", "mls_group_members_group_id"] {
            let _ = manager
                .get_connection()
                .execute(Statement::from_string(
                    backend,
                    format!("DROP INDEX IF EXISTS {index}"),
                ))
                .await;
        }
        manager
            .drop_table(Table::drop().table(MlsGroupMembers::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MlsGroupMembers {
    Table,
    GroupId,
    UserId,
    RoleInChat,
    JoinedAtEpoch,
    LeftAtEpoch,
    JoinedAt,
}

#[derive(DeriveIden)]
enum MlsGroups {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

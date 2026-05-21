use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MlsGroups::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MlsGroups::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(MlsGroups::GroupType).string().not_null())
                    .col(ColumnDef::new(MlsGroups::CurrentEpoch).big_integer().not_null())
                    .col(ColumnDef::new(MlsGroups::Ciphersuite).integer().not_null())
                    .col(ColumnDef::new(MlsGroups::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(MlsGroups::CreatedByUserId).uuid().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MlsGroups::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MlsGroups {
    Table,
    Id,
    GroupType,
    CurrentEpoch,
    Ciphersuite,
    CreatedAt,
    CreatedByUserId,
}

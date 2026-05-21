use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(InvitationTokens::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(InvitationTokens::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(InvitationTokens::TokenHash).blob().not_null().unique_key())
                    .col(ColumnDef::new(InvitationTokens::CreatedByUserId).uuid())
                    .col(ColumnDef::new(InvitationTokens::RoleToGrant).string().not_null())
                    .col(ColumnDef::new(InvitationTokens::MaxUses).integer().not_null())
                    .col(ColumnDef::new(InvitationTokens::UsesCount).integer().not_null())
                    .col(ColumnDef::new(InvitationTokens::ExpiresAt).big_integer().not_null())
                    .col(ColumnDef::new(InvitationTokens::RevokedAt).big_integer())
                    .col(ColumnDef::new(InvitationTokens::CreatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(InvitationTokens::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum InvitationTokens {
    Table,
    Id,
    TokenHash,
    CreatedByUserId,
    RoleToGrant,
    MaxUses,
    UsesCount,
    ExpiresAt,
    RevokedAt,
    CreatedAt,
}

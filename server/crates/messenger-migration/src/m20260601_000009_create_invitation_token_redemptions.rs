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
                    .table(InvitationTokenRedemptions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(InvitationTokenRedemptions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(InvitationTokenRedemptions::TokenId).uuid().not_null())
                    .col(
                        ColumnDef::new(InvitationTokenRedemptions::RedeemedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(InvitationTokenRedemptions::ResultUserId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(InvitationTokenRedemptions::ResultDeviceId)
                            .uuid()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_invitation_token_redemptions_token_id")
                            .from(
                                InvitationTokenRedemptions::Table,
                                InvitationTokenRedemptions::TokenId,
                            )
                            .to(InvitationTokens::Table, InvitationTokens::Id)
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
                "CREATE INDEX IF NOT EXISTS invitation_token_redemptions_token_id \
                 ON invitation_token_redemptions(token_id)"
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
                "DROP INDEX IF EXISTS invitation_token_redemptions_token_id".to_owned(),
            ))
            .await;
        manager
            .drop_table(
                Table::drop()
                    .table(InvitationTokenRedemptions::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum InvitationTokenRedemptions {
    Table,
    Id,
    TokenId,
    RedeemedAt,
    ResultUserId,
    ResultDeviceId,
}

#[derive(DeriveIden)]
enum InvitationTokens {
    Table,
    Id,
}

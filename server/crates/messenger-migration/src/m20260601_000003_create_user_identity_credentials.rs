use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserIdentityCredentials::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserIdentityCredentials::UserId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserIdentityCredentials::SignaturePublicKey)
                            .blob()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserIdentityCredentials::Credential)
                            .blob()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserIdentityCredentials::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_identity_credentials_user_id")
                            .from(UserIdentityCredentials::Table, UserIdentityCredentials::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserIdentityCredentials::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserIdentityCredentials {
    Table,
    UserId,
    SignaturePublicKey,
    Credential,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

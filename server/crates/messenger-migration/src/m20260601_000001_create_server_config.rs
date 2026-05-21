use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ServerConfig::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServerConfig::Id)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ServerConfig::ServerIdentitySecretKey).blob().not_null())
                    .col(ColumnDef::new(ServerConfig::ServerIdentityPublicKey).blob().not_null())
                    .col(ColumnDef::new(ServerConfig::UsernameBlindIndexKey).blob().not_null())
                    .col(ColumnDef::new(ServerConfig::UsernameHashVersion).integer().not_null())
                    .col(ColumnDef::new(ServerConfig::BootstrapTokenIssued).boolean().not_null())
                    .col(ColumnDef::new(ServerConfig::MlsCiphersuite).integer().not_null())
                    .col(ColumnDef::new(ServerConfig::SchemaVersion).integer().not_null())
                    .col(ColumnDef::new(ServerConfig::CreatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ServerConfig::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ServerConfig {
    Table,
    Id,
    ServerIdentitySecretKey,
    ServerIdentityPublicKey,
    UsernameBlindIndexKey,
    UsernameHashVersion,
    BootstrapTokenIssued,
    MlsCiphersuite,
    SchemaVersion,
    CreatedAt,
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(DeviceProvisioningRequests::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("new_device_temp_signing_public_key"))
                            .blob()
                            .not_null()
                            .default(vec![0u8; 32]),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(DeviceProvisioningRequests::Table)
                    .drop_column(Alias::new("new_device_temp_signing_public_key"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum DeviceProvisioningRequests {
    Table,
}

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "device_provisioning_requests")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub user_id: Uuid,
    pub new_device_temp_public_key: Vec<u8>,
    pub new_device_temp_signing_public_key: Vec<u8>,
    pub nonce: Vec<u8>,
    pub status: String,           // "pending" | "approved" | "expired" | "consumed"
    pub expires_at: i64,
    pub encrypted_bootstrap_blob: Option<Vec<u8>>,
    pub approved_by_device_id: Option<Uuid>,
    pub new_device_id: Option<Uuid>,
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mls_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub group_type: String,      // "direct" | "group"
    pub current_epoch: i64,
    pub ciphersuite: i32,
    pub created_at: i64,
    pub created_by_user_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::mls_group_members::Entity")]
    Members,
    #[sea_orm(has_many = "super::mls_group_devices::Entity")]
    Devices,
    #[sea_orm(has_many = "super::mls_messages::Entity")]
    Messages,
}

impl ActiveModelBehavior for ActiveModel {}

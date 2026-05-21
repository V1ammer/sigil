use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mls_welcomes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub group_id: Uuid,

    #[sea_orm(indexed)]
    pub recipient_device_id: Uuid,

    pub epoch: i64,
    pub welcome_ciphertext: Vec<u8>,
    pub created_at: i64,
    pub consumed_at: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mls_groups::Entity",
        from = "Column::GroupId",
        to = "super::mls_groups::Column::Id"
    )]
    Group,
}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mls_group_devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub device_id: Uuid,

    pub leaf_index: Option<i32>,     // позиция в MLS ratchet tree (опционально)
    pub added_at_epoch: i64,
    pub removed_at_epoch: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mls_groups::Entity",
        from = "Column::GroupId",
        to = "super::mls_groups::Column::Id"
    )]
    Group,
    #[sea_orm(
        belongs_to = "super::devices::Entity",
        from = "Column::DeviceId",
        to = "super::devices::Column::Id"
    )]
    Device,
}

impl Related<super::mls_groups::Entity> for Entity {
    fn to() -> RelationDef { Relation::Group.def() }
}

impl Related<super::devices::Entity> for Entity {
    fn to() -> RelationDef { Relation::Device.def() }
}

impl ActiveModelBehavior for ActiveModel {}

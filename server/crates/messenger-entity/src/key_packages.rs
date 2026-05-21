use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "key_packages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    #[sea_orm(indexed)]
    pub device_id: Uuid,

    pub key_package: Vec<u8>,           // serialized MLS KeyPackage

    #[sea_orm(unique)]
    pub init_key_hash: Vec<u8>,         // BLAKE3(init_key) для дедупа

    pub created_at: i64,
    pub expires_at: i64,                // от lifetime в KeyPackage
    pub consumed_at: Option<i64>,
    pub consumed_by_user_id: Option<Uuid>,

    /// Last-resort: переиспользуемый `KeyPackage`.
    pub is_last_resort: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::devices::Entity",
        from = "Column::DeviceId",
        to = "super::devices::Column::Id"
    )]
    Device,
}

impl Related<super::devices::Entity> for Entity {
    fn to() -> RelationDef { Relation::Device.def() }
}

impl ActiveModelBehavior for ActiveModel {}

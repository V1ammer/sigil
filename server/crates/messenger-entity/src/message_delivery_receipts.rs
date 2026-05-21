use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "message_delivery_receipts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub message_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub recipient_device_id: Uuid,

    pub delivered_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mls_messages::Entity",
        from = "Column::MessageId",
        to = "super::mls_messages::Column::Id"
    )]
    Message,
}

impl Related<super::mls_messages::Entity> for Entity {
    fn to() -> RelationDef { Relation::Message.def() }
}

impl ActiveModelBehavior for ActiveModel {}

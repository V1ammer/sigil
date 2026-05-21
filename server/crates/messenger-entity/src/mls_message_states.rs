use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mls_message_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub message_id: Uuid,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub replacement_message_id: Option<Uuid>,
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

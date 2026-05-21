use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mls_messages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,                          // UUIDv7 — sortable

    #[sea_orm(indexed)]
    pub group_id: Uuid,

    pub epoch: i64,

    pub sender_user_id: Uuid,
    pub sender_device_id: Uuid,

    pub wire_format: String,               // "application" | "commit" | "proposal"

    /// `MLSMessage` (`PrivateMessage` для application или `PublicMessage` для handshake).
    pub mls_ciphertext: Vec<u8>,

    pub parent_message_id: Option<Uuid>,
    pub thread_root_id: Option<Uuid>,
    pub reply_to_message_id: Option<Uuid>,

    pub client_message_id: Uuid,           // для идемпотентности

    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mls_groups::Entity",
        from = "Column::GroupId",
        to = "super::mls_groups::Column::Id"
    )]
    Group,
    #[sea_orm(has_one = "super::mls_message_states::Entity")]
    State,
    #[sea_orm(has_many = "super::message_delivery_receipts::Entity")]
    DeliveryReceipts,
    #[sea_orm(has_many = "super::attachments::Entity")]
    Attachments,
    #[sea_orm(has_many = "super::reactions::Entity")]
    Reactions,
}

impl Related<super::mls_groups::Entity> for Entity {
    fn to() -> RelationDef { Relation::Group.def() }
}

impl ActiveModelBehavior for ActiveModel {}

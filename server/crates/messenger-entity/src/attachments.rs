use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "attachments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    #[sea_orm(indexed)]
    pub message_id: Option<Uuid>,  // NULL до finalize, потом FK на mls_messages

    pub uploader_device_id: Uuid,

    /// Либо `payload_ciphertext` в БД для маленьких, либо `storage_ref` для object storage.
    pub payload_ciphertext: Option<Vec<u8>>,
    pub storage_ref: Option<String>,

    pub padded_size: i64,
    pub size_bucket: i32,

    pub created_at: i64,
    pub expires_at: i64,
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

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    #[sea_orm(indexed)]
    pub user_id: Uuid,

    pub hpke_init_public_key: Vec<u8>,
    pub device_signing_public_key: Vec<u8>,

    /// Подпись от `identity_signature_key` (или от authorizing device, если так решено).
    pub authorization_signature: Vec<u8>,

    pub authorized_by_device_id: Option<Uuid>,

    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub revoked_by_device_id: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id"
    )]
    User,
    #[sea_orm(has_many = "super::key_packages::Entity")]
    KeyPackages,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef { Relation::User.def() }
}

impl Related<super::key_packages::Entity> for Entity {
    fn to() -> RelationDef { Relation::KeyPackages.def() }
}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "invitation_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    #[sea_orm(unique)]
    pub token_hash: Vec<u8>,            // BLAKE3(raw_token)

    pub created_by_user_id: Option<Uuid>, // NULL для bootstrap-токена
    pub role_to_grant: String,
    pub max_uses: i32,
    pub uses_count: i32,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::invitation_token_redemptions::Entity")]
    Redemptions,
}

impl Related<super::invitation_token_redemptions::Entity> for Entity {
    fn to() -> RelationDef { Relation::Redemptions.def() }
}

impl ActiveModelBehavior for ActiveModel {}

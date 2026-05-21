use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "invitation_token_redemptions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,

    #[sea_orm(indexed)]
    pub token_id: Uuid,

    pub redeemed_at: i64,
    pub result_user_id: Uuid,
    pub result_device_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::invitation_tokens::Entity",
        from = "Column::TokenId",
        to = "super::invitation_tokens::Column::Id"
    )]
    Token,
}

impl Related<super::invitation_tokens::Entity> for Entity {
    fn to() -> RelationDef { Relation::Token.def() }
}

impl ActiveModelBehavior for ActiveModel {}

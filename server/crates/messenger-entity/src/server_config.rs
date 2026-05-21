use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "server_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i32, // всегда 1
    pub server_identity_secret_key: Vec<u8>,
    pub server_identity_public_key: Vec<u8>,
    pub username_blind_index_key: Vec<u8>,
    pub username_hash_version: i32,
    pub bootstrap_token_issued: bool,
    pub mls_ciphersuite: i32,
    pub schema_version: i32,
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

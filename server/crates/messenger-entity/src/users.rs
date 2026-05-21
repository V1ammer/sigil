use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    #[sea_orm(unique, indexed)]
    pub username_blind_index: Vec<u8>,

    pub username_hash_version: i32,

    pub role: String,    // "admin" | "user"
    pub status: String,  // "active" | "suspended" | "deleted"

    /// Округлено до суток (`unix_ts` / 86400 * 86400).
    pub created_at: i64,

    /// Whether this user wants to send read receipts.
    /// Default false (privacy by default).
    pub send_read_receipts: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::user_identity_credentials::Entity")]
    IdentityCredential,
    #[sea_orm(has_many = "super::devices::Entity")]
    Devices,
    #[sea_orm(has_many = "super::mls_group_members::Entity")]
    Memberships,
}

impl ActiveModelBehavior for ActiveModel {}

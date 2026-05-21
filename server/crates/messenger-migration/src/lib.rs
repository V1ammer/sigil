#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

pub use sea_orm_migration::prelude::*;

mod m20260601_000001_create_server_config;
mod m20260601_000002_create_users;
mod m20260601_000003_create_user_identity_credentials;
mod m20260601_000004_create_devices;
mod m20260601_000005_create_key_packages;
mod m20260601_000006_create_device_provisioning_requests;
mod m20260601_000007_create_key_change_events;
mod m20260601_000008_create_invitation_tokens;
mod m20260601_000009_create_invitation_token_redemptions;
mod m20260601_000010_create_mls_groups;
mod m20260601_000011_create_mls_group_members;
mod m20260601_000012_create_mls_group_devices;
mod m20260601_000013_create_mls_messages;
mod m20260601_000014_create_mls_message_states;
mod m20260601_000015_create_mls_welcomes;
mod m20260601_000016_create_attachments;
mod m20260601_000017_create_reactions;
mod m20260601_000018_create_message_delivery_receipts;
mod m20260601_000019_create_push_tokens;
mod m20260601_000020_add_provisioning_signing_key;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260601_000001_create_server_config::Migration),
            Box::new(m20260601_000002_create_users::Migration),
            Box::new(m20260601_000003_create_user_identity_credentials::Migration),
            Box::new(m20260601_000004_create_devices::Migration),
            Box::new(m20260601_000005_create_key_packages::Migration),
            Box::new(m20260601_000006_create_device_provisioning_requests::Migration),
            Box::new(m20260601_000007_create_key_change_events::Migration),
            Box::new(m20260601_000008_create_invitation_tokens::Migration),
            Box::new(m20260601_000009_create_invitation_token_redemptions::Migration),
            Box::new(m20260601_000010_create_mls_groups::Migration),
            Box::new(m20260601_000011_create_mls_group_members::Migration),
            Box::new(m20260601_000012_create_mls_group_devices::Migration),
            Box::new(m20260601_000013_create_mls_messages::Migration),
            Box::new(m20260601_000014_create_mls_message_states::Migration),
            Box::new(m20260601_000015_create_mls_welcomes::Migration),
            Box::new(m20260601_000016_create_attachments::Migration),
            Box::new(m20260601_000017_create_reactions::Migration),
            Box::new(m20260601_000018_create_message_delivery_receipts::Migration),
            Box::new(m20260601_000019_create_push_tokens::Migration),
            Box::new(m20260601_000020_add_provisioning_signing_key::Migration),
        ]
    }
}

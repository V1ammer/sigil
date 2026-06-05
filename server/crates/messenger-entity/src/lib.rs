#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

pub mod prelude;

pub mod attachments;
pub mod device_provisioning_requests;
pub mod devices;
pub mod invitation_token_redemptions;
pub mod invitation_tokens;
pub mod key_change_events;
pub mod key_packages;
pub mod message_delivery_receipts;
pub mod mls_group_devices;
pub mod mls_group_members;
pub mod mls_groups;
pub mod mls_message_states;
pub mod mls_messages;
pub mod mls_welcomes;
pub mod reactions;
pub mod server_config;
pub mod user_identity_credentials;
pub mod users;

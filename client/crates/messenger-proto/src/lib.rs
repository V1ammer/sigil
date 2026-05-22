//! Wire-format types shared between client and server.

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

pub mod admin;
pub mod auth;
pub mod attachments;
pub mod error;
pub mod invites;
pub mod keypackages;
pub mod mls;
pub mod provisioning;
pub mod reactions;
pub mod server;
pub mod users;
pub mod ws;

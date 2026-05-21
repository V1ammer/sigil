#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;
pub mod telemetry;

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    cli::run_cli(messenger_migration::Migrator).await;
}

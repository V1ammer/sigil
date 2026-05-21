#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use std::sync::Arc;

use messenger_server::bootstrap;
use messenger_server::config::AppConfig;
use messenger_server::db;
use messenger_server::routes::build_router;
use messenger_server::state::{AppState, NonceCache};
use messenger_server::tasks;
use messenger_server::telemetry;
use tower_http::limit::RequestBodyLimitLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    let _ = dotenvy::dotenv();

    let config = AppConfig::from_env()?;
    telemetry::init(&config);

    let db = db::connect(&config).await?;
    db::run_migrations(&db).await?;

    let identity = bootstrap::load_or_init(&db).await?;
    let nonce_cache = Arc::new(NonceCache::new(config.nonce_cache_capacity));

    let state = AppState {
        db,
        config: Arc::new(config.clone()),
        nonce_cache,
        server_identity: Arc::new(identity),
    };

    // Запуск GC для provisioning-запросов
    tokio::spawn(tasks::provisioning_gc::run_provisioning_gc(state.db.clone()));

    let app = build_router(state.clone())
        .layer(telemetry::trace_layer())
        .layer(RequestBodyLimitLayer::new(
            state.config.max_request_body_bytes,
        ))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ));

    let listener = tokio::net::TcpListener::bind(state.config.bind_addr).await?;
    tracing::info!(addr = %state.config.bind_addr, "server started");
    axum::serve(listener, app).await?;
    Ok(())
}

use tower_http::trace::{MakeSpan, TraceLayer};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, registry};
use uuid::Uuid;

use crate::config::{AppConfig, LogFormat};

/// Инициализирует tracing subscriber с заданным форматом и уровнем.
pub fn init(config: &AppConfig) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let registry = registry().with(env_filter);

    match config.log_format {
        LogFormat::Json => {
            registry
                .with(
                    fmt::layer()
                        .json()
                        .with_current_span(true)
                        .with_span_list(true),
                )
                .init();
        }
        LogFormat::Pretty => {
            registry.with(fmt::layer().pretty()).init();
        }
    }
}

/// `MakeSpan`, который пишет только method, path и `request_id`.
/// Никаких IP, UA, тел, заголовков.
#[derive(Clone)]
pub struct SafeMakeSpan;

impl<B> MakeSpan<B> for SafeMakeSpan {
    fn make_span(&mut self, req: &axum::http::Request<B>) -> tracing::Span {
        tracing::info_span!(
            "http_request",
            method = %req.method(),
            path = %req.uri().path(),
            request_id = %Uuid::now_v7(),
        )
    }
}

/// Создаёт `TraceLayer` с `SafeMakeSpan`.
#[must_use]
pub fn trace_layer() -> TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    SafeMakeSpan,
    tower_http::trace::DefaultOnRequest,
    tower_http::trace::DefaultOnResponse,
    tower_http::trace::DefaultOnBodyChunk,
    tower_http::trace::DefaultOnEos,
    tower_http::trace::DefaultOnFailure,
> {
    TraceLayer::new_for_http().make_span_with(SafeMakeSpan)
}

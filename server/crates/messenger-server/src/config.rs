use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use figment::providers::{Env, Serialized};
use figment::Figment;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum LogFormat {
    #[default]
    Json,
    Pretty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: SocketAddr,

    /// URL подключения к БД (обязателен).
    pub database_url: String,

    /// Куда складывать attachments если в FS.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Максимальный размер тела запроса.
    #[serde(default = "default_max_request_body_bytes")]
    pub max_request_body_bytes: usize,

    /// Таймаут WebSocket idle.
    #[serde(default = "default_websocket_idle_timeout_secs")]
    pub websocket_idle_timeout_secs: u64,

    /// Допуск skew часов для signed challenge.
    #[serde(default = "default_clock_skew_tolerance_secs")]
    pub clock_skew_tolerance_secs: i64,

    /// Ёмкость кеша nonce.
    #[serde(default = "default_nonce_cache_capacity")]
    pub nonce_cache_capacity: usize,

    /// Формат логов.
    #[serde(default)]
    pub log_format: LogFormat,

    /// Уровень логирования (RUST_LOG-совместимый).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// TTL незафинализированного attachment'а.
    #[serde(default = "default_attachment_ttl_unfinalized_secs")]
    pub attachment_ttl_unfinalized_secs: u64,

    /// Lifetime `KeyPackage` по умолчанию.
    #[serde(default = "default_keypackage_lifetime_secs")]
    pub keypackage_default_lifetime_secs: u64,
}

fn default_bind_addr() -> SocketAddr {
    SocketAddr::from_str("127.0.0.1:8080").unwrap()
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

fn default_max_request_body_bytes() -> usize {
    16 * 1024 * 1024 // 16 MB
}

fn default_websocket_idle_timeout_secs() -> u64 {
    120
}

fn default_clock_skew_tolerance_secs() -> i64 {
    60
}

fn default_nonce_cache_capacity() -> usize {
    10_000
}

fn default_log_level() -> String {
    "info,messenger=debug".to_string()
}

fn default_attachment_ttl_unfinalized_secs() -> u64 {
    3600
}

fn default_keypackage_lifetime_secs() -> u64 {
    90 * 24 * 3600
}

impl AppConfig {
    /// Загружает конфиг из переменных окружения с префиксом `MESSENGER_`.
    ///
    /// # Errors
    ///
    /// Возвращает `AppError::Config` если обязательные поля отсутствуют
    /// или валидация не пройдена.
    pub fn from_env() -> Result<Self, crate::error::AppError> {
        let config: AppConfig = Figment::new()
            .merge(Serialized::defaults(AppConfig::default()))
            .merge(Env::prefixed("MESSENGER_").global())
            .extract()
            .map_err(|e| crate::error::AppError::Config(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), crate::error::AppError> {
        if self.database_url.is_empty() {
            return Err(crate::error::AppError::Config(
                "MESSENGER_DATABASE_URL must be set".to_string(),
            ));
        }
        if self.clock_skew_tolerance_secs <= 0 || self.clock_skew_tolerance_secs > 600 {
            return Err(crate::error::AppError::Config(
                "MESSENGER_CLOCK_SKEW_TOLERANCE_SECS must be in (0, 600]".to_string(),
            ));
        }
        // Создаём data_dir если не существует
        if !self.data_dir.exists() {
            std::fs::create_dir_all(&self.data_dir).map_err(|e| {
                crate::error::AppError::Config(format!(
                    "cannot create data_dir {}: {e}",
                    self.data_dir.display()
                ))
            })?;
        }
        if !self.data_dir.is_dir() {
            return Err(crate::error::AppError::Config(format!(
                "data_dir {} is not a directory",
                self.data_dir.display()
            )));
        }
        Ok(())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            database_url: String::new(),
            data_dir: default_data_dir(),
            max_request_body_bytes: default_max_request_body_bytes(),
            websocket_idle_timeout_secs: default_websocket_idle_timeout_secs(),
            clock_skew_tolerance_secs: default_clock_skew_tolerance_secs(),
            nonce_cache_capacity: default_nonce_cache_capacity(),
            log_format: LogFormat::Json,
            log_level: default_log_level(),
            attachment_ttl_unfinalized_secs: default_attachment_ttl_unfinalized_secs(),
            keypackage_default_lifetime_secs: default_keypackage_lifetime_secs(),
        }
    }
}

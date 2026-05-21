use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;

use crate::attachments::StorageBackend;
use crate::config::AppConfig;
pub use crate::identity::ServerIdentity;
pub use crate::ws_registry::WsRegistry;

/// LRU-кеш для nonce (защита от replay-attacks).
///
/// Nonce хранятся 180 секунд — чуть больше окна `clock_skew` (60s),
/// чтобы старые nonce ещё были в кэше, когда таймстамп уже отвергнется отдельно.
pub struct NonceCache {
    inner: moka::sync::Cache<Vec<u8>, ()>,
}

impl NonceCache {
    /// Создаёт новый кеш с указанной capacity и TTL 180 секунд.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let inner = moka::sync::Cache::builder()
            .max_capacity(capacity as u64)
            .time_to_live(Duration::from_secs(180))
            .build();
        Self { inner }
    }

    /// Проверяет nonce: возвращает `true` если nonce уже был (replay).
    /// Если nonce новый — вставляет и возвращает `false`.
    #[must_use]
    pub fn check_and_insert(&self, nonce: &[u8]) -> bool {
        let key = nonce.to_vec();
        if self.inner.contains_key(&key) {
            return true; // уже существует — replay
        }
        self.inner.insert(key, ());
        false
    }
}

/// Разделяемое состояние приложения.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<AppConfig>,
    pub nonce_cache: Arc<NonceCache>,
    pub server_identity: Arc<ServerIdentity>,
    pub storage: StorageBackend,
    pub ws_registry: WsRegistry,
}

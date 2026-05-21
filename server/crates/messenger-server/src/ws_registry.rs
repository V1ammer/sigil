//! Реестр активных WebSocket-соединений.
//!
//! Позволяет отправлять push-уведомления устройствам пользователей
//! в реальном времени через их WebSocket-подключения.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Фрейм, отправляемый сервером клиенту через WebSocket.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    AuthOk { user_id: Uuid },
    AuthError { code: String },
    Pong,
    NewMessage { group_id: Uuid, message_id: Uuid, epoch: i64 },
    NewWelcome { welcome_id: Uuid, group_id: Uuid },
    KeyChange { user_id: Uuid, device_id: Uuid, event: String },
    Typing { group_id: Uuid, user_id: Uuid, started: bool },
    Error { code: String, message: Option<String> },
}

/// Реестр активных WebSocket-подключений.
///
/// Структура: `user_id → device_id → bounded mpsc::Sender<ServerFrame>`.
/// Использует `DashMap` для lock-free concurrent доступа.
#[derive(Clone)]
pub struct WsRegistry {
    /// `user_id → device_id → Sender`
    inner: Arc<DashMap<Uuid, DashMap<Uuid, mpsc::Sender<ServerFrame>>>>,
}

impl WsRegistry {
    /// Создаёт новый пустой реестр.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Регистрирует новое WebSocket-подключение.
    ///
    /// Если устройство уже имеет активное подключение — старое соединение
    /// разрывается (sender канала закрывается, и writer-task завершится).
    pub fn register(&self, user_id: Uuid, device_id: Uuid, tx: mpsc::Sender<ServerFrame>) {
        let user_map = self.inner.entry(user_id).or_default();
        // Заменяем старое подключение если есть (разрыв через Drop старого tx)
        user_map.insert(device_id, tx);
    }

    /// Удаляет регистрацию WebSocket-подключения.
    pub fn unregister(&self, user_id: Uuid, device_id: Uuid) {
        if let Some(user_map) = self.inner.get(&user_id) {
            // Удаляем только если это всё ещё наш sender
            user_map.remove(&device_id);
            // Если у пользователя больше нет устройств — удаляем запись
            if user_map.is_empty() {
                drop(user_map);
                self.inner.remove(&user_id);
            }
        }
    }

    /// Отправляет фрейм конкретному устройству пользователя.
    ///
    /// Использует `try_send` — если канал переполнен (backpressure)
    /// или закрыт, фрейм тихо отбрасывается (клиент переподключится).
    pub fn send_to_device(&self, user_id: Uuid, device_id: Uuid, frame: ServerFrame) {
        if let Some(user_map) = self.inner.get(&user_id) {
            if let Some(tx) = user_map.get(&device_id) {
                let _ = tx.try_send(frame);
            }
        }
    }

    /// Отправляет фрейм всем устройствам пользователя.
    pub fn send_to_user(&self, user_id: Uuid, frame: &ServerFrame) {
        if let Some(user_map) = self.inner.get(&user_id) {
            for tx in user_map.iter() {
                let _ = tx.try_send(frame.clone());
            }
        }
    }
}

impl Default for WsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

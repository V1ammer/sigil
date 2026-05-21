//! Публичные endpoints для работы с инвайт-токенами.
//!
//! В S06 — только заглушка для redeem. Полная реализация в S07.

use axum::http::StatusCode;

/// `POST /v1/invite/redeem` — заглушка.
///
/// Полная реализация будет в S07 (создание пользователя/устройства).
/// Сейчас возвращает 501 Not Implemented, чтобы тесты могли проверить
/// что endpoint существует и маршрутизация работает.
pub async fn redeem_stub() -> (StatusCode, &'static str) {
    (StatusCode::NOT_IMPLEMENTED, "not implemented yet — S07")
}

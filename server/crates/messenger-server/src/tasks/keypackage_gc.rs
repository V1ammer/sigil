//! Фоновая задача для очистки просроченных `KeyPackages`.
//!
//! Запускается раз в час. Удаляет записи с `expires_at < now - 86400`
//! (grace period 1 день после истечения).

use std::time::Duration;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::services::invite::now_secs;

/// Запускает GC цикл для `KeyPackages`.
///
/// Бесконечный цикл с интервалом 3600 секунд. При ошибке пишет warning в лог.
pub async fn run_keypackage_gc(db: DatabaseConnection) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        let now = now_secs();
        let res = messenger_entity::key_packages::Entity::delete_many()
            .filter(messenger_entity::key_packages::Column::ExpiresAt.lt(now - 86_400))
            .exec(&db)
            .await;
        if let Err(e) = res {
            tracing::warn!(error = ?e, "keypackage gc failed");
        }
    }
}

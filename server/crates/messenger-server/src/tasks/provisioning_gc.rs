//! Фоновая задача для очистки просроченных provisioning-запросов.
//!
//! Запускается раз в 60 секунд. Удаляет записи с `expires_at < now`
//! и статусом `pending`, `expired` или `consumed`.

use std::time::Duration;

use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};

use crate::services::invite::now_secs;

/// Запускает GC цикл для provisioning-запросов.
///
/// Бесконечный цикл с интервалом 60 секунд. При ошибке пишет warning в лог.
pub async fn run_provisioning_gc(db: DatabaseConnection) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let now = now_secs();
        let res = messenger_entity::device_provisioning_requests::Entity::delete_many()
            .filter(
                Condition::all()
                    .add(
                        messenger_entity::device_provisioning_requests::Column::ExpiresAt
                            .lt(now),
                    )
                    .add(
                        Condition::any()
                            .add(
                                messenger_entity::device_provisioning_requests::Column::Status
                                    .eq("pending"),
                            )
                            .add(
                                messenger_entity::device_provisioning_requests::Column::Status
                                    .eq("expired"),
                            )
                            .add(
                                messenger_entity::device_provisioning_requests::Column::Status
                                    .eq("consumed"),
                            ),
                    ),
            )
            .exec(&db)
            .await;
        if let Err(e) = res {
            tracing::warn!(error = ?e, "provisioning gc failed");
        }
    }
}

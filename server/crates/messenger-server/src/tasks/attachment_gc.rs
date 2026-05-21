//! Фоновая задача для очистки незафинализированных просроченных вложений.
//!
//! Запускается раз в 300 секунд (5 минут). Находит attachment'ы с
//! `message_id IS NULL` и `expires_at < now`, удаляет файлы с диска и записи из БД.

use std::time::Duration;

use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};

use crate::attachments::{StorageBackend, StoredRef};
use crate::services::invite::now_secs;

/// Запускает GC цикл для attachment'ов.
///
/// Бесконечный цикл с интервалом 300 секунд. При ошибке пишет warning в лог.
pub async fn run_attachment_gc(db: DatabaseConnection, storage: StorageBackend) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        let now = now_secs();

        let stale = messenger_entity::attachments::Entity::find()
            .filter(
                Condition::all()
                    .add(messenger_entity::attachments::Column::MessageId.is_null())
                    .add(messenger_entity::attachments::Column::ExpiresAt.lt(now)),
            )
            .all(&db)
            .await;

        let stale = match stale {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = ?e, "attachment gc query failed");
                continue;
            }
        };

        for a in &stale {
            // Удалить файл с диска если on-disk
            if let Some(ref sref_str) = a.storage_ref {
                #[allow(clippy::cast_sign_loss)]
                let size = a.padded_size as u64;
                let sref = StoredRef::OnDisk {
                    relative_path: std::path::PathBuf::from(sref_str),
                    size,
                };
                if let Err(e) = storage.delete(&sref).await {
                    tracing::warn!(
                        attachment_id = %a.id,
                        error = ?e,
                        "attachment gc: failed to delete file"
                    );
                }
            }

            if let Err(e) = messenger_entity::attachments::Entity::delete_by_id(a.id)
                .exec(&db)
                .await
            {
                tracing::warn!(
                    attachment_id = %a.id,
                    error = ?e,
                    "attachment gc: failed to delete row"
                );
            }
        }

        if !stale.is_empty() {
            tracing::info!(count = stale.len(), "attachment gc: cleaned up stale attachments");
        }
    }
}

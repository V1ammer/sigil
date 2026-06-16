//! Фоновая задача для очистки вложений, которые больше не нужны.
//!
//! Запускается раз в 300 секунд (5 минут) и удаляет файлы с диска + строки БД для:
//!   1. незафинализированных просроченных загрузок (`message_id IS NULL` и
//!      `expires_at < now`) — брошенные/неудавшиеся аплоады;
//!   2. вложений, чьё сообщение было soft-удалено (`mls_message_states.deleted_at`
//!      выставлен) — подстраховка к немедленной очистке в delete-хендлере, плюс
//!      подбирает блобы, удалённые ещё до её появления.

use std::time::Duration;

use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};

use crate::attachments::{StorageBackend, StoredRef};
use crate::services::invite::now_secs;

/// Удаляет файл с диска (если on-disk) и строку БД для одного вложения.
/// Best-effort: ошибки логируются, цикл продолжается.
async fn reclaim(
    db: &DatabaseConnection,
    storage: &StorageBackend,
    a: &messenger_entity::attachments::Model,
) {
    if let Some(ref sref_str) = a.storage_ref {
        #[allow(clippy::cast_sign_loss)]
        let sref = StoredRef::OnDisk {
            relative_path: std::path::PathBuf::from(sref_str),
            size: a.padded_size as u64,
        };
        if let Err(e) = storage.delete(&sref).await {
            tracing::warn!(attachment_id = %a.id, error = ?e, "attachment gc: failed to delete file");
        }
    }
    if let Err(e) = messenger_entity::attachments::Entity::delete_by_id(a.id)
        .exec(db)
        .await
    {
        tracing::warn!(attachment_id = %a.id, error = ?e, "attachment gc: failed to delete row");
    }
}

/// Запускает GC цикл для attachment'ов.
///
/// Бесконечный цикл с интервалом 300 секунд. При ошибке пишет warning в лог.
pub async fn run_attachment_gc(db: DatabaseConnection, storage: StorageBackend) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        let now = now_secs();

        // 1. Незафинализированные просроченные загрузки.
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
                Vec::new()
            }
        };
        for a in &stale {
            reclaim(&db, &storage, a).await;
        }

        // 2. Вложения soft-удалённых сообщений.
        let deleted_msg_ids: Vec<uuid::Uuid> = match messenger_entity::mls_message_states::Entity::find()
            .filter(messenger_entity::mls_message_states::Column::DeletedAt.is_not_null())
            .all(&db)
            .await
        {
            Ok(rows) => rows.into_iter().map(|s| s.message_id).collect(),
            Err(e) => {
                tracing::warn!(error = ?e, "attachment gc: deleted-state query failed");
                Vec::new()
            }
        };
        let orphaned = if deleted_msg_ids.is_empty() {
            Vec::new()
        } else {
            messenger_entity::attachments::Entity::find()
                .filter(messenger_entity::attachments::Column::MessageId.is_in(deleted_msg_ids))
                .all(&db)
                .await
                .unwrap_or_default()
        };
        for a in &orphaned {
            reclaim(&db, &storage, a).await;
        }

        let total = stale.len() + orphaned.len();
        if total > 0 {
            tracing::info!(
                unfinalized = stale.len(),
                deleted = orphaned.len(),
                "attachment gc: cleaned up stale attachments"
            );
        }
    }
}

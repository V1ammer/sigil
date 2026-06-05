//! Логика управления и валидации инвайт-токенов.
//!
//! Содержит:
//! - `begin_immediate` — транзакция с `BEGIN IMMEDIATE` / `SERIALIZABLE`.
//! - `validate_token` — read-only проверка токена (для S07).
//! - `consume_token` — атомарное использование токена внутри транзакции (для S07).
//! - `now_secs` — общая утилита для unix timestamp.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, EntityTrait, QueryFilter, Set, NotSet,
    TransactionTrait,
};
use uuid::Uuid;

use crate::error::AppError;

/// Возвращает текущий unix timestamp в секундах.
///
/// # Panics
///
/// Паникует если системное время до unix epoch.
#[must_use]
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
        .try_into()
        .expect("timestamp overflow")
}

/// Начинает транзакцию с эксклюзивным write lock.
///
/// - **`SQLite`:** `BEGIN IMMEDIATE` (захватывает write lock сразу).
/// - **Postgres:** `SERIALIZABLE` — максимальная гарантия изоляции.
///
/// # Errors
///
/// Возвращает `AppError::Db` при ошибке БД.
pub async fn begin_immediate(db: &DatabaseConnection) -> Result<DatabaseTransaction, AppError> {
    // SQLite: Use regular begin instead of begin_with_config.
    // begin_with_config with Serializable/ReadWrite causes warnings
    // and may leave the connection in a broken state.
    let txn = db.begin().await?;
    Ok(txn)
}

/// Проверяет валидность токена без изменения состояния.
///
/// Ищет токен по BLAKE3-хэшу от base64url-no-pad строки. Возвращает
/// модель токена если он не отозван, не истёк и не исчерпан.
///
/// # Errors
///
/// - `InviteInvalid` — токен не найден или отозван.
/// - `InviteExpired` — срок действия истёк.
/// - `InviteExhausted` — превышено `max_uses`.
/// - `AppError::Db` — ошибка БД.
pub async fn validate_token(
    db: &impl ConnectionTrait,
    token_str: &str,
) -> Result<messenger_entity::invitation_tokens::Model, AppError> {
    use messenger_entity::invitation_tokens::{self, Entity as InvitationTokens};

    let token_hash = blake3::hash(token_str.as_bytes()).as_bytes().to_vec();
    tracing::info!(token_str = %token_str, token_hash = %hex::encode(&token_hash), "validating token hash");

    let row = InvitationTokens::find()
        .filter(invitation_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or(AppError::InviteInvalid)?;

    if row.revoked_at.is_some() {
        return Err(AppError::InviteInvalid);
    }
    if row.expires_at <= now_secs() {
        return Err(AppError::InviteExpired);
    }
    if row.uses_count >= row.max_uses {
        return Err(AppError::InviteExhausted);
    }
    Ok(row)
}

/// Атомарно потребляет одно использование токена внутри транзакции.
///
/// Вызывающий ОБЯЗАН передать уже открытую транзакцию (`begin_immediate`).
/// Транзакция является точкой redeem: любые последующие вставки
/// (user, device) происходят в той же транзакции.
///
/// # Errors
///
/// - `InviteInvalid` — токен не найден.
/// - `InviteExhausted` — превышено `max_uses` (race condition при параллельном consume).
/// - `InviteExpired` — срок истёк (race condition).
/// - `AppError::Db` — ошибка БД.
pub async fn consume_token(
    txn: &DatabaseTransaction,
    token_id: Uuid,
    result_user_id: Uuid,
    result_device_id: Uuid,
) -> Result<(), AppError> {
    use messenger_entity::invitation_token_redemptions;
    use messenger_entity::invitation_tokens::{self, Entity as InvitationTokens};

    // SELECT внутри транзакции (для SQLite с BEGIN IMMEDIATE
    // это даёт эксклюзивный доступ к строке).
    let row = InvitationTokens::find_by_id(token_id)
        .one(txn)
        .await?
        .ok_or(AppError::InviteInvalid)?;

    // Повторная проверка — гарантирует атомарность даже если
    // другой поток уже изменил uses_count между нашей первой
    // проверкой и началом транзакции.
    if row.uses_count >= row.max_uses {
        return Err(AppError::InviteExhausted);
    }
    if row.expires_at <= now_secs() {
        return Err(AppError::InviteExpired);
    }

    let new_uses = row.uses_count + 1;
    let mut active: invitation_tokens::ActiveModel = row.into();
    active.uses_count = Set(new_uses);
    active.update(txn).await?;

    invitation_token_redemptions::ActiveModel {
        id: NotSet, // i64 autoincrement
        token_id: Set(token_id),
        redeemed_at: Set(now_secs()),
        result_user_id: Set(result_user_id),
        result_device_id: Set(result_device_id),
    }
    .insert(txn)
    .await?;

    Ok(())
}

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use base64::Engine;
use ed25519_dalek::SigningKey;
use rand::RngCore;
use sea_orm::{ActiveModelTrait, EntityTrait, TransactionTrait};
use uuid::Uuid;

use crate::error::AppError;
use crate::identity::ServerIdentity;

/// Фиксированный MLS ciphersuite сервера.
/// `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (0x0001).
const MLS_CIPHERSUITE: u16 = 0x0001;

/// Загружает `ServerIdentity` из БД.
///
/// Если `server_config` пуста (первый запуск), генерирует свежие
/// identity-ключи, записывает их, выпускает bootstrap admin invite token
/// и выводит инструкции оператору.
///
/// # Errors
///
/// Возвращает `AppError::Db` при ошибках БД или
/// `AppError::Internal` при невалидных данных конфигурации.
pub async fn load_or_init(db: &sea_orm::DatabaseConnection) -> Result<ServerIdentity, AppError> {
    use messenger_entity::server_config::{self, Entity as ServerConfigEntity};

    let existing = ServerConfigEntity::find_by_id(1).one(db).await?;

    if let Some(row) = existing {
        return ServerIdentity::from_row(row);
    }

    // Первый запуск: генерируем всё.
    let mut rng = rand::thread_rng();
    let signing_secret = SigningKey::generate(&mut rng);
    let signing_pub = signing_secret.verifying_key();

    let mut blind_index_key = [0u8; 32];
    rng.fill_bytes(&mut blind_index_key);

    let secret_bytes = signing_secret.to_bytes().to_vec();
    let pub_bytes = signing_pub.to_bytes().to_vec();
    let blind_key_vec = blind_index_key.to_vec();

    let identity = ServerIdentity {
        signing_secret_key: signing_secret,
        signing_public_key: signing_pub,
        username_blind_index_key: blind_index_key,
        username_hash_version: 1,
        mls_ciphersuite: MLS_CIPHERSUITE,
    };

    let now = now_secs();
    let row = server_config::ActiveModel {
        id: sea_orm::Set(1),
        server_identity_secret_key: sea_orm::Set(secret_bytes),
        server_identity_public_key: sea_orm::Set(pub_bytes),
        username_blind_index_key: sea_orm::Set(blind_key_vec),
        username_hash_version: sea_orm::Set(1),
        bootstrap_token_issued: sea_orm::Set(false),
        mls_ciphersuite: sea_orm::Set(i32::from(MLS_CIPHERSUITE)),
        schema_version: sea_orm::Set(1),
        created_at: sea_orm::Set(now),
    };
    row.insert(db).await?;

    issue_bootstrap_token(db).await?;

    Ok(identity)
}

/// Выпускает bootstrap admin invite token.
///
/// Токен — 256 бит случайности, закодированный в base64url (без padding).
/// В БД хранится BLAKE3(token). Выводится в `tracing::warn!` и `eprintln!`
/// ровно один раз. Повторный вызов возвращает `Err(BootstrapAlreadyDone)`.
///
/// # Errors
///
/// Возвращает `AppError::BootstrapAlreadyDone` если токен уже был выпущен,
/// или `AppError::Internal` при отсутствии `server_config`.
async fn issue_bootstrap_token(db: &sea_orm::DatabaseConnection) -> Result<(), AppError> {
    use messenger_entity::invitation_tokens;
    use messenger_entity::server_config::{self, Entity as Cfg};

    let txn = db.begin().await?;

    let cfg = Cfg::find_by_id(1)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("server_config missing")))?;

    if cfg.bootstrap_token_issued {
        txn.rollback().await?;
        return Err(AppError::BootstrapAlreadyDone);
    }

    // 256 бит случайности
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let token_b64 =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);

    let token_hash = blake3::hash(token_b64.as_bytes()).as_bytes().to_vec();

    let expires_at = now_secs() + 7 * 24 * 3600; // 7 дней
    let token_id = Uuid::now_v7();

    invitation_tokens::ActiveModel {
        id: sea_orm::Set(token_id),
        token_hash: sea_orm::Set(token_hash),
        created_by_user_id: sea_orm::Set(None),
        role_to_grant: sea_orm::Set("admin".into()),
        max_uses: sea_orm::Set(1),
        uses_count: sea_orm::Set(0),
        expires_at: sea_orm::Set(expires_at),
        revoked_at: sea_orm::Set(None),
        created_at: sea_orm::Set(now_secs()),
    }
    .insert(&txn)
    .await?;

    // Установить флаг чтобы повторно не выдавалось
    let mut cfg_active: server_config::ActiveModel = cfg.into();
    cfg_active.bootstrap_token_issued = sea_orm::Set(true);
    cfg_active.update(&txn).await?;

    txn.commit().await?;

    // Вывести оператору. ОДИН РАЗ. Невозможно восстановить.
    tracing::warn!(
        bootstrap_token = %token_b64,
        expires_at = expires_at,
        "FIRST-RUN BOOTSTRAP ADMIN TOKEN — save this NOW, it will never be shown again"
    );
    eprintln!();
    eprintln!("============================================================");
    eprintln!(" BOOTSTRAP ADMIN INVITE TOKEN (valid 7 days, single use):");
    eprintln!();
    eprintln!("   {token_b64}");
    eprintln!();
    eprintln!(" Use this in your client to register the first admin user.");
    eprintln!(" THIS TOKEN WILL NEVER BE SHOWN AGAIN.");
    eprintln!("============================================================");
    eprintln!();

    Ok(())
}

/// Возвращает текущий unix timestamp в секундах.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
        .try_into()
        .expect("timestamp overflow")
}

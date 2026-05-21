#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

use ed25519_dalek::{SigningKey, VerifyingKey};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::AppError;

type HmacSha256 = Hmac<Sha256>;

/// Identity-ключи и параметры сервера, загружаемые из `server_config`.
///
/// Содержит серверный Ed25519 signing keypair, HMAC-ключ для blind index
/// username'ов, версию схемы хеширования и фиксированный MLS ciphersuite.
pub struct ServerIdentity {
    /// Серверный Ed25519 signing key для подписи токенов и challenge'ов.
    pub signing_secret_key: SigningKey,
    pub signing_public_key: VerifyingKey,

    /// HMAC-ключ для blind index username (32 байта).
    pub username_blind_index_key: [u8; 32],

    /// Версия HMAC-ключа (для ротации).
    pub username_hash_version: i32,

    /// Фиксированный MLS ciphersuite сервера.
    pub mls_ciphersuite: u16,
}

impl ServerIdentity {
    /// Заглушка, используемая **только** до вызова `load_or_init`.
    ///
    /// Никогда не должна использоваться для реальных криптографических операций.
    #[must_use]
    pub fn placeholder() -> Self {
        let zero_key = SigningKey::from_bytes(&[0u8; 32]);
        let pub_key = zero_key.verifying_key();
        Self {
            signing_secret_key: zero_key,
            signing_public_key: pub_key,
            username_blind_index_key: [0u8; 32],
            username_hash_version: 0,
            mls_ciphersuite: 0,
        }
    }

    /// Создаёт `ServerIdentity` из строки `server_config`.
    ///
    /// # Errors
    ///
    /// Возвращает `AppError::Internal` если ключи имеют неверную длину.
    pub fn from_row(row: messenger_entity::server_config::Model) -> Result<Self, AppError> {
        let secret_bytes: [u8; 32] = row
            .server_identity_secret_key
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid server secret key length")))?;
        let signing_secret_key = SigningKey::from_bytes(&secret_bytes);
        let signing_public_key = signing_secret_key.verifying_key();

        let blind_key: [u8; 32] = row
            .username_blind_index_key
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid blind index key length")))?;

        Ok(Self {
            signing_secret_key,
            signing_public_key,
            username_blind_index_key: blind_key,
            username_hash_version: row.username_hash_version,
            mls_ciphersuite: u16::try_from(row.mls_ciphersuite)
                .map_err(|_| AppError::Internal(anyhow::anyhow!(
                    "invalid mls_ciphersuite value: {}", row.mls_ciphersuite
                )))?,
        })
    }

    /// Вычисляет blind index для канонизированного username.
    ///
    /// Использует HMAC-SHA256 с серверным `username_blind_index_key`.
    ///
    /// # Panics
    ///
    /// Паникует если `username_blind_index_key` не 32 байта (это инвариант,
    /// гарантируется конструктором `from_row` или `placeholder`).
    #[must_use]
    pub fn blind_index(&self, username: &str) -> Vec<u8> {
        let canon = canonicalize_username(username);
        let mut mac = HmacSha256::new_from_slice(&self.username_blind_index_key)
            .expect("HMAC key length is valid for Sha256");
        mac.update(canon.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }
}

/// Каноническая форма username для blind index: NFKC normalize + lowercase + trim.
///
/// NFKC схлопывает совместимые формы (напр. "ﬃ" → "ffi") и
/// уменьшает путаницу с visually similar символами.
#[must_use]
pub fn canonicalize_username(input: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    input
        .nfkc()
        .collect::<String>()
        .to_lowercase()
        .trim()
        .to_string()
}

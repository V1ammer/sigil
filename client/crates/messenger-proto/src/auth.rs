use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Тип redeem-запроса (клиентская копия, совместимая с сервером).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RedeemKind {
    NewUser,
    NewDevice,
}

/// Request to redeem an invite token.
///
/// Должен совпадать с серверной структурой `server/.../routes/invite.rs`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RedeemRequest {
    /// Инвайт-токен как строка (base64url-no-pad).
    pub token: String,
    /// Тип регистрации.
    pub kind: RedeemKind,
    /// MLS BasicCredential (сериализованный).
    #[serde(default)]
    pub identity_credential: Option<Vec<u8>>,
    /// Ed25519 public key пользователя (32 байта).
    #[serde(default)]
    pub signature_public_key: Option<Vec<u8>>,
    /// Plaintext username.
    pub username: Option<String>,
    /// Подпись challenge'а существующим identity-ключом (только NewDevice).
    #[serde(default)]
    pub existing_identity_proof: Option<Vec<u8>>,
    /// HPKE init public key нового устройства.
    #[serde(with = "serde_bytes")]
    pub device_init_public_key: Vec<u8>,
    /// Ed25519 signing public key нового устройства.
    #[serde(with = "serde_bytes")]
    pub device_signing_public_key: Vec<u8>,
    /// Подпись identity-ключом: Ed25519(identity_sk, device_signing_pk || device_init_pk || ts_le).
    #[serde(with = "serde_bytes")]
    pub device_authorization_signature: Vec<u8>,
    /// Timestamp в секундах (для проверки device_authorization_signature).
    pub device_authorization_timestamp: i64,
}

/// Response after redeeming an invite token.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RedeemResponse {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub role: String,
}

use uuid::Uuid;

use crate::error::AppError;

/// Разобранный заголовок `X-Auth-Signature`.
///
/// Формат: `<device_id_hex>:<unix_ts>:<nonce_hex>:<signature_hex>`
pub struct AuthHeader {
    pub device_id: Uuid,
    pub timestamp_secs: i64,
    pub nonce: Vec<u8>,
    pub signature: Vec<u8>,
}

impl AuthHeader {
    /// Парсит строковое значение заголовка `X-Auth-Signature`.
    ///
    /// # Errors
    ///
    /// Возвращает `AppError::BadRequest` если:
    /// - Не 4 части, разделённые `:`.
    /// - `device_id` не 32 hex символа (16 байт UUID).
    /// - `unix_ts` не целое число.
    /// - `nonce` меньше 16 байт после hex-decode.
    /// - `signature` не ровно 64 байта после hex-decode.
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let parts: Vec<&str> = raw.split(':').collect();
        if parts.len() != 4 {
            return Err(AppError::BadRequest(
                "invalid X-Auth-Signature format: expected 4 colon-separated parts".into(),
            ));
        }

        let device_id = Uuid::parse_str(parts[0]).map_err(|_| {
            AppError::BadRequest("invalid device_id in X-Auth-Signature".into())
        })?;

        let timestamp_secs: i64 = parts[1].parse().map_err(|_| {
            AppError::BadRequest("invalid timestamp in X-Auth-Signature".into())
        })?;

        let nonce = hex::decode(parts[2]).map_err(|_| {
            AppError::BadRequest("invalid nonce hex in X-Auth-Signature".into())
        })?;
        if nonce.len() < 16 {
            return Err(AppError::BadRequest(
                "nonce too short in X-Auth-Signature: must be >= 16 bytes".into(),
            ));
        }

        let signature = hex::decode(parts[3]).map_err(|_| {
            AppError::BadRequest("invalid signature hex in X-Auth-Signature".into())
        })?;
        if signature.len() != 64 {
            return Err(AppError::BadRequest(
                "invalid signature length in X-Auth-Signature: must be 64 bytes".into(),
            ));
        }

        Ok(Self {
            device_id,
            timestamp_secs,
            nonce,
            signature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_header() {
        // 32 hex chars = 16 bytes UUID
        let device_id = "550e8400e29b41d4a716446655440000";
        let ts = "1234567890";
        let nonce = "0123456789abcdef0123456789abcdef"; // 32 hex = 16 bytes
        let sig = "abcd".repeat(32); // 128 hex = 64 bytes

        let raw = format!("{device_id}:{ts}:{nonce}:{sig}");
        let parsed = AuthHeader::parse(&raw).unwrap();
        assert_eq!(
            parsed.device_id.to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(parsed.timestamp_secs, 1_234_567_890);
        assert_eq!(parsed.nonce.len(), 16);
        assert_eq!(parsed.signature.len(), 64);
    }

    #[test]
    fn test_parse_wrong_parts_count() {
        assert!(AuthHeader::parse("a:b:c").is_err());
        assert!(AuthHeader::parse("a:b").is_err());
        assert!(AuthHeader::parse("").is_err());
    }

    #[test]
    fn test_parse_invalid_uuid() {
        let raw = format!("not-a-uuid:1234:aaaa:{}", "ab".repeat(64));
        assert!(AuthHeader::parse(&raw).is_err());
    }

    #[test]
    fn test_parse_non_numeric_timestamp() {
        let raw = format!(
            "550e8400e29b41d4a716446655440000:not-a-number:aaaa:{}",
            "ab".repeat(64)
        );
        assert!(AuthHeader::parse(&raw).is_err());
    }

    #[test]
    fn test_parse_short_nonce() {
        let raw = format!(
            "550e8400e29b41d4a716446655440000:1234:{}:{}",
            "aa",      // 1 byte
            "ab".repeat(64)
        );
        assert!(AuthHeader::parse(&raw).is_err());
    }

    #[test]
    fn test_parse_wrong_signature_length() {
        let raw = format!(
            "550e8400e29b41d4a716446655440000:1234:0123456789abcdef0123456789abcdef:{}",
            "aa" // 1 byte
        );
        assert!(AuthHeader::parse(&raw).is_err());
    }
}

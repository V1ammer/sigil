//! Provisioning QR payload encode/decode.

use base64::Engine;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::CryptoError;

/// Data encoded into a QR code during device provisioning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QrPayload {
    /// Server URL.
    pub server_url: String,
    /// User ID.
    pub user_id: Uuid,
    /// Provisioning session ID.
    pub provisioning_id: Uuid,
    /// New device's temporary X25519 public key.
    pub new_device_temp_x25519_pub: [u8; 32],
    /// New device's temporary Ed25519 public key.
    pub new_device_temp_ed25519_pub: [u8; 32],
    /// Nonce for replay protection.
    pub nonce: Vec<u8>,
}

/// Encode a QR payload to a URL-safe base64 string.
///
/// # Errors
///
/// Returns `CryptoError::Encoding` on serialization failure.
pub fn encode_qr(payload: &QrPayload) -> Result<String, CryptoError> {
    let cbor = rmp_serde::to_vec_named(payload)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cbor))
}

/// Decode a QR payload from a URL-safe base64 string.
///
/// # Errors
///
/// Returns `CryptoError::Encoding` on decoding or deserialization failure.
pub fn decode_qr(s: &str) -> Result<QrPayload, CryptoError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| CryptoError::Encoding(e.to_string()))?;
    Ok(rmp_serde::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_payload_roundtrip() {
        let payload = QrPayload {
            server_url: "https://example.com".into(),
            user_id: Uuid::now_v7(),
            provisioning_id: Uuid::now_v7(),
            new_device_temp_x25519_pub: [1u8; 32],
            new_device_temp_ed25519_pub: [2u8; 32],
            nonce: vec![3, 4, 5],
        };
        let encoded = encode_qr(&payload).unwrap();
        let decoded = decode_qr(&encoded).unwrap();
        assert_eq!(payload, decoded);
    }
}

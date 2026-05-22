//! Reaction blind index via MLS exporter secret.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::CryptoError;

/// Compute a stable blind index for a reaction.
///
/// The index is derived from the MLS group exporter secret, making it
/// deterministic per `(group, message, emoji)` but unguessable to the server.
///
/// # Errors
///
/// Returns `CryptoError::Mls` if the exporter secret cannot be derived.
pub fn reaction_blind_index(
    exporter_secret: &[u8],
    message_id: uuid::Uuid,
    emoji: &str,
) -> Result<Vec<u8>, CryptoError> {
    let mut mac = HmacSha256::new_from_slice(exporter_secret)
        .map_err(|e| CryptoError::Mls(format!("HMAC init failed: {e}")))?;
    mac.update(message_id.as_bytes());
    mac.update(emoji.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

type HmacSha256 = Hmac<Sha256>;

//! Blind index computation for usernames.
//!
//! Uses HMAC-SHA256 with a server-provided key. The server never sees the
//! plaintext username.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::CryptoError;

/// Compute a blind index for a username.
///
/// # Errors
///
/// Returns `CryptoError::Mls` if HMAC initialization fails (should never happen
/// with a 32-byte key).
pub fn username_blind_index(username: &str, server_key: &[u8; 32]) -> Result<Vec<u8>, CryptoError> {
    let mut mac = HmacSha256::new_from_slice(server_key)
        .map_err(|e| CryptoError::Mls(format!("HMAC init failed: {e}")))?;
    mac.update(username.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

type HmacSha256 = Hmac<Sha256>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blind_index_deterministic() {
        let key = [42u8; 32];
        let idx1 = username_blind_index("alice", &key).unwrap();
        let idx2 = username_blind_index("alice", &key).unwrap();
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn test_blind_index_different_usernames() {
        let key = [42u8; 32];
        let idx1 = username_blind_index("alice", &key).unwrap();
        let idx2 = username_blind_index("bob", &key).unwrap();
        assert_ne!(idx1, idx2);
    }
}

//! Canonical signed message builder.
//!
//! Must byte-for-byte match the server version in `messenger-crypto::canonical`.

/// Builds the canonical message that the client signs.
///
/// Format (each part separated by literal `\n`):
/// ```text
/// method
/// path
/// timestamp_secs
/// nonce_hex
/// blake3(body_bytes)_hex
/// ```
///
/// # Panics
///
/// Does not panic under normal operation.
#[must_use]
pub fn build_signed_message(
    method: &str,
    path: &str,
    timestamp_secs: i64,
    nonce: &[u8],
    body: &[u8],
) -> Vec<u8> {
    let body_hash = blake3::hash(body);
    format!(
        "{method}\n{path}\n{timestamp_secs}\n{nonce_hex}\n{body_hash}",
        nonce_hex = hex::encode(nonce),
        body_hash = body_hash.to_hex()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_message_matches_server() {
        let m = build_signed_message("POST", "/v1/foo", 1_234_567_890, b"\x01\x02", b"body bytes");
        let expected = format!(
            "POST\n/v1/foo\n1234567890\n0102\n{}",
            blake3::hash(b"body bytes").to_hex()
        );
        assert_eq!(m, expected.into_bytes());
    }

    #[test]
    fn test_canonical_empty_body() {
        let m = build_signed_message("GET", "/v1/bar", 0, b"", b"");
        let expected = format!("GET\n/v1/bar\n0\n\n{}", blake3::hash(b"").to_hex());
        assert_eq!(m, expected.into_bytes());
    }
}

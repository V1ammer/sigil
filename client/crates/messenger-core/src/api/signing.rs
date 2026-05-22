//! Request signing helpers.

use blake3;

/// Build the canonical message to be signed for a request.
///
/// Format: `method\npath\ntimestamp\nnonce\nblake3(body)`
#[must_use]
pub fn build_signed_message(method: &str, path: &str, timestamp: i64, nonce: &[u8], body: &[u8]) -> Vec<u8> {
    let body_hash = blake3::hash(body);
    format!(
        "{}\n{}\n{}\n{}\n{}",
        method,
        path,
        timestamp,
        hex::encode(nonce),
        body_hash.to_hex()
    )
    .into_bytes()
}

/// Current Unix timestamp in seconds.
#[must_use]
pub fn now_secs() -> i64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .try_into()
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as i64
    }
}

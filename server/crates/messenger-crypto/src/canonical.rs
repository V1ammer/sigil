#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

/// Строит каноническое сообщение, которое клиент подписывает.
///
/// Формат (каждая часть разделена литералом `\n`):
/// ```text
/// method
/// path
/// timestamp_secs
/// nonce_hex
/// blake3(body_bytes)_hex
/// ```
///
/// И сервер, и клиент ДОЛЖНЫ использовать эту же функцию.
///
/// # Паника
///
/// Не паникует при нормальной работе.
///
/// # Panics
///
/// Не паникует.
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

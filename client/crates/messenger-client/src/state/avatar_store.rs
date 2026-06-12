//! Локальное хранение собственного аватара (data URL в localStorage) и
//! конвертация data URL ↔ сырые байты для шифрования/расшифровки блоба.

use uuid::Uuid;

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok()).flatten()
}

fn key(user_id: Uuid) -> String {
    format!("messenger_own_avatar_{user_id}")
}

/// Сохранённый data URL собственного аватара.
#[must_use]
pub fn load_own_avatar(user_id: Uuid) -> Option<String> {
    storage()?.get_item(&key(user_id)).ok().flatten()
}

pub fn save_own_avatar(user_id: Uuid, data_url: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(&key(user_id), data_url);
    }
}

pub fn clear_own_avatar(user_id: Uuid) {
    if let Some(s) = storage() {
        let _ = s.remove_item(&key(user_id));
    }
}

/// `data:image/jpeg;base64,...` → (mime, сырые байты).
#[must_use]
pub fn data_url_to_bytes(data_url: &str) -> Option<(String, Vec<u8>)> {
    let rest = data_url.strip_prefix("data:")?;
    let (meta, b64) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?.to_string();
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    Some((mime, bytes))
}

/// Сырые байты картинки → data URL.
#[must_use]
pub fn bytes_to_data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_url_roundtrip() {
        let bytes = vec![0xFFu8, 0xD8, 0xFF, 0x00, 0x42];
        let url = bytes_to_data_url("image/jpeg", &bytes);
        let (mime, decoded) = data_url_to_bytes(&url).expect("roundtrip");
        assert_eq!(mime, "image/jpeg");
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn rejects_non_data_url() {
        assert!(data_url_to_bytes("https://example.com/a.jpg").is_none());
        assert!(data_url_to_bytes("data:image/jpeg,plain").is_none());
    }
}

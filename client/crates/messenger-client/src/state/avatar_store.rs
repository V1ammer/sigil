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

// --- Учёт «в какие группы текущий аватар уже разослан» ---
//
// Доставка через одноразовые события (смена аватара, создание чата, join по
// welcome) ненадёжна: чат может появиться позже смены аватара, а join-хук не
// срабатывает при неудачном MLS-join. Поэтому клиент хранит отпечаток
// разосланного аватара per-group и периодически досылает недостающее.

const ANNOUNCED_KEY: &str = "messenger_avatar_announced";

/// Отпечаток текущего аватара пользователя ("none", если аватара нет).
#[must_use]
pub fn avatar_fingerprint(user_id: Uuid) -> String {
    match load_own_avatar(user_id) {
        Some(data_url) => blake3::hash(data_url.as_bytes()).to_hex()[..16].to_string(),
        None => "none".to_string(),
    }
}

/// Map group_id → отпечаток аватара, который туда уже разослан.
#[must_use]
pub fn announced_map() -> std::collections::HashMap<Uuid, String> {
    let Some(s) = storage() else { return std::collections::HashMap::new() };
    let Ok(Some(json)) = s.get_item(ANNOUNCED_KEY) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_str::<Vec<(String, String)>>(&json)
        .map(|entries| {
            entries
                .into_iter()
                .filter_map(|(id, fp)| id.parse::<Uuid>().ok().map(|id| (id, fp)))
                .collect()
        })
        .unwrap_or_default()
}

/// Запомнить, что в группу разослан аватар с данным отпечатком.
pub fn mark_announced(group_id: Uuid, fingerprint: &str) {
    let mut map = announced_map();
    map.insert(group_id, fingerprint.to_string());
    if let Some(s) = storage() {
        let entries: Vec<(String, String)> =
            map.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        if let Ok(json) = serde_json::to_string(&entries) {
            let _ = s.set_item(ANNOUNCED_KEY, &json);
        }
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

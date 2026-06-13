//! Локальное хранение профиля (display name, bio) по user_id.
//!
//! Сервер zero-knowledge о профиле, поэтому display name живёт только на
//! устройстве и доставляется собеседникам внутри MLS-конвертов
//! (`sender_display_name_override`) — тем же путём, что и аватар.

use uuid::Uuid;

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok()).flatten()
}

fn name_key(user_id: Uuid) -> String {
    format!("messenger_display_name_{user_id}")
}

fn bio_key(user_id: Uuid) -> String {
    format!("messenger_bio_{user_id}")
}

/// Сохранённое отображаемое имя (None — не задано, используется username).
#[must_use]
pub fn load_display_name(user_id: Uuid) -> Option<String> {
    storage()?
        .get_item(&name_key(user_id))
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

pub fn save_display_name(user_id: Uuid, name: &str) {
    if let Some(s) = storage() {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            let _ = s.remove_item(&name_key(user_id));
        } else {
            let _ = s.set_item(&name_key(user_id), trimmed);
        }
    }
}

#[must_use]
pub fn load_bio(user_id: Uuid) -> String {
    storage()
        .and_then(|s| s.get_item(&bio_key(user_id)).ok().flatten())
        .unwrap_or_default()
}

pub fn save_bio(user_id: Uuid, bio: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(&bio_key(user_id), bio);
    }
}

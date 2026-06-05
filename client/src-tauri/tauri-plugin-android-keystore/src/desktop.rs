//! Desktop fallback — no-op keystore.
// All keystore operations on desktop are handled by the OS keyring via messenger-storage.
// This module exists only to satisfy the plugin structure.

use tauri::Runtime;

pub struct KeystoreDesktop<R: Runtime>(pub crate::mobile::KeystoreMobile);

impl<R: Runtime> KeystoreDesktop<R> {
    pub fn new() -> Self {
        Self(crate::mobile::KeystoreMobile)
    }
}

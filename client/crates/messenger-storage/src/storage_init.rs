//! Bootstrap helper: initialise platform-specific storage.

use crate::{error::StorageError, traits::MessengerLocalStore};

/// Initialise storage for the given profile.
pub async fn init_storage(profile_name: &str) -> Result<Box<dyn MessengerLocalStore>, StorageError> {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    return crate::desktop::init(profile_name).await;

    #[cfg(target_os = "android")]
    return crate::android::init(profile_name).await;

    #[cfg(target_arch = "wasm32")]
    return crate::web::init(profile_name).await;
}

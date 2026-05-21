//! Cross-platform secure storage and local encrypted database.
//! - Native (desktop / Android): rusqlite+sqlcipher, OS keystore via `keyring` or JNI.
//! - Web: `IndexedDB`, `WebCrypto` non-extractable keys.

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

#[cfg(feature = "native")]
pub mod native {}

#[cfg(feature = "web")]
pub mod web {}

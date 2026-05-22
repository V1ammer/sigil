//! Cross-platform secure storage and local encrypted database.
//!
//! - Native (desktop): OS keyring + SQLCipher via `rusqlite`.
//! - Native (Android): Android Keystore via JNI (skeleton, see C12).
//! - Web: WebCrypto non-extractable keys + IndexedDB.

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

pub mod error;
pub mod traits;
pub mod types;
pub mod migrations;
pub mod storage_init;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod desktop;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_arch = "wasm32")]
pub mod web;

pub use error::*;
pub use traits::*;
pub use types::*;
pub use storage_init::*;

//! Cross-platform client core: crypto, identity, MLS, and business logic.
//! Compiles for native (desktop / android) and wasm (web).

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

pub mod api;
pub mod canonical;
pub mod ed25519;
pub mod error;
pub mod identity;

#[cfg(feature = "native")]
pub mod age_wrap;
#[cfg(feature = "native")]
pub mod blind_index;
#[cfg(feature = "native")]
pub mod bootstrap;
#[cfg(feature = "native")]
pub mod mls;
#[cfg(feature = "native")]
pub mod prov;

#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

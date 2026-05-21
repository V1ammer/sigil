//! Cross-platform client core: API client, crypto, business logic.
//! Compiles for native (desktop / android) and wasm (web).

#![warn(clippy::all, clippy::pedantic)]
#![forbid(unsafe_code)]

#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

//! MLS ciphersuite constant.

use openmls::prelude::Ciphersuite;

/// Fixed ciphersuite for this messenger.
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

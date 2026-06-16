//! MLS credential generation from client identity.

use openmls::prelude::{BasicCredential, CredentialWithKey, SignaturePublicKey};

use crate::identity::ClientIdentity;

/// Build an MLS `CredentialWithKey` for this device.
///
/// The credential identity is the user id (shared across the user's devices),
/// but the MLS leaf signature key MUST be unique per leaf — so it's the
/// per-device signing key. Using the shared identity key made two of a user's
/// devices in one group collide with `DuplicateSignatureKey`. The signer
/// (`IdentitySigner`) signs with this same per-device key.
pub fn build_credential(identity: &ClientIdentity) -> CredentialWithKey {
    let credential = BasicCredential::new(identity.user_id.as_bytes().to_vec()).into();
    let signature_key: SignaturePublicKey = identity.device_signing_key.public_bytes().as_slice().into();
    CredentialWithKey {
        credential,
        signature_key,
    }
}

/// Build an MLS `CredentialWithKey` from raw user ID and public key bytes.
pub fn build_credential_from_parts(user_id: uuid::Uuid, public_key: &[u8; 32]) -> CredentialWithKey {
    let credential = BasicCredential::new(user_id.as_bytes().to_vec()).into();
    let signature_key: SignaturePublicKey = public_key.as_slice().into();
    CredentialWithKey {
        credential,
        signature_key,
    }
}

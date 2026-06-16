//! MLS credential generation from client identity.

use openmls::prelude::{BasicCredential, CredentialWithKey, SignaturePublicKey};

use crate::identity::ClientIdentity;

/// Build an MLS `CredentialWithKey` from the client's identity signing key.
///
/// NOTE: the signature key is the user's identity key, shared across the user's
/// devices. So at most ONE device per user may be in a group — adding two
/// collides with `DuplicateSignatureKey`. Callers add a single device per user.
/// (Per-device signature keys require provider-stored SignatureKeyPairs and are
/// a separate change; switching `IdentitySigner` to the device key alone breaks
/// leaf-signature verification on join.)
pub fn build_credential(identity: &ClientIdentity) -> CredentialWithKey {
    let credential = BasicCredential::new(identity.user_id.as_bytes().to_vec()).into();
    let signature_key: SignaturePublicKey = identity.identity_signing_key.public_bytes().as_slice().into();
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

//! MLS credential generation from client identity.

use openmls::prelude::{BasicCredential, CredentialWithKey, SignaturePublicKey};
use openmls_traits::signatures::{Signer as OmlsSigner, SignerError};
use openmls_traits::types::SignatureScheme;

use crate::identity::ClientIdentity;

/// Build an MLS `CredentialWithKey` for this device.
///
/// The credential *identity* is the user id (shared across the user's devices),
/// but the leaf *signature key* is the per-device signing key. MLS requires a
/// unique signature key per leaf, so using a per-device key lets two of a user's
/// devices coexist in one group (no `DuplicateSignatureKey`) — enabling real
/// multi-device. The signature key here MUST match the key [`DeviceSigner`]
/// signs with, on EVERY path (keypackage build AND group ops); a mismatch makes
/// leaf-signature validation fail with `InvalidNodeSignature` on join.
pub fn build_credential(identity: &ClientIdentity) -> CredentialWithKey {
    let credential = BasicCredential::new(identity.user_id.as_bytes().to_vec()).into();
    let signature_key: SignaturePublicKey = identity.device_signing_key.public_bytes().as_slice().into();
    CredentialWithKey {
        credential,
        signature_key,
    }
}

/// The single MLS signer for a device — signs with the per-device signing key.
///
/// Both keypackage generation and every group operation MUST use this signer so
/// the leaf signature always matches the per-device key in [`build_credential`].
pub struct DeviceSigner<'a>(pub &'a ClientIdentity);

impl OmlsSigner for DeviceSigner<'_> {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        Ok(self.0.device_signing_key.sign(payload).to_vec())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
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

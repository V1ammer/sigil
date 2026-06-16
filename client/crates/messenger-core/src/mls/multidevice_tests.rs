//! Multi-device MLS tests: two leaves of the SAME user (two devices) plus a
//! second user in one group. Guards the per-device-signature-key design — a
//! shared signature key would collide with `DuplicateSignatureKey`, and a
//! credential/signer mismatch would fail `new_from_welcome` with
//! `InvalidNodeSignature`.

use openmls::prelude::{
    GroupId, KeyPackage, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn,
    MlsMessageIn, ProcessedMessageContent, StagedWelcome,
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use tls_codec::{Deserialize as _, Serialize as _};
use uuid::Uuid;

use crate::identity::ClientIdentity;

use super::ciphersuite::CIPHERSUITE;
use super::credentials::{build_credential, DeviceSigner};
use super::keypackage::generate_keypackage;

/// Deserialize a freshly-serialized welcome blob into an openmls `Welcome`.
fn extract_welcome(bytes: &[u8]) -> openmls::prelude::Welcome {
    let msg_in = MlsMessageIn::tls_deserialize_exact(bytes).expect("welcome deserialize");
    match msg_in.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        other => panic!("expected welcome, got {:?}", std::mem::discriminant(&other)),
    }
}

fn kp_from(provider: &OpenMlsRustCrypto, id: &ClientIdentity) -> KeyPackage {
    let generated = generate_keypackage(provider, id, 86_400, false).expect("keypackage");
    rmp_serde::from_slice(&generated.key_package_bytes).expect("kp decode")
}

#[test]
fn two_devices_of_one_user_plus_peer() {
    // User A on two devices: same user_id + identity key, distinct device keys.
    let user_a = Uuid::now_v7();
    let a1 = ClientIdentity::generate_new_user(user_a, "alice".into(), Uuid::now_v7());
    let a_seed = a1.identity_signing_key.secret_bytes();
    let a2 = ClientIdentity::generate_new_device(user_a, "alice".into(), Uuid::now_v7(), a_seed);
    // A second, distinct user.
    let b1 = ClientIdentity::generate_new_user(Uuid::now_v7(), "bob".into(), Uuid::now_v7());

    // Sanity: the two devices share the identity key but have DISTINCT device
    // (= MLS leaf) signature keys — the whole point of per-device leaves.
    assert_eq!(
        a1.identity_signing_key.public_bytes(),
        a2.identity_signing_key.public_bytes()
    );
    assert_ne!(
        a1.device_signing_key.public_bytes(),
        a2.device_signing_key.public_bytes()
    );

    // Each device/user builds its keypackage in its OWN provider (which holds
    // the matching private init key needed to later join).
    let p_a1 = OpenMlsRustCrypto::default();
    let p_a2 = OpenMlsRustCrypto::default();
    let p_b1 = OpenMlsRustCrypto::default();
    let kp_a2 = kp_from(&p_a2, &a2);
    let kp_b1 = kp_from(&p_b1, &b1);

    // Device 1 creates the group.
    let signer_a1 = DeviceSigner(&a1);
    let create_cfg = MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .use_ratchet_tree_extension(true)
        .build();
    let gid = GroupId::from_slice(Uuid::now_v7().as_bytes());
    let mut group_a1 =
        MlsGroup::new_with_group_id(&p_a1, &signer_a1, &create_cfg, gid, build_credential(&a1))
            .expect("create group");

    // Add device 2 (same user) AND user B in one commit. Two same-user leaves
    // here is exactly what used to fail with DuplicateSignatureKey.
    let (_commit, welcome_out, _gi) = group_a1
        .add_members(&p_a1, &signer_a1, &[kp_a2, kp_b1])
        .expect("add_members must not DuplicateSignatureKey");
    group_a1.merge_pending_commit(&p_a1).expect("merge commit");

    let welcome_bytes = welcome_out.tls_serialize_detached().expect("welcome serialize");

    // Both joiners process the welcome — used to fail with InvalidNodeSignature
    // when the credential key and the signing key disagreed.
    let join_cfg = MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .build();
    let mut group_a2 =
        StagedWelcome::new_from_welcome(&p_a2, &join_cfg, extract_welcome(&welcome_bytes), None)
            .expect("a2 new_from_welcome")
            .into_group(&p_a2)
            .expect("a2 into_group");
    let mut group_b1 =
        StagedWelcome::new_from_welcome(&p_b1, &join_cfg, extract_welcome(&welcome_bytes), None)
            .expect("b1 new_from_welcome")
            .into_group(&p_b1)
            .expect("b1 into_group");

    // Group has three leaves (a1, a2, b1).
    assert_eq!(group_a1.members().count(), 3);

    // Device 1 sends an application message; device 2 (same user) AND user B
    // both decrypt it.
    let payload = b"multi-device hello";
    let msg = group_a1
        .create_message(&p_a1, &signer_a1, payload)
        .expect("create_message");
    let msg_bytes = msg.tls_serialize_detached().expect("msg serialize");

    for (label, provider, group) in [
        ("a2", &p_a2, &mut group_a2),
        ("b1", &p_b1, &mut group_b1),
    ] {
        let protocol = MlsMessageIn::tls_deserialize_exact(&msg_bytes)
            .expect("msg deserialize")
            .try_into_protocol_message()
            .expect("protocol message");
        let processed = group
            .process_message(provider, protocol)
            .unwrap_or_else(|e| panic!("{label} process_message: {e:?}"));
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                assert_eq!(app.into_bytes(), payload, "{label} plaintext mismatch");
            }
            other => panic!("{label} expected application message, got {other:?}"),
        }
    }
}

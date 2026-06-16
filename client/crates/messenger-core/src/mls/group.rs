//! MLS group runtime — snapshot-per-operation strategy.
//!
//! Each operation:
//! 1. Load `MemoryStorage` blob from `MessengerLocalStore`.
//! 2. Create `OpenMlsRustCrypto` backed by that storage.
//! 3. Load `MlsGroup` from storage via `MlsGroup::load`.
//! 4. Perform the MLS operation.
//! 5. Serialize storage back to blob and save.

use std::collections::HashMap;
use std::sync::Arc;

use openmls::prelude::{
    GroupId, KeyPackage, KeyPackageBundle, LeafNodeIndex, LeafNodeParameters, MlsGroup,
    MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn, MlsMessageIn,
    ProcessedMessageContent, SenderRatchetConfiguration, StagedWelcome,
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::storage::StorageProvider;
use openmls_traits::{signatures::Signer as OmlsSigner, types::SignatureScheme, OpenMlsProvider};
use tls_codec::{Deserialize, Serialize as TlsSerializeTrait};
use uuid::Uuid;

use crate::error::CryptoError;
use crate::identity::ClientIdentity;

use super::ciphersuite::CIPHERSUITE;
use super::credentials::build_credential;

/// Output of creating a new MLS group.
#[derive(Debug)]
pub struct CreateGroupOutput {
    /// The initial commit message (to broadcast).
    pub initial_commit: Vec<u8>,
    /// Welcome messages for each added member.
    pub welcomes: Vec<Vec<u8>>,
    /// Serialized group state for local storage.
    pub group_state: Vec<u8>,
}

/// Output of joining a group via welcome.
#[derive(Debug)]
pub struct JoinGroupOutput {
    /// The joined group ID.
    pub group_id: Uuid,
    /// Serialized group state for local storage.
    pub group_state: Vec<u8>,
}

/// A decrypted application message.
#[derive(Debug)]
pub struct DecryptedMessage {
    /// Plaintext payload.
    pub plaintext: Vec<u8>,
    /// Sender leaf index.
    pub sender_index: u32,
}

/// A pending commit that has not yet been merged.
#[derive(Debug)]
pub struct PendingCommit {
    /// Commit message bytes.
    pub commit: Vec<u8>,
    /// Welcome messages for new members.
    pub welcomes: Vec<Vec<u8>>,
    /// Optional group info.
    pub group_info: Option<Vec<u8>>,
    /// Current epoch at the time the commit was created (pre-commit).
    pub epoch: u64,
}

/// MLS runtime using snapshot-per-operation persistence.
pub struct MlsRuntime {
    local: Arc<dyn messenger_storage::traits::MessengerLocalStore>,
    device_id: Uuid,
}

/// Fixed UUID for storing device-level openmls key material (KeyPackages, etc.).
const DEVICE_STATE_KEY: Uuid = Uuid::from_u128(0);

impl MlsRuntime {
    /// Create a new runtime backed by the given local store and device ID.
    #[must_use]
    pub fn new(local: Arc<dyn messenger_storage::traits::MessengerLocalStore>, device_id: Uuid) -> Self {
        Self { local, device_id }
    }

    /// Generate a KeyPackage and persist the device key material.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on openmls failure.
    pub async fn generate_keypackage(
        &self,
        identity: &ClientIdentity,
        lifetime_secs: u64,
        is_last_resort: bool,
    ) -> Result<super::keypackage::GeneratedKeyPackage, CryptoError> {
        let provider = self.load_device_provider().await?;
        let kp = super::keypackage::generate_keypackage(&provider, identity, lifetime_secs, is_last_resort)?;
        self.save_device_provider(&provider).await?;
        Ok(kp)
    }

    /// Load device-level provider state into a fresh `OpenMlsRustCrypto`.
    async fn load_device_provider(&self) -> Result<OpenMlsRustCrypto, CryptoError> {
        let provider = OpenMlsRustCrypto::default();
        if let Some(blob) = self.local.load_mls_group_state(self.device_id, DEVICE_STATE_KEY).await? {
            let map: HashMap<Vec<u8>, Vec<u8>> =
                rmp_serde::from_slice(&blob).map_err(|e| CryptoError::Serialization(e.to_string()))?;
            {
                let mut store = provider.storage().values.write().unwrap();
                store.clear();
                store.extend(map);
            }
        }
        Ok(provider)
    }

    /// Save device-level provider state back to local storage.
    async fn save_device_provider(&self, provider: &OpenMlsRustCrypto) -> Result<(), CryptoError> {
        let blob = serialize_provider_storage(provider)?;
        self.local.save_mls_group_state(self.device_id, DEVICE_STATE_KEY, &blob).await?;
        Ok(())
    }

    /// Create a new MLS group and add initial members in a single commit.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on openmls failure.
    pub async fn create_group(
        &self,
        creator: &ClientIdentity,
        group_id: Uuid,
        initial_members_keypackages: &[Vec<u8>],
    ) -> Result<CreateGroupOutput, CryptoError> {
        let provider = OpenMlsRustCrypto::default();
        let signer = IdentitySigner(creator);
        let credential = build_credential(creator);

        let sender_ratchet = SenderRatchetConfiguration::new(1_000, 1_000);
        let mls_group_config = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .sender_ratchet_configuration(sender_ratchet.clone())
            .build();

        let gid = GroupId::from_slice(group_id.as_bytes());
        let mut group = MlsGroup::new_with_group_id(
            &provider,
            &signer,
            &mls_group_config,
            gid,
            credential,
        )
        .map_err(|e| CryptoError::Mls(format!("new_with_group_id: {e:?}")))?;

        let mut welcomes = Vec::new();
        let mut commit_bytes = Vec::new();

        if !initial_members_keypackages.is_empty() {
            let mut kps = Vec::with_capacity(initial_members_keypackages.len());
            for kp_bytes in initial_members_keypackages {
                let kp: KeyPackage = rmp_serde::from_slice(kp_bytes)
                    .map_err(|e| CryptoError::Serialization(e.to_string()))?;
                kps.push(kp);
            }

            let (commit_msg, welcome_msg, _group_info) = group
                .add_members(&provider, &signer, &kps)
                .map_err(|e| CryptoError::Mls(format!("add_members: {e:?}")))?;

            commit_bytes = commit_msg
                .tls_serialize_detached()
                .map_err(|e| CryptoError::Mls(format!("serialize commit: {e:?}")))?;
            let welcome_bytes = welcome_msg
                .tls_serialize_detached()
                .map_err(|e| CryptoError::Mls(format!("serialize welcome: {e:?}")))?;
            welcomes.push(welcome_bytes);

            group
                .merge_pending_commit(&provider)
                .map_err(|e| CryptoError::Mls(format!("merge_pending_commit: {e:?}")))?;
        }

        let state_blob = serialize_provider_storage(&provider)?;
        self.local
            .save_mls_group_state(self.device_id, group_id, &state_blob)
            .await?;

        Ok(CreateGroupOutput {
            initial_commit: commit_bytes,
            welcomes,
            group_state: state_blob,
        })
    }

    /// Join an existing group via a Welcome message.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on openmls failure.
    pub async fn join_via_welcome(
        &self,
        _identity: &ClientIdentity,
        welcome_bytes: &[u8],
    ) -> Result<JoinGroupOutput, CryptoError> {
        let provider = self.load_device_provider().await?;

        let msg_in = MlsMessageIn::tls_deserialize_exact(welcome_bytes)
            .map_err(|e| CryptoError::Mls(format!("deserialize welcome message: {e:?}")))?;
        let welcome = match msg_in.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            other => return Err(CryptoError::Mls(format!(
                "expected welcome message, got {:?}",
                std::mem::discriminant(&other)
            ))),
        };

        let sender_ratchet = SenderRatchetConfiguration::new(1_000, 1_000);
        let join_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .sender_ratchet_configuration(sender_ratchet)
            .build();
        let staged = StagedWelcome::new_from_welcome(&provider, &join_config, welcome, None)
            .map_err(|e| CryptoError::Mls(format!("new_from_welcome: {e:?}")))?;
        let group = staged.into_group(&provider)
            .map_err(|e| CryptoError::Mls(format!("into_group: {e:?}")))?;

        let group_id = Uuid::from_slice(group.group_id().as_slice())
            .map_err(|e| CryptoError::Mls(format!("invalid group id: {e}")))?;

        // Save both device state (KeyPackages consumed) and group state.
        self.save_device_provider(&provider).await?;
        let state_blob = serialize_provider_storage(&provider)?;
        self.local
            .save_mls_group_state(self.device_id, group_id, &state_blob)
            .await?;

        Ok(JoinGroupOutput {
            group_id,
            group_state: state_blob,
        })
    }

    /// Store a deserialized KeyPackageBundle in the device provider storage.
    ///
    /// This is used by a new device to persist an MLS KeyPackage (with its
    /// HPKE init private key) that was generated by the approving device and
    /// transferred via the bootstrap blob.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on deserialization or storage failure.
    pub async fn store_keypackage_bundle(
        &self,
        bundle_bytes: &[u8],
    ) -> Result<(), CryptoError> {
        let provider = self.load_device_provider().await?;

        let bundle: KeyPackageBundle = rmp_serde::from_slice(bundle_bytes)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;

        let hash_ref = bundle.key_package().hash_ref(provider.crypto())
            .map_err(|e| CryptoError::Mls(format!("hash_ref: {e:?}")))?;

        provider.storage().write_key_package(&hash_ref, &bundle)
            .map_err(|e| CryptoError::Mls(format!("write_key_package: {e:?}")))?;

        self.save_device_provider(&provider).await?;
        Ok(())
    }

    /// Encrypt an application message for the given group.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` if the group is not found or encryption fails.
    pub async fn encrypt_application_message(
        &self,
        group_id: Uuid,
        identity: &ClientIdentity,
        plaintext_payload: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;
        let _epoch = group.epoch().as_u64(); // current epoch
        let signer = IdentitySigner(identity);

        let msg_out = group
            .create_message(&provider, &signer, plaintext_payload)
            .map_err(|e| CryptoError::Mls(format!("create_message: {e:?}")))?;

        let wire = msg_out
            .tls_serialize_detached()
            .map_err(|e| CryptoError::Mls(format!("serialize: {e:?}")))?;

        self.save_group(group_id, &provider).await?;
        Ok(wire)
    }

    /// Decrypt an application message for the given group.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` if the group is not found or decryption fails.
    pub async fn decrypt_application_message(
        &self,
        group_id: Uuid,
        ciphertext: &[u8],
    ) -> Result<DecryptedMessage, CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;

        let msg_in = MlsMessageIn::tls_deserialize_exact(ciphertext)
            .map_err(|e| CryptoError::Mls(format!("deserialize: {e:?}")))?;
        let protocol_msg = msg_in
            .try_into_protocol_message()
            .map_err(|e| CryptoError::Mls(format!("protocol conversion: {e:?}")))?;

        let processed = group
            .process_message(&provider, protocol_msg)
            .map_err(|e| CryptoError::Mls(format!("process_message: {e:?}")))?;

        let content = processed.into_content();
        match content {
            ProcessedMessageContent::ApplicationMessage(app_msg) => {
                let plaintext = app_msg.into_bytes();
                // Epoch doesn't change on app message, but prng state may have changed.
                self.save_group(group_id, &provider).await?;
                Ok(DecryptedMessage {
                    plaintext,
                    sender_index: 0, // We don't track sender index in this MVP
                })
            }
            other => Err(CryptoError::Mls(format!(
                "expected application message, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Process a commit message from another member.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on failure.
    pub async fn process_commit(
        &self,
        group_id: Uuid,
        commit_bytes: &[u8],
    ) -> Result<(), CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;

        let msg_in = MlsMessageIn::tls_deserialize_exact(commit_bytes)
            .map_err(|e| CryptoError::Mls(format!("deserialize: {e:?}")))?;
        let protocol_msg = msg_in
            .try_into_protocol_message()
            .map_err(|e| CryptoError::Mls(format!("protocol conversion: {e:?}")))?;

        let processed = group
            .process_message(&provider, protocol_msg)
            .map_err(|e| CryptoError::Mls(format!("process_message: {e:?}")))?;

        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                group
                    .merge_staged_commit(&provider, *staged_commit)
                    .map_err(|e| CryptoError::Mls(format!("merge_staged_commit: {e:?}")))?;
            }
            other => {
                return Err(CryptoError::Mls(format!(
                    "expected staged commit, got {:?}",
                    std::mem::discriminant(&other)
                )));
            }
        }

        self.save_group(group_id, &provider).await?;
        Ok(())
    }

    /// Propose adding members to the group (without merging).
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on failure.
    pub async fn propose_add(
        &self,
        group_id: Uuid,
        identity: &ClientIdentity,
        keypackages: &[Vec<u8>],
    ) -> Result<PendingCommit, CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;
        let epoch = group.epoch().as_u64(); // current epoch before add
        let signer = IdentitySigner(identity);

        let mut kps = Vec::with_capacity(keypackages.len());
        for kp_bytes in keypackages {
            let kp: KeyPackage = rmp_serde::from_slice(kp_bytes)
                .map_err(|e| CryptoError::Serialization(e.to_string()))?;
            kps.push(kp);
        }

        let (commit_msg, welcome_msg, group_info) = group
            .add_members(&provider, &signer, &kps)
            .map_err(|e| CryptoError::Mls(format!("add_members: {e:?}")))?;

        let commit = commit_msg
            .tls_serialize_detached()
            .map_err(|e| CryptoError::Mls(format!("serialize commit: {e:?}")))?;
        let welcome = welcome_msg
            .tls_serialize_detached()
            .map_err(|e| CryptoError::Mls(format!("serialize welcome: {e:?}")))?;
        let group_info = group_info.map(|gi| {
            gi.tls_serialize_detached()
                .map_err(|e| CryptoError::Mls(format!("serialize group_info: {e:?}")))
        }).transpose()?;

        // Don't merge here — caller will merge after distributing the commit.
        self.save_group(group_id, &provider).await?;

        Ok(PendingCommit {
            commit,
            welcomes: vec![welcome],
            group_info,
            epoch,
        })
    }

    /// Propose removing members from the group (without merging).
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on failure.
    pub async fn propose_remove(
        &self,
        group_id: Uuid,
        identity: &ClientIdentity,
        leaf_indices: &[u32],
    ) -> Result<PendingCommit, CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;
        let epoch = group.epoch().as_u64(); // current epoch before removal
        let signer = IdentitySigner(identity);

        let indices: Vec<LeafNodeIndex> = leaf_indices
            .iter()
            .map(|&i| LeafNodeIndex::new(i))
            .collect();

        let (commit_msg, welcome_msg, group_info) = group
            .remove_members(&provider, &signer, &indices)
            .map_err(|e| CryptoError::Mls(format!("remove_members: {e:?}")))?;

        let commit = commit_msg
            .tls_serialize_detached()
            .map_err(|e| CryptoError::Mls(format!("serialize commit: {e:?}")))?;
        let welcome = welcome_msg
            .tls_serialize_detached()
            .map_err(|e| CryptoError::Mls(format!("serialize welcome: {e:?}")))?;
        let group_info = group_info.map(|gi| {
            gi.tls_serialize_detached()
                .map_err(|e| CryptoError::Mls(format!("serialize group_info: {e:?}")))
        }).transpose()?;

        self.save_group(group_id, &provider).await?;

        Ok(PendingCommit {
            commit,
            welcomes: if welcome.is_empty() { Vec::new() } else { vec![welcome] },
            group_info,
            epoch,
        })
    }

    /// Merge a pending commit that was previously proposed.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on failure.
    pub async fn merge_pending(&self, group_id: Uuid) -> Result<(), CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;

        group
            .merge_pending_commit(&provider)
            .map_err(|e| CryptoError::Mls(format!("merge_pending_commit: {e:?}")))?;

        self.save_group(group_id, &provider).await?;
        Ok(())
    }

    /// Rotate our own leaf key (MLS self-update) without changing membership.
    ///
    /// Advances the epoch and gives post-compromise security: a passively
    /// compromised old key can't decrypt messages after this commit is merged.
    /// Returns the commit to distribute (no welcomes). Don't merge until the
    /// server accepts it — call [`MlsRuntime::merge_pending`] on success or
    /// [`MlsRuntime::clear_pending_commit`] on rejection.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on failure.
    pub async fn self_update(
        &self,
        group_id: Uuid,
        identity: &ClientIdentity,
    ) -> Result<PendingCommit, CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;
        let epoch = group.epoch().as_u64();
        let signer = IdentitySigner(identity);

        let bundle = group
            .self_update(&provider, &signer, LeafNodeParameters::default())
            .map_err(|e| CryptoError::Mls(format!("self_update: {e:?}")))?;
        let commit = bundle
            .commit()
            .tls_serialize_detached()
            .map_err(|e| CryptoError::Mls(format!("serialize commit: {e:?}")))?;

        self.save_group(group_id, &provider).await?;
        Ok(PendingCommit {
            commit,
            welcomes: Vec::new(),
            group_info: None,
            epoch,
        })
    }

    /// Discard a staged (proposed-but-not-merged) commit, e.g. after the server
    /// rejected it on an epoch conflict, so the local group returns to the last
    /// merged epoch and can re-sync.
    ///
    /// # Errors
    ///
    /// Returns `CryptoError::Mls` on failure.
    pub async fn clear_pending_commit(&self, group_id: Uuid) -> Result<(), CryptoError> {
        let (mut group, provider) = self.load_group(group_id).await?;
        group
            .clear_pending_commit(provider.storage())
            .map_err(|e| CryptoError::Mls(format!("clear_pending_commit: {e:?}")))?;
        self.save_group(group_id, &provider).await?;
        Ok(())
    }

    /// List `(leaf_index, user_id)` for every current member leaf.
    ///
    /// The MLS leaf credential carries the member's user id — a user with
    /// several devices occupies several leaves, all with the same id. Used to
    /// map a target user to the real tree leaves for removal (the server-side
    /// `leaf_index` is only an advisory hint and must not be trusted for this).
    ///
    /// # Errors
    ///
    /// Returns `CryptoError` if the group can't be loaded.
    pub async fn member_leaves(&self, group_id: Uuid) -> Result<Vec<(u32, Uuid)>, CryptoError> {
        use openmls::prelude::BasicCredential;
        let (group, _provider) = self.load_group(group_id).await?;
        let mut out = Vec::new();
        for member in group.members() {
            let leaf = member.index.u32();
            if let Some(uid) = BasicCredential::try_from(member.credential)
                .ok()
                .and_then(|bc| <[u8; 16]>::try_from(bc.identity()).ok())
                .map(Uuid::from_bytes)
            {
                out.push((leaf, uid));
            }
        }
        Ok(out)
    }

    /// Load a group from local storage, returning the group and its provider.
    async fn load_group(
        &self,
        group_id: Uuid,
    ) -> Result<(MlsGroup, OpenMlsRustCrypto), CryptoError> {
        let provider = OpenMlsRustCrypto::default();

        if let Some(blob) = self.local.load_mls_group_state(self.device_id, group_id).await? {
            let map: HashMap<Vec<u8>, Vec<u8>> =
                rmp_serde::from_slice(&blob).map_err(|e| CryptoError::Serialization(e.to_string()))?;
            {
                let mut store = provider.storage().values.write().unwrap();
                store.clear();
                store.extend(map);
            }
        }

        let gid = GroupId::from_slice(group_id.as_bytes());
        let group = MlsGroup::load(provider.storage(), &gid)
            .map_err(|e| CryptoError::Mls(format!("load group: {e:?}")))?
            .ok_or_else(|| CryptoError::InvalidState(format!("group {group_id} not found")))?;

        Ok((group, provider))
    }

    /// Save the provider's storage back to local storage.
    async fn save_group(
        &self,
        group_id: Uuid,
        provider: &OpenMlsRustCrypto,
    ) -> Result<(), CryptoError> {
        let blob = serialize_provider_storage(provider)?;
        self.local
            .save_mls_group_state(self.device_id, group_id, &blob)
            .await?;
        Ok(())
    }
}

/// Serialize the provider's in-memory storage to a msgpack blob.
fn serialize_provider_storage(provider: &OpenMlsRustCrypto) -> Result<Vec<u8>, CryptoError> {
    let values = provider.storage().values.read().unwrap();
    rmp_serde::to_vec_named(&*values).map_err(|e| CryptoError::Serialization(e.to_string()))
}

/// Wrapper to implement openmls `Signer` trait for `ClientIdentity`.
struct IdentitySigner<'a>(&'a ClientIdentity);

impl OmlsSigner for IdentitySigner<'_> {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, openmls_traits::signatures::SignerError> {
        Ok(self.0.identity_signing_key.sign(payload).to_vec())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::*;
    use crate::identity::ClientIdentity;
    use crate::mls::application::{AppMessageBody, AppMessageKind, ApplicationEnvelope};
    use crate::mls::keypackage::generate_keypackage;
    use messenger_storage::traits::MessengerLocalStore;
    use messenger_storage::types::*;

    struct MockStore {
        mls: Mutex<HashMap<(Uuid, Uuid), Vec<u8>>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                mls: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait(?Send)]
    impl MessengerLocalStore for MockStore {
        async fn save_identity(
            &self,
            _user_id: Uuid,
            _identity: &EncryptedIdentity,
        ) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn load_identity(
            &self,
            _user_id: Uuid,
        ) -> Result<Option<EncryptedIdentity>, messenger_storage::error::StorageError> {
            Ok(None)
        }
        async fn save_mls_group_state(
            &self,
            device_id: Uuid,
            group_id: Uuid,
            state: &[u8],
        ) -> Result<(), messenger_storage::error::StorageError> {
            self.mls.lock().unwrap().insert((device_id, group_id), state.to_vec());
            Ok(())
        }
        async fn load_mls_group_state(
            &self,
            device_id: Uuid,
            group_id: Uuid,
        ) -> Result<Option<Vec<u8>>, messenger_storage::error::StorageError> {
            Ok(self.mls.lock().unwrap().get(&(device_id, group_id)).cloned())
        }
        async fn list_mls_group_ids(&self, device_id: Uuid) -> Result<Vec<Uuid>, messenger_storage::error::StorageError> {
            Ok(self.mls.lock().unwrap().keys().filter(|(d, _)| *d == device_id).map(|(_, g)| *g).collect())
        }
        async fn save_chat_meta(&self, _chat: &ChatMeta) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn list_chats(&self) -> Result<Vec<ChatMeta>, messenger_storage::error::StorageError> {
            Ok(Vec::new())
        }
        async fn save_message(&self, _msg: &CachedMessage) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn list_messages(
            &self,
            _group_id: Uuid,
            _limit: usize,
            _before_id: Option<Uuid>,
        ) -> Result<Vec<CachedMessage>, messenger_storage::error::StorageError> {
            Ok(Vec::new())
        }
        async fn mark_message_state(
            &self,
            _message_id: Uuid,
            _edited_at: Option<i64>,
            _deleted_at: Option<i64>,
        ) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn save_keypackage_local(
            &self,
            _kp: &LocalKeyPackage,
        ) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn list_local_keypackages(
            &self,
        ) -> Result<Vec<LocalKeyPackage>, messenger_storage::error::StorageError> {
            Ok(Vec::new())
        }
        async fn delete_local_keypackage(
            &self,
            _id: Uuid,
        ) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn get_setting(
            &self,
            _key: &str,
        ) -> Result<Option<String>, messenger_storage::error::StorageError> {
            Ok(None)
        }
        async fn set_setting(
            &self,
            _key: &str,
            _value: &str,
        ) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn save_attachment_meta(
            &self,
            _att: &AttachmentMeta,
        ) -> Result<(), messenger_storage::error::StorageError> {
            Ok(())
        }
        async fn load_attachment_meta(
            &self,
            _attachment_id: Uuid,
        ) -> Result<Option<AttachmentMeta>, messenger_storage::error::StorageError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_mls_create_group_two_members() {
        let store = Arc::new(MockStore::new());

        let alice = ClientIdentity::generate_new_user(Uuid::now_v7(), "alice".into(), Uuid::now_v7());
        let bob = ClientIdentity::generate_new_user(Uuid::now_v7(), "bob".into(), Uuid::now_v7());

        let alice_rt = MlsRuntime::new(store.clone(), alice.device_id);
        let bob_rt = MlsRuntime::new(store.clone(), bob.device_id);

        let bob_kp = bob_rt.generate_keypackage(&bob, 86_400, false).await.unwrap();

        let group_id = Uuid::now_v7();
        let out = alice_rt
            .create_group(&alice, group_id, &[bob_kp.key_package_bytes])
            .await
            .unwrap();
        assert!(!out.welcomes.is_empty());

        // Bob joins via welcome
        let welcome_bytes = out.welcomes[0].clone();
        bob_rt.join_via_welcome(&bob, &welcome_bytes).await.unwrap();

        // Alice sends a message
        let envelope = ApplicationEnvelope {
            client_message_id: Uuid::now_v7(),
            kind: AppMessageKind::Text,
            body: AppMessageBody::Text {
                text: "hello bob".into(),
                formatted_html: None,
            },
            reply_to_message_id: None,
            thread_root_id: None,
            created_at: 0,
            sender_display_name_override: None,
        };
        let plaintext = rmp_serde::to_vec_named(&envelope).unwrap();
        let ct = alice_rt
            .encrypt_application_message(group_id, &alice, &plaintext)
            .await
            .unwrap();

        // Bob decrypts
        let dec = bob_rt.decrypt_application_message(group_id, &ct).await.unwrap();
        let recv: ApplicationEnvelope = rmp_serde::from_slice(&dec.plaintext).unwrap();
        assert_eq!(
            match recv.body {
                AppMessageBody::Text { text, .. } => text,
                _ => panic!("expected text"),
            },
            "hello bob"
        );
    }

    #[tokio::test]
    async fn test_mls_add_remove_member() {
        let store = Arc::new(MockStore::new());

        let alice = ClientIdentity::generate_new_user(Uuid::now_v7(), "alice".into(), Uuid::now_v7());
        let bob = ClientIdentity::generate_new_user(Uuid::now_v7(), "bob".into(), Uuid::now_v7());
        let carol = ClientIdentity::generate_new_user(Uuid::now_v7(), "carol".into(), Uuid::now_v7());

        let alice_rt = MlsRuntime::new(store.clone(), alice.device_id);
        let bob_rt = MlsRuntime::new(store.clone(), bob.device_id);
        let carol_rt = MlsRuntime::new(store.clone(), carol.device_id);

        let bob_kp = bob_rt.generate_keypackage(&bob, 86_400, false).await.unwrap();

        let group_id = Uuid::now_v7();
        let out = alice_rt
            .create_group(&alice, group_id, &[bob_kp.key_package_bytes])
            .await
            .unwrap();

        bob_rt.join_via_welcome(&bob, &out.welcomes[0]).await.unwrap();

        // Alice adds Carol
        let carol_kp = carol_rt.generate_keypackage(&carol, 86_400, false).await.unwrap();
        let pending = alice_rt
            .propose_add(group_id, &alice, &[carol_kp.key_package_bytes])
            .await
            .unwrap();

        // Merge the add commit
        alice_rt.merge_pending(group_id).await.unwrap();

        // Verify Carol can join
        carol_rt.join_via_welcome(&carol, &pending.welcomes[0])
            .await
            .unwrap();

        // Alice removes Bob (leaf index 1 — Bob)
        let _pending_remove = alice_rt
            .propose_remove(group_id, &alice, &[1])
            .await
            .unwrap();
        alice_rt.merge_pending(group_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_reaction_blind_index_stable() {
        let store = Arc::new(MockStore::new());

        let alice = ClientIdentity::generate_new_user(Uuid::now_v7(), "alice".into(), Uuid::now_v7());
        let bob = ClientIdentity::generate_new_user(Uuid::now_v7(), "bob".into(), Uuid::now_v7());

        let alice_rt = MlsRuntime::new(store.clone(), alice.device_id);
        let bob_rt = MlsRuntime::new(store.clone(), bob.device_id);

        let bob_kp = bob_rt.generate_keypackage(&bob, 86_400, false).await.unwrap();

        let group_id = Uuid::now_v7();
        let out = alice_rt
            .create_group(&alice, group_id, &[bob_kp.key_package_bytes])
            .await
            .unwrap();
        bob_rt.join_via_welcome(&bob, &out.welcomes[0]).await.unwrap();

        // Export secret for blind index (from Alice's perspective)
        let (group, provider) = alice_rt.load_group(group_id).await.unwrap();
        let exporter = group
            .export_secret(provider.crypto(), "messenger/reaction/v1", &[0u8; 16], 32)
            .unwrap();

        let msg1 = Uuid::now_v7();
        let msg2 = Uuid::now_v7();

        let idx1_a = crate::mls::exporter::reaction_blind_index(&exporter, msg1, "👍").unwrap();
        let idx1_b = crate::mls::exporter::reaction_blind_index(&exporter, msg1, "👍").unwrap();
        let idx2 = crate::mls::exporter::reaction_blind_index(&exporter, msg2, "👍").unwrap();

        assert_eq!(idx1_a, idx1_b, "same message+emoji should yield same index");
        assert_ne!(idx1_a, idx2, "different message should yield different index");
    }
}

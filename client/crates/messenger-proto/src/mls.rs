use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single welcome payload for a recipient device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WelcomePayload {
    pub recipient_device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub welcome_ciphertext: Vec<u8>,
}

/// One member device included at group creation.
///
/// Field names and types mirror the server's `MemberDeviceInit` exactly so the
/// rmp_serde-named payload decodes correctly.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberDeviceInit {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub leaf_index: i32,
    pub role_in_chat: String, // "owner" | "member"
}

/// Request to create a new MLS group.
///
/// Mirrors the server `CreateGroupRequest`: the creator must be among
/// `member_devices`, and `welcomes` carries one entry per recipient device (the
/// single batched MLS welcome replicated per device). The ratchet tree travels
/// inside the welcome via the ratchet-tree extension, so it isn't sent here.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateGroupRequest {
    pub group_type: String, // "direct" | "group"
    #[serde(with = "serde_bytes")]
    pub initial_commit: Vec<u8>,
    pub welcomes: Vec<WelcomePayload>,
    pub member_devices: Vec<MemberDeviceInit>,
}

/// Request to transfer group ownership to another active member.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransferOwnerRequest {
    pub new_owner_user_id: Uuid,
}

/// Request to create a direct chat on the server side (without MLS).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateDirectChatRequest {
    pub target_username: String,
}

/// Response after creating a group.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateGroupResponse {
    pub group_id: Uuid,
    pub epoch: i64,
    pub created_at: i64,
}

/// Member change hint sent alongside a commit.
///
/// Mirrors the server `MemberChange`. `leaf_index`/`role_in_chat` are optional
/// metadata (used when adding a device); they're `None` for removes.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberChange {
    pub kind: String, // "add" | "remove"
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub leaf_index: Option<i32>,
    pub role_in_chat: Option<String>,
}

/// Request to post a commit to a group.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PostCommitRequest {
    pub expected_epoch: i64,
    #[serde(with = "serde_bytes")]
    pub commit: Vec<u8>,
    pub welcomes: Vec<WelcomePayload>,
    pub member_changes: Vec<MemberChange>,
}

/// Response after posting a commit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PostCommitResponse {
    pub message_id: Uuid,
    pub new_epoch: i64,
}

/// State of a message (edit/delete tracking).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageState {
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub replacement_message_id: Option<Uuid>,
}

/// A single message returned by the server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredMessage {
    pub id: Uuid,
    pub group_id: Uuid,
    pub epoch: i64,
    pub sender_user_id: Uuid,
    pub sender_device_id: Uuid,
    pub wire_format: String,
    #[serde(with = "serde_bytes")]
    pub mls_ciphertext: Vec<u8>,
    pub parent_message_id: Option<Uuid>,
    pub thread_root_id: Option<Uuid>,
    pub reply_to_message_id: Option<Uuid>,
    pub client_message_id: Uuid,
    pub created_at: i64,
    pub state: Option<MessageState>,
}

/// Request to post an application message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PostMessageRequest {
    pub expected_epoch: i64,
    #[serde(with = "serde_bytes")]
    pub mls_ciphertext: Vec<u8>,
    pub parent_message_id: Option<Uuid>,
    pub reply_to_message_id: Option<Uuid>,
    pub thread_root_id: Option<Uuid>,
    pub client_message_id: Uuid,
}

/// Response after posting a message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PostMessageResponse {
    pub message_id: Uuid,
    pub created_at: i64,
}

/// Response listing messages in a group.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListMessagesResponse {
    pub messages: Vec<StoredMessage>,
}

/// A group member with epoch metadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupMember {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub role_in_chat: String,
    pub joined_at_epoch: i64,
    pub left_at_epoch: Option<i64>,
    pub joined_at: i64,
}

/// Response listing group members.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListGroupMembersResponse {
    pub members: Vec<GroupMember>,
}

/// Summary of a group for the "my groups" list.
/// Matches server's GroupSummary in routes/mls.rs.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupSummary {
    pub id: Uuid,
    pub group_type: String,
    pub current_epoch: i64,
    pub created_at: i64,
    pub role_in_chat: String,
    pub joined_at: i64,
}

/// Response listing my groups.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListGroupsResponse {
    pub groups: Vec<GroupSummary>,
    pub has_more: bool,
}

/// Per-device delivery status.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceDelivery {
    pub device_id: Uuid,
    pub delivered_at: i64,
}

/// Response for message delivery status.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeliveryStatusResponse {
    pub total_devices: i32,
    pub delivered_count: i32,
    pub per_device: Vec<DeviceDelivery>,
}

/// Request to update message state (edit/delete).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateMessageStateRequest {
    pub kind: String, // "edit" | "delete"
    pub replacement_message_id: Option<Uuid>,
}

/// A welcome entry in the "my welcomes" list.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WelcomeEntry {
    pub id: Uuid,
    pub group_id: Uuid,
    pub recipient_device_id: Uuid,
    pub epoch: i64,
    #[serde(with = "serde_bytes")]
    pub welcome_ciphertext: Vec<u8>,
}

/// Response listing pending welcomes.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListWelcomesResponse {
    pub welcomes: Vec<WelcomeEntry>,
}

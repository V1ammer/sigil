use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single welcome payload for a recipient device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WelcomePayload {
    pub recipient_device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub welcome_ciphertext: Vec<u8>,
}

/// Request to create a new MLS group.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateGroupRequest {
    pub group_type: String, // "direct" | "group"
    #[serde(with = "serde_bytes")]
    pub initial_commit: Vec<u8>,
    pub welcomes: Vec<WelcomePayload>,
    #[serde(with = "serde_bytes")]
    pub ratchet_tree: Vec<u8>,
}

/// Response after creating a group.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateGroupResponse {
    pub group_id: Uuid,
    pub epoch: i64,
    pub created_at: i64,
}

/// Member change hint sent alongside a commit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberChange {
    pub kind: String, // "add" | "remove"
    pub user_id: Uuid,
    pub device_id: Uuid,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupSummary {
    pub group_id: Uuid,
    pub group_type: String,
    pub current_epoch: i64,
    pub created_at: i64,
    pub created_by_user_id: Uuid,
}

/// Response listing my groups.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListGroupsResponse {
    pub groups: Vec<GroupSummary>,
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

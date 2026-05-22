use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Response after uploading an attachment.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadAttachmentResponse {
    pub attachment_id: Uuid,
    pub expires_at: i64,
}

/// Request to finalize an attachment (bind to a message).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FinalizeAttachmentRequest {
    pub message_id: Uuid,
}

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

/// Response after uploading one part of a chunked upload.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadPartResponse {
    pub received: u64,
}

/// Status of a streamed (chunked) upload — for resuming.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AttachmentStatusResponse {
    pub received: u64,
    pub padded_size: u64,
}

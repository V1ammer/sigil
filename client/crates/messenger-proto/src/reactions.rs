use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to add a reaction (server only stores blind index).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AddReactionRequest {
    pub message_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub reaction_blind_index: Vec<u8>,
}

/// Request to remove a reaction.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoveReactionRequest {
    pub message_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub reaction_blind_index: Vec<u8>,
}

use crate::api::{client::ApiClient, ApiError};
use messenger_proto::reactions::{AddReactionRequest, RemoveReactionRequest};
use uuid::Uuid;

impl ApiClient {
    /// Add a reaction (server-side dedup by blind index).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn add_reaction(
        &self,
        message_id: Uuid,
        req: &AddReactionRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/messages/{message_id}/reactions");
        self.send("POST", &path, Some(req)).await
    }

    /// Remove a reaction.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn remove_reaction(
        &self,
        message_id: Uuid,
        req: &RemoveReactionRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/messages/{message_id}/reactions");
        self.send("DELETE", &path, Some(req)).await
    }
}

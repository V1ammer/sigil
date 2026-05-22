use crate::api::{client::ApiClient, ApiError};
use messenger_proto::mls::*;
use uuid::Uuid;

impl ApiClient {
    /// Create a new MLS group.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or validation failure.
    pub async fn create_group(&self, req: &CreateGroupRequest) -> Result<CreateGroupResponse, ApiError> {
        self.send("POST", "/v1/groups", Some(req)).await
    }

    /// List groups the authenticated user belongs to.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn list_groups(&self, since: Option<Uuid>) -> Result<ListGroupsResponse, ApiError> {
        let path = if let Some(cursor) = since {
            format!("/v1/groups/me?since={}", cursor)
        } else {
            "/v1/groups/me".to_string()
        };
        self.send::<(), _>("GET", &path, None).await
    }

    /// List members of a group.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if group not found.
    pub async fn list_group_members(&self, group_id: Uuid) -> Result<ListGroupMembersResponse, ApiError> {
        let path = format!("/v1/groups/{}/members", group_id);
        self.send::<(), _>("GET", &path, None).await
    }

    /// Post a commit to a group.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or epoch conflict.
    pub async fn post_commit(&self, group_id: Uuid, req: &PostCommitRequest) -> Result<PostCommitResponse, ApiError> {
        let path = format!("/v1/groups/{}/commit", group_id);
        self.send("POST", &path, Some(req)).await
    }

    /// Pull messages from a group.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn list_messages(
        &self,
        group_id: Uuid,
        since: Option<Uuid>,
        limit: Option<u32>,
    ) -> Result<ListMessagesResponse, ApiError> {
        let mut path = format!("/v1/groups/{}/messages", group_id);
        let mut qs = Vec::new();
        if let Some(cursor) = since {
            qs.push(format!("since={}", cursor));
        }
        if let Some(l) = limit {
            qs.push(format!("limit={}", l));
        }
        if !qs.is_empty() {
            path.push('?');
            path.push_str(&qs.join("&"));
        }
        self.send::<(), _>("GET", &path, None).await
    }

    /// Post an application message to a group.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or epoch conflict.
    pub async fn post_message(
        &self,
        group_id: Uuid,
        req: &PostMessageRequest,
    ) -> Result<PostMessageResponse, ApiError> {
        let path = format!("/v1/groups/{}/messages", group_id);
        self.send("POST", &path, Some(req)).await
    }

    /// Get delivery status for a message.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if message not found.
    pub async fn message_delivery(&self, message_id: Uuid) -> Result<DeliveryStatusResponse, ApiError> {
        let path = format!("/v1/messages/{}/delivery", message_id);
        self.send::<(), _>("GET", &path, None).await
    }

    /// Update message state (edit / delete).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn update_message_state(
        &self,
        message_id: Uuid,
        req: &UpdateMessageStateRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/messages/{}/state", message_id);
        self.send("POST", &path, Some(req)).await
    }

    /// List pending welcomes for the current user.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure.
    pub async fn list_welcomes(&self, since: Option<Uuid>) -> Result<ListWelcomesResponse, ApiError> {
        let path = if let Some(cursor) = since {
            format!("/v1/welcomes/me?since={}", cursor)
        } else {
            "/v1/welcomes/me".to_string()
        };
        self.send::<(), _>("GET", &path, None).await
    }

    /// Acknowledge a welcome (mark as consumed).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or if welcome not found.
    pub async fn ack_welcome(&self, welcome_id: Uuid) -> Result<(), ApiError> {
        let path = format!("/v1/welcomes/{}/ack", welcome_id);
        self.send::<(), ()>("POST", &path, None).await
    }
}

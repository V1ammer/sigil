use crate::api::{client::ApiClient, ApiError};
use messenger_proto::auth::{RedeemRequest, RedeemResponse};
use messenger_proto::invites::*;
use uuid::Uuid;

impl ApiClient {
    /// Redeem an invite token.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network, auth, or validation failure.
    pub async fn redeem_invite(&self, req: &RedeemRequest) -> Result<RedeemResponse, ApiError> {
        self.send("POST", "/v1/invite/redeem", Some(req)).await
    }

    /// Create a new invite token (admin only).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn create_invite(
        &self,
        req: &CreateInviteRequest,
    ) -> Result<CreateInviteResponse, ApiError> {
        self.send("POST", "/v1/admin/invites", Some(req)).await
    }

    /// List active invite tokens (admin only).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn list_invites(&self) -> Result<ListInvitesResponse, ApiError> {
        self.send::<(), _>("GET", "/v1/admin/invites", None).await
    }

    /// Revoke an invite token (admin only).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn revoke_invite(&self, id: Uuid) -> Result<(), ApiError> {
        let path = format!("/v1/admin/invites/{}", id);
        self.send::<(), ()>("DELETE", &path, None).await
    }
}

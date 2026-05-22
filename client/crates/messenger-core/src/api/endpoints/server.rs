use crate::api::{client::ApiClient, ApiError};
use messenger_proto::server::ServerInfo;

impl ApiClient {
    /// Get public server information.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or parse failure.
    pub async fn server_info(&self) -> Result<ServerInfo, ApiError> {
        self.send::<(), ServerInfo>("GET", "/v1/server/info", None).await
    }
}

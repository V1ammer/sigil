//! HTTP transport trait and platform-specific defaults.

use async_trait::async_trait;

use super::{ApiError, HttpResponse};

/// Platform-agnostic HTTP transport.
#[async_trait(?Send)]
pub trait HttpTransport {
    /// Execute an HTTP request.
    async fn request(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, ApiError>;
}

/// Returns the default transport for the current platform.
#[must_use]
pub fn default_transport() -> Box<dyn HttpTransport> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(native::ReqwestTransport::new())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(web::GlooNetTransport)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

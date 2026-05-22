use serde::{Deserialize, Serialize};

/// API error body returned by the server on 4xx/5xx responses.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiErrorBody {
    pub code: String,
    pub details: Option<serde_json::Value>,
}

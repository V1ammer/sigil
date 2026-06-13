use crate::api::{client::ApiClient, ApiError};
use messenger_proto::attachments::*;
use uuid::Uuid;

impl ApiClient {
    /// Upload an encrypted attachment blob.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or validation failure.
    pub async fn upload_attachment(
        &self,
        ciphertext: Vec<u8>,
        padded_size: u64,
        size_bucket: u32,
    ) -> Result<UploadAttachmentResponse, ApiError> {
        let extra = vec![
            ("X-Attachment-Padded-Size".to_string(), padded_size.to_string()),
            ("X-Attachment-Size-Bucket".to_string(), size_bucket.to_string()),
        ];
        let resp = self.send_raw("POST", "/v1/attachments", extra, ciphertext).await?;
        crate::api::client::parse_response(resp)
    }

    /// Finalize an attachment by binding it to a message.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn finalize_attachment(
        &self,
        attachment_id: Uuid,
        req: &FinalizeAttachmentRequest,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/attachments/{}/finalize", attachment_id);
        self.send("POST", &path, Some(req)).await
    }

    /// Download an attachment. Supports optional byte range.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn download_attachment(
        &self,
        attachment_id: Uuid,
        range: Option<(u64, u64)>,
    ) -> Result<Vec<u8>, ApiError> {
        let path = format!("/v1/attachments/{}", attachment_id);
        let mut extra = Vec::new();
        if let Some((start, end)) = range {
            extra.push((
                "Range".to_string(),
                format!("bytes={}-{}", start, end),
            ));
        }
        let resp = self.send_raw("GET", &path, extra, Vec::new()).await?;
        // Без этой проверки тело ошибки (msgpack `{code: ...}`, ~20 байт)
        // возвращалось бы как «шифртекст» и падало в decrypt с aead::Error.
        // Проверяем статус и отдаём осмысленную ApiError::Api.
        if resp.status >= 400 {
            let err: messenger_proto::error::ApiErrorBody =
                rmp_serde::from_slice(&resp.body).unwrap_or_else(|_| {
                    messenger_proto::error::ApiErrorBody {
                        code: format!("HTTP_{}", resp.status),
                        details: None,
                    }
                });
            return Err(ApiError::Api {
                status: resp.status,
                body: err,
            });
        }
        Ok(resp.body)
    }
}

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

    /// Upload an attachment, transparently choosing single-shot (small) or
    /// chunked/resumable (large) transport. Chunked uploads send fixed-size
    /// parts and re-sync from the server's received count on a `409` offset
    /// conflict, so a dropped part doesn't restart the whole upload.
    ///
    /// `padded_size` must equal `ciphertext.len()`.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or validation failure.
    pub async fn upload_attachment_smart(
        &self,
        ciphertext: Vec<u8>,
        padded_size: u64,
        size_bucket: u32,
    ) -> Result<UploadAttachmentResponse, ApiError> {
        /// Files at/under this go in one request; above, chunked.
        const SINGLE_SHOT_MAX: usize = 8 * 1024 * 1024;
        /// Per-part size for chunked uploads (fits the server's part body limit).
        const PART_SIZE: usize = 4 * 1024 * 1024;

        if ciphertext.len() <= SINGLE_SHOT_MAX {
            return self.upload_attachment(ciphertext, padded_size, size_bucket).await;
        }

        let init = self.init_attachment(padded_size, size_bucket).await?;
        let id = init.attachment_id;
        let total = ciphertext.len() as u64;
        let mut offset: u64 = 0;
        while offset < total {
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            let end = start.saturating_add(PART_SIZE).min(ciphertext.len());
            let part = ciphertext[start..end].to_vec();
            match self.upload_attachment_part(id, offset, part).await {
                Ok(resp) => offset = resp.received,
                // Offset conflict — re-sync from the server's authoritative count.
                Err(ApiError::Api { status: 409, .. }) => {
                    offset = self.attachment_status(id).await?.received;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(init)
    }

    /// Initialize a chunked (resumable) upload for a large attachment. Declares
    /// the total ciphertext size; parts are then sent with `upload_attachment_part`.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or validation failure.
    pub async fn init_attachment(
        &self,
        padded_size: u64,
        size_bucket: u32,
    ) -> Result<UploadAttachmentResponse, ApiError> {
        let extra = vec![
            ("X-Attachment-Padded-Size".to_string(), padded_size.to_string()),
            ("X-Attachment-Size-Bucket".to_string(), size_bucket.to_string()),
        ];
        let resp = self.send_raw("POST", "/v1/attachments/init", extra, Vec::new()).await?;
        crate::api::client::parse_response(resp)
    }

    /// Upload one part of a chunked attachment at `offset` (must equal the bytes
    /// already received). Returns the new total received.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure or a `409` offset/state conflict.
    pub async fn upload_attachment_part(
        &self,
        attachment_id: Uuid,
        offset: u64,
        part: Vec<u8>,
    ) -> Result<UploadPartResponse, ApiError> {
        let path = format!("/v1/attachments/{attachment_id}/parts");
        let extra = vec![("X-Attachment-Offset".to_string(), offset.to_string())];
        let resp = self.send_raw("POST", &path, extra, part).await?;
        if resp.status >= 400 {
            let err: messenger_proto::error::ApiErrorBody =
                rmp_serde::from_slice(&resp.body).unwrap_or_else(|_| {
                    messenger_proto::error::ApiErrorBody {
                        code: format!("HTTP_{}", resp.status),
                        details: None,
                    }
                });
            return Err(ApiError::Api { status: resp.status, body: err });
        }
        crate::api::client::parse_response(resp)
    }

    /// Current received-byte count for a streamed upload (to resume after a
    /// failure or offset conflict).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network or permission failure.
    pub async fn attachment_status(
        &self,
        attachment_id: Uuid,
    ) -> Result<AttachmentStatusResponse, ApiError> {
        let path = format!("/v1/attachments/{attachment_id}/status");
        let resp = self.send_raw("GET", &path, Vec::new(), Vec::new()).await?;
        if resp.status >= 400 {
            let err: messenger_proto::error::ApiErrorBody =
                rmp_serde::from_slice(&resp.body).unwrap_or_else(|_| {
                    messenger_proto::error::ApiErrorBody {
                        code: format!("HTTP_{}", resp.status),
                        details: None,
                    }
                });
            return Err(ApiError::Api { status: resp.status, body: err });
        }
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

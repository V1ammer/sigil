//! Symmetric encryption of attachment payloads.
//!
//! Uses AES-256-GCM with a random 12-byte nonce prepended to ciphertext.
//! The decryption key is 32 random bytes (AES-256 key).

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

use crate::error::CryptoError;

/// Generate a random 32-byte AES-256 key.
#[must_use]
pub fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

/// Encrypt plaintext with AES-256-GCM.
///
/// Returns `iv (12 bytes) || ciphertext || tag (16 bytes)`.
///
/// # Errors
///
/// Returns `CryptoError` if encryption fails.
pub fn encrypt_attachment(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Crypto(format!("aes key init: {e}")))?;

    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::Crypto(format!("aes encrypt: {e}")))?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt ciphertext produced by [`encrypt_attachment`].
///
/// Input format: `iv (12 bytes) || ciphertext || tag (16 bytes)`.
///
/// # Errors
///
/// Returns `CryptoError` if decryption fails (wrong key or corrupted data).
pub fn decrypt_attachment(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < 12 {
        return Err(CryptoError::Crypto("attachment data too short".into()));
    }
    let (iv_bytes, ct) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Crypto(format!("aes key init: {e}")))?;
    let nonce = Nonce::from_slice(iv_bytes);

    cipher
        .decrypt(nonce, ct)
        .map_err(|e| CryptoError::Crypto(format!("aes decrypt: {e}")))
}

// ───────────────────────── chunked / streamable format ─────────────────────
//
// Whole-blob AES-GCM can't be decrypted from a byte range (one tag covers the
// whole ciphertext), so it can't be streamed. The chunked format below splits
// the plaintext into fixed-size chunks, each sealed independently with the same
// key and a per-chunk nonce derived from the chunk index. Because every chunk
// has a deterministic offset, a player can `Range`-fetch and decrypt chunk N on
// its own and feed it to MediaSource — playback starts on the first chunk.
//
// Layout: header(26) || chunk_0 || chunk_1 || …  where chunk_i is the AES-GCM
// sealing of plaintext bytes [i*chunk_size, …) and is `plaintext_len_i + 16`
// bytes. Header:
//   0..4   magic "SGCS"
//   4      version (1)
//   5      flags (0)
//   6..10  chunk_size      u32 LE
//   10..18 total_plain_len u64 LE
//   18..26 base_nonce      8 bytes  (nonce_i = base_nonce || (i as u32 BE))

/// Magic marking a chunked/streamable attachment blob.
pub const CHUNK_MAGIC: &[u8; 4] = b"SGCS";
/// Fixed header length of the chunked format.
pub const CHUNK_HEADER_LEN: usize = 26;
/// Default plaintext chunk size (256 KiB).
pub const DEFAULT_CHUNK_SIZE: u32 = 256 * 1024;
/// AES-GCM tag length appended to every chunk's ciphertext.
pub const CHUNK_TAG_LEN: u64 = 16;

fn chunk_nonce(base: &[u8; 8], index: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[0..8].copy_from_slice(base);
    n[8..12].copy_from_slice(&index.to_be_bytes());
    n
}

/// Encrypt `plaintext` into the chunked/streamable format.
///
/// # Errors
///
/// Returns `CryptoError` if encryption fails.
pub fn encrypt_attachment_chunked(
    key: &[u8; 32],
    plaintext: &[u8],
    chunk_size: u32,
) -> Result<Vec<u8>, CryptoError> {
    let chunk_size = chunk_size.max(1);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Crypto(format!("aes key init: {e}")))?;

    let mut base_nonce = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut base_nonce);

    let cs = chunk_size as usize;
    let mut out = Vec::with_capacity(CHUNK_HEADER_LEN + plaintext.len() + plaintext.len() / cs.max(1) * 16 + 16);
    out.extend_from_slice(CHUNK_MAGIC);
    out.push(1);
    out.push(0);
    out.extend_from_slice(&chunk_size.to_le_bytes());
    out.extend_from_slice(&(plaintext.len() as u64).to_le_bytes());
    out.extend_from_slice(&base_nonce);

    for (index, chunk) in plaintext.chunks(cs).enumerate() {
        let nonce_bytes = chunk_nonce(&base_nonce, index as u32);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, chunk)
            .map_err(|e| CryptoError::Crypto(format!("aes encrypt chunk {index}: {e}")))?;
        out.extend_from_slice(&ct);
    }
    Ok(out)
}

/// Parsed header of a chunked attachment blob.
#[derive(Clone, Copy, Debug)]
pub struct ChunkedHeader {
    /// Plaintext bytes per chunk.
    pub chunk_size: u32,
    /// Total plaintext length.
    pub total_len: u64,
    /// Per-blob nonce prefix.
    pub base_nonce: [u8; 8],
}

impl ChunkedHeader {
    /// Parse the 26-byte header. Returns `None` if `data` isn't a chunked blob.
    #[must_use]
    pub fn parse(data: &[u8]) -> Option<ChunkedHeader> {
        if data.len() < CHUNK_HEADER_LEN || &data[0..4] != CHUNK_MAGIC || data[4] != 1 {
            return None;
        }
        let chunk_size = u32::from_le_bytes(data[6..10].try_into().ok()?);
        if chunk_size == 0 {
            return None;
        }
        let total_len = u64::from_le_bytes(data[10..18].try_into().ok()?);
        let mut base_nonce = [0u8; 8];
        base_nonce.copy_from_slice(&data[18..26]);
        Some(ChunkedHeader { chunk_size, total_len, base_nonce })
    }

    /// Number of chunks in the blob.
    #[must_use]
    pub fn num_chunks(&self) -> u32 {
        if self.total_len == 0 {
            return 0;
        }
        let cs = u64::from(self.chunk_size);
        u32::try_from(self.total_len.div_ceil(cs)).unwrap_or(u32::MAX)
    }

    /// Plaintext length of chunk `index`.
    #[must_use]
    pub fn chunk_plaintext_len(&self, index: u32) -> u64 {
        let cs = u64::from(self.chunk_size);
        let start = u64::from(index) * cs;
        if start >= self.total_len {
            return 0;
        }
        (self.total_len - start).min(cs)
    }

    /// Inclusive ciphertext byte range `(start, end)` for chunk `index`.
    #[must_use]
    pub fn chunk_byte_range(&self, index: u32) -> (u64, u64) {
        let cs = u64::from(self.chunk_size);
        let start = CHUNK_HEADER_LEN as u64 + u64::from(index) * (cs + CHUNK_TAG_LEN);
        let end = start + self.chunk_plaintext_len(index) + CHUNK_TAG_LEN - 1;
        (start, end)
    }
}

/// Decrypt one chunk's ciphertext (no header — `base_nonce`/`index` come from
/// the parsed [`ChunkedHeader`]).
///
/// # Errors
///
/// Returns `CryptoError` on authentication failure.
pub fn decrypt_chunk(
    key: &[u8; 32],
    base_nonce: &[u8; 8],
    index: u32,
    ct: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Crypto(format!("aes key init: {e}")))?;
    let nonce_bytes = chunk_nonce(base_nonce, index);
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| CryptoError::Crypto(format!("aes decrypt chunk {index}: {e}")))
}

/// Decrypt a whole chunked blob (for the non-streaming fallback path).
///
/// # Errors
///
/// Returns `CryptoError` if the header is missing or a chunk fails to decrypt.
pub fn decrypt_attachment_chunked(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let header = ChunkedHeader::parse(data)
        .ok_or_else(|| CryptoError::Crypto("not a chunked attachment".into()))?;
    let mut out = Vec::with_capacity(usize::try_from(header.total_len).unwrap_or(0));
    for i in 0..header.num_chunks() {
        let (s, e) = header.chunk_byte_range(i);
        let (s, e) = (s as usize, e as usize);
        if e >= data.len() {
            return Err(CryptoError::Crypto("chunked blob truncated".into()));
        }
        out.extend_from_slice(&decrypt_chunk(key, &header.base_nonce, i, &data[s..=e])?);
    }
    Ok(out)
}

/// Decrypt an attachment in either format: chunked if it carries the magic,
/// otherwise the legacy whole-blob layout. Used by the non-streaming path so it
/// transparently handles both.
///
/// # Errors
///
/// Returns `CryptoError` on decryption failure.
pub fn decrypt_attachment_auto(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if ChunkedHeader::parse(data).is_some() {
        decrypt_attachment_chunked(key, data)
    } else {
        decrypt_attachment(key, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_encryption_key();
        let plaintext = b"hello voice message!";
        let encrypted = encrypt_attachment(&key, plaintext).unwrap();
        let decrypted = decrypt_attachment(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
        // Should have iv (12) + ciphertext + tag (16)
        assert!(encrypted.len() > 12);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_encryption_key();
        let key2 = generate_encryption_key();
        let plaintext = b"secret data";
        let encrypted = encrypt_attachment(&key1, plaintext).unwrap();
        let result = decrypt_attachment(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let key = generate_encryption_key();
        let encrypted = encrypt_attachment(&key, b"").unwrap();
        let decrypted = decrypt_attachment(&key, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn chunked_roundtrip_multi_chunk() {
        let key = generate_encryption_key();
        // 3.5 chunks at a 1000-byte chunk size.
        let plaintext: Vec<u8> = (0..3500u32).map(|i| (i % 251) as u8).collect();
        let blob = encrypt_attachment_chunked(&key, &plaintext, 1000).unwrap();
        // Whole-blob decode matches.
        assert_eq!(decrypt_attachment_chunked(&key, &blob).unwrap(), plaintext);
        assert_eq!(decrypt_attachment_auto(&key, &blob).unwrap(), plaintext);
    }

    #[test]
    fn chunked_per_chunk_offsets_decrypt_independently() {
        let key = generate_encryption_key();
        let plaintext: Vec<u8> = (0..3500u32).map(|i| (i % 251) as u8).collect();
        let blob = encrypt_attachment_chunked(&key, &plaintext, 1000).unwrap();
        let header = ChunkedHeader::parse(&blob).unwrap();
        assert_eq!(header.num_chunks(), 4);
        // Decrypt each chunk straight from its computed byte range — this mirrors
        // what the streaming player does with Range requests.
        let mut reassembled = Vec::new();
        for i in 0..header.num_chunks() {
            let (s, e) = header.chunk_byte_range(i);
            let ct = &blob[s as usize..=e as usize];
            reassembled.extend_from_slice(&decrypt_chunk(&key, &header.base_nonce, i, ct).unwrap());
        }
        assert_eq!(reassembled, plaintext);
        // Last chunk byte range must end exactly at the blob's end.
        let (_, last_end) = header.chunk_byte_range(header.num_chunks() - 1);
        assert_eq!(last_end as usize, blob.len() - 1);
    }

    #[test]
    fn chunked_wrong_chunk_index_fails() {
        let key = generate_encryption_key();
        let plaintext: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
        let blob = encrypt_attachment_chunked(&key, &plaintext, 1000).unwrap();
        let header = ChunkedHeader::parse(&blob).unwrap();
        let (s, e) = header.chunk_byte_range(0);
        // Decrypting chunk 0's bytes under index 1's nonce must fail.
        assert!(decrypt_chunk(&key, &header.base_nonce, 1, &blob[s as usize..=e as usize]).is_err());
    }

    #[test]
    fn legacy_blob_not_parsed_as_chunked() {
        let key = generate_encryption_key();
        let legacy = encrypt_attachment(&key, b"plain old whole-blob payload").unwrap();
        assert!(ChunkedHeader::parse(&legacy).is_none());
        // auto-decrypt still handles it via the legacy branch.
        assert_eq!(
            decrypt_attachment_auto(&key, &legacy).unwrap(),
            b"plain old whole-blob payload"
        );
    }
}

//! Content-addressed storage over the [`KvStore`] port.
//!
//! Bodies are addressed by their SHA-256 hash and compressed with zstd at level 3.
//! Reads verify the decompressed body against its address. Listing scans only
//! content keys and reads their small sibling timestamp values.

use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{ContentHash, KvError, KvStore};

const ZSTD_LEVEL: i32 = 3;

/// Prefix for compressed content values, followed by the content hash in hex.
pub const CONTENT_PREFIX: &str = "content:";

/// Prefix for last-put timestamps, followed by the content hash in hex.
pub const STORED_AT_PREFIX: &str = "content-at:";

/// Async port for hash-addressed content with verified reads and paged listing.
pub trait ContentStore: Send + Sync {
    /// Hash and store the plain body. Putting a held blob again leaves its bytes
    /// untouched and refreshes its stored time to now: the time is that of the last put.
    fn put(&self, body: &[u8]) -> impl Future<Output = Result<ContentHash, KvError>> + Send;

    /// Retrieve and verify a body, or return `None` if its hash is absent.
    fn get(
        &self,
        hash: ContentHash,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send;

    /// Check whether the content hash is present.
    fn has(&self, hash: ContentHash) -> impl Future<Output = Result<bool, KvError>> + Send;

    /// Remove the body and its stored time. Absent content is a no-op.
    fn delete(&self, hash: ContentHash) -> impl Future<Output = Result<(), KvError>> + Send;

    /// Hashes in ascending hex order, strictly after `after`, at most `limit`,
    /// each with the time of its last put ([`UNIX_EPOCH`] when none is recorded).
    fn list(
        &self,
        after: Option<ContentHash>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<(ContentHash, SystemTime)>, KvError>> + Send;
}

/// A [`ContentStore`] backed by any [`KvStore`], using zstd-compressed bodies.
///
/// This store is single-process: one process owns a store. Writes through this
/// instance are serialized; two processes sharing a store can race.
pub struct KvContentStore<K: KvStore> {
    inner: K,
    write: tokio::sync::Mutex<()>,
}

impl<K: KvStore> KvContentStore<K> {
    /// Wrap the given key-value backend as a content store.
    pub fn new(inner: K) -> Self {
        Self {
            inner,
            write: tokio::sync::Mutex::new(()),
        }
    }

    /// Borrow the underlying key-value backend.
    pub fn inner(&self) -> &K {
        &self.inner
    }

    async fn list_hashes(
        &self,
        after: Option<ContentHash>,
        limit: usize,
    ) -> Result<Vec<ContentHash>, KvError> {
        let mut cursor = after.map(key);
        let mut hashes = Vec::new();
        let mut skipped = 0usize;
        while hashes.len() < limit {
            let remaining = limit - hashes.len();
            let keys = self
                .inner
                .scan_keys(CONTENT_PREFIX, cursor.as_deref(), remaining)
                .await?;
            let scanned = keys.len();
            cursor = keys.last().cloned();
            for key in keys {
                if let Some(hash) = hash_from_key(&key) {
                    hashes.push(hash);
                } else {
                    skipped += 1;
                }
            }
            if scanned < remaining {
                break;
            }
        }
        if skipped > 0 {
            tracing::warn!(skipped, "Skipped non-canonical content keys");
        }
        Ok(hashes)
    }
}

impl<K: KvStore> ContentStore for KvContentStore<K> {
    async fn put(&self, body: &[u8]) -> Result<ContentHash, KvError> {
        let hash = ContentHash::from_bytes(body);
        let _write = self.write.lock().await;
        let stored_at = encode_time(SystemTime::now()).to_vec();
        if self.has(hash).await? {
            // Held bytes stay untouched: only the stored time moves to now.
            self.inner.put(&stored_at_key(hash), stored_at).await?;
            return Ok(hash);
        }
        let compressed = compress(body)?;
        self.inner.put(&stored_at_key(hash), stored_at).await?;
        self.inner.put(&key(hash), compressed).await?;
        Ok(hash)
    }

    async fn get(&self, hash: ContentHash) -> Result<Option<Vec<u8>>, KvError> {
        self.inner
            .get(&key(hash))
            .await?
            .map(|raw| decompress_verified(hash, &raw))
            .transpose()
    }

    async fn has(&self, hash: ContentHash) -> Result<bool, KvError> {
        self.inner.exists(&key(hash)).await
    }

    async fn delete(&self, hash: ContentHash) -> Result<(), KvError> {
        let _write = self.write.lock().await;
        self.inner.delete(&key(hash)).await?;
        self.inner.delete(&stored_at_key(hash)).await
    }

    async fn list(
        &self,
        after: Option<ContentHash>,
        limit: usize,
    ) -> Result<Vec<(ContentHash, SystemTime)>, KvError> {
        let hashes = self.list_hashes(after, limit).await?;
        let mut entries = Vec::with_capacity(hashes.len());
        for hash in hashes {
            let stored_at = match self.inner.get(&stored_at_key(hash)).await? {
                Some(raw) => decode_time(&raw)?,
                None => UNIX_EPOCH,
            };
            entries.push((hash, stored_at));
        }
        Ok(entries)
    }
}

fn key(hash: ContentHash) -> String {
    format!("{CONTENT_PREFIX}{hash}")
}

fn stored_at_key(hash: ContentHash) -> String {
    format!("{STORED_AT_PREFIX}{hash}")
}

fn encode_time(t: SystemTime) -> [u8; 8] {
    let millis = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    u64::try_from(millis).map_or(u64::MAX.to_be_bytes(), u64::to_be_bytes)
}

fn decode_time(raw: &[u8]) -> Result<SystemTime, KvError> {
    let bytes = <[u8; 8]>::try_from(raw).map_err(|e| {
        KvError::Serialization(format!(
            "stored time must be 8 bytes, got {}: {e}",
            raw.len()
        ))
    })?;
    UNIX_EPOCH
        .checked_add(Duration::from_millis(u64::from_be_bytes(bytes)))
        .ok_or_else(|| KvError::Serialization("stored time is out of range".into()))
}

fn compress(body: &[u8]) -> Result<Vec<u8>, KvError> {
    zstd::encode_all(body, ZSTD_LEVEL)
        .map_err(|e| KvError::Serialization(format!("zstd encode: {e}")))
}

fn decompress_verified(hash: ContentHash, raw: &[u8]) -> Result<Vec<u8>, KvError> {
    let body =
        zstd::decode_all(raw).map_err(|e| KvError::Serialization(format!("zstd decode: {e}")))?;
    if ContentHash::from_bytes(&body) != hash {
        return Err(KvError::HashMismatch(format!("expected {hash}")));
    }
    Ok(body)
}

fn hash_from_key(key: &str) -> Option<ContentHash> {
    let hex = key.strip_prefix(CONTENT_PREFIX)?;
    if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    ContentHash::try_from(hex).ok()
}

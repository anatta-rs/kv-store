//! Content-addressed storage over the [`KvStore`] port.
//!
//! Bodies are addressed by their SHA-256 hash and compressed with zstd at level 3.
//! Reads verify the decompressed body against its address. Listing scans only
//! content keys and reads their small sibling timestamp values.

use std::collections::HashSet;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{ContentHash, KvError, KvStore};

const ZSTD_LEVEL: i32 = 3;

/// Prefix for compressed content values, followed by the content hash in hex.
pub const CONTENT_PREFIX: &str = "content:";

/// Prefix for last-put timestamps, followed by the content hash in hex.
pub const STORED_AT_PREFIX: &str = "content-at:";

/// How a [`ContentStore::delete_many`] ask was answered, each asked hash in one list
/// (a hash asked twice is answered once), in the order first asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteOutcome {
    /// Hashes whose bytes and time were removed.
    pub destroyed: Vec<ContentHash>,
    /// Hashes left in place because their last put is younger than the grace.
    pub kept: Vec<ContentHash>,
    /// Hashes that were not held: never there, or already destroyed.
    pub absent: Vec<ContentHash>,
}

/// Settings of a [`KvContentStore`], given to [`KvContentStore::with_options`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentOptions {
    /// How long after its last put a hash is safe from [`ContentStore::delete_many`]:
    /// a put and a delete racing within this time leave the bytes in place.
    pub grace: Duration,
}

impl Default for ContentOptions {
    /// A grace of one hour.
    fn default() -> Self {
        Self {
            grace: Duration::from_secs(3600),
        }
    }
}

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

    /// Retrieve and verify many bodies with one backend call: one answer per asked
    /// hash, in the order asked, `None` in place of an absent hash. A hash asked
    /// twice is answered twice; a corrupted body makes the whole call an error.
    /// An empty ask is an empty answer and makes no backend call.
    fn get_many(
        &self,
        hashes: &[ContentHash],
    ) -> impl Future<Output = Result<Vec<Option<Vec<u8>>>, KvError>> + Send;

    /// Hash and store many plain bodies with one read and one write on the backend:
    /// one hash per body, in order. Held bodies only have their stored time
    /// refreshed, as [`ContentStore::put`] does. An empty ask makes no backend call.
    fn put_many(
        &self,
        bodies: &[&[u8]],
    ) -> impl Future<Output = Result<Vec<ContentHash>, KvError>> + Send;

    /// Check whether the content hash is present.
    fn has(&self, hash: ContentHash) -> impl Future<Output = Result<bool, KvError>> + Send;

    /// Remove the body and its stored time. Absent content is a no-op.
    fn delete(&self, hash: ContentHash) -> impl Future<Output = Result<(), KvError>> + Send;

    /// Destroy the bodies and stored times of the hashes whose last put is older than
    /// the store's grace, in one batch, and say which hashes were destroyed, which
    /// were kept (a put within the grace) and which were not held. A put always wins:
    /// it never interleaves with this call, so a put issued after the read of the times
    /// waits and re-creates the hash, and a put before it keeps the hash. An empty ask
    /// makes no backend call.
    fn delete_many(
        &self,
        hashes: &[ContentHash],
    ) -> impl Future<Output = Result<DeleteOutcome, KvError>> + Send;

    /// As [`ContentStore::delete_many`] but ignoring the grace, for an offline repair:
    /// every held hash is destroyed. Returns the destroyed hashes.
    fn delete_many_force(
        &self,
        hashes: &[ContentHash],
    ) -> impl Future<Output = Result<Vec<ContentHash>, KvError>> + Send;

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
    grace: Duration,
}

impl<K: KvStore> KvContentStore<K> {
    /// Wrap the given key-value backend as a content store.
    pub fn new(inner: K) -> Self {
        Self::with_options(inner, ContentOptions::default())
    }

    /// Wrap the given key-value backend as a content store with the given settings.
    pub fn with_options(inner: K, options: ContentOptions) -> Self {
        Self {
            inner,
            write: tokio::sync::Mutex::new(()),
            grace: options.grace,
        }
    }

    /// Borrow the underlying key-value backend.
    pub fn inner(&self) -> &K {
        &self.inner
    }

    // Runs under the write lock: no put slips between the read of the times and the batch.
    async fn delete_stale(
        &self,
        hashes: &[ContentHash],
        grace: Option<Duration>,
    ) -> Result<DeleteOutcome, KvError> {
        let mut seen = HashSet::with_capacity(hashes.len());
        let distinct: Vec<ContentHash> =
            hashes.iter().copied().filter(|h| seen.insert(*h)).collect();
        let mut outcome = DeleteOutcome::default();
        if distinct.is_empty() {
            return Ok(outcome);
        }
        let _write = self.write.lock().await;
        let now = SystemTime::now();
        let time_keys: Vec<String> = distinct.iter().copied().map(stored_at_key).collect();
        let raws = self.inner.get_many(&time_keys).await?;
        if raws.len() != distinct.len() {
            return Err(KvError::Backend(format!(
                "get_many answered {} values for {} keys",
                raws.len(),
                distinct.len()
            )));
        }
        let mut doomed = Vec::new();
        for (hash, raw) in distinct.into_iter().zip(raws) {
            let last_put = raw.map(|raw| decode_time(&raw)).transpose()?;
            if let (Some(grace), Some(last_put)) = (grace, last_put)
                && is_younger(now, last_put, grace)
            {
                outcome.kept.push(hash);
            } else if self.has(hash).await? {
                doomed.extend([key(hash), stored_at_key(hash)]);
                outcome.destroyed.push(hash);
            } else {
                if last_put.is_some() {
                    doomed.push(stored_at_key(hash));
                }
                outcome.absent.push(hash);
            }
        }
        if !doomed.is_empty() {
            self.inner.delete_many(&doomed).await?;
        }
        Ok(outcome)
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

    async fn get_many(&self, hashes: &[ContentHash]) -> Result<Vec<Option<Vec<u8>>>, KvError> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }
        let keys: Vec<String> = hashes.iter().copied().map(key).collect();
        let raws = self.inner.get_many(&keys).await?;
        if raws.len() != hashes.len() {
            return Err(KvError::Backend(format!(
                "get_many answered {} values for {} keys",
                raws.len(),
                hashes.len()
            )));
        }
        hashes
            .iter()
            .zip(raws)
            .map(|(hash, raw)| raw.map(|raw| decompress_verified(*hash, &raw)).transpose())
            .collect()
    }

    async fn put_many(&self, bodies: &[&[u8]]) -> Result<Vec<ContentHash>, KvError> {
        if bodies.is_empty() {
            return Ok(Vec::new());
        }
        let hashes: Vec<ContentHash> = bodies
            .iter()
            .map(|body| ContentHash::from_bytes(body))
            .collect();
        let _write = self.write.lock().await;
        let stored_at = encode_time(SystemTime::now()).to_vec();
        // Each distinct hash once, with the first body that has it.
        let mut distinct: Vec<(ContentHash, &[u8])> = Vec::with_capacity(bodies.len());
        let mut seen = HashSet::with_capacity(bodies.len());
        for (hash, body) in hashes.iter().zip(bodies) {
            if seen.insert(*hash) {
                distinct.push((*hash, body));
            }
        }
        let keys: Vec<String> = distinct.iter().map(|(hash, _)| key(*hash)).collect();
        let held = self.inner.get_many(&keys).await?;
        if held.len() != distinct.len() {
            return Err(KvError::Backend(format!(
                "get_many answered {} values for {} keys",
                held.len(),
                distinct.len()
            )));
        }
        let mut entries = Vec::with_capacity(distinct.len() * 2);
        for ((hash, body), held) in distinct.into_iter().zip(held) {
            entries.push((stored_at_key(hash), stored_at.clone()));
            if held.is_none() {
                entries.push((key(hash), compress(body)?));
            }
        }
        self.inner.put_many(entries).await?;
        Ok(hashes)
    }

    async fn has(&self, hash: ContentHash) -> Result<bool, KvError> {
        self.inner.exists(&key(hash)).await
    }

    async fn delete(&self, hash: ContentHash) -> Result<(), KvError> {
        let _write = self.write.lock().await;
        self.inner.delete(&key(hash)).await?;
        self.inner.delete(&stored_at_key(hash)).await
    }

    async fn delete_many(&self, hashes: &[ContentHash]) -> Result<DeleteOutcome, KvError> {
        self.delete_stale(hashes, Some(self.grace)).await
    }

    async fn delete_many_force(&self, hashes: &[ContentHash]) -> Result<Vec<ContentHash>, KvError> {
        Ok(self.delete_stale(hashes, None).await?.destroyed)
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

// A time in the future (clock skew) counts as just now.
fn is_younger(now: SystemTime, last_put: SystemTime, grace: Duration) -> bool {
    match now.duration_since(last_put) {
        Ok(age) => age < grace,
        Err(_) => true,
    }
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

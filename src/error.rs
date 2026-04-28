//! Error types for the KV store.

/// Errors produced by [`KvStore`](crate::KvStore) operations.
#[derive(Debug, thiserror::Error)]
pub enum KvError {
    /// The underlying storage backend failed.
    #[error("storage backend error: {0}")]
    Backend(String),

    /// The key was invalid (empty, too long, etc.).
    #[error("invalid key: {0}")]
    InvalidKey(String),

    /// Serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Content hash mismatch detected during collision verification.
    #[error("content hash mismatch: {0}")]
    HashMismatch(String),
}

#[cfg(feature = "rocksdb-backend")]
impl From<rocksdb::Error> for KvError {
    fn from(err: rocksdb::Error) -> Self {
        Self::Backend(err.into_string())
    }
}

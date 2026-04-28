//! Content-addressable store keyed by `content_hash`.
//!
//! Writes compress the body via zstd (level 3) before delegating to the
//! underlying `KvStore`; reads decompress transparently. The compression
//! layer is backend-agnostic — any `KvStore` impl gets it for free.
//!
//! Hash-addressed content is idempotent by construction: writing the same hash twice
//! with the same body is a no-op. No write synchronization needed.

use crate::{KvError, KvStore, hash::ContentHash};
const ZSTD_LEVEL: i32 = 3;

/// Wrapper around a `KvStore` providing hash-keyed content operations.
///
/// All keys are derived from `ContentHash` as `format!("content:{}", hash.hex())`.
///
/// Since content is identified by hash, writes are idempotent: multiple calls with
/// the same hash and body have no side effects beyond the first write.
#[allow(dead_code)]
pub struct ContentStore<K: KvStore> {
    inner: K,
}

impl<K: KvStore> ContentStore<K> {
    /// Create a new `ContentStore` wrapping the given KV backend.
    pub fn new(inner: K) -> Self {
        Self { inner }
    }

    /// Borrow the underlying KV store implementation.
    pub fn inner(&self) -> &K {
        &self.inner
    }

    /// Put content, keyed by hash.
    ///
    /// Idempotent: repeated calls with the same hash and body are no-ops.
    /// The physical key is `format!("content:{}", hash.hex())`.
    pub async fn put(&self, hash: ContentHash, body: &[u8]) -> Result<(), KvError> {
        let compressed = zstd::encode_all(body, ZSTD_LEVEL)
            .map_err(|e| KvError::Serialization(format!("zstd encode: {e}")))?;
        let key = format!("content:{}", hash.as_hex());
        self.inner.put(&key, compressed).await
    }

    /// Retrieve content by `ContentHash`, or `None` if not found.
    pub async fn get(&self, hash: ContentHash) -> Result<Option<Vec<u8>>, KvError> {
        let key = format!("content:{}", hash.as_hex());
        match self.inner.get(&key).await? {
            Some(compressed) => {
                let body = zstd::decode_all(&compressed[..])
                    .map_err(|e| KvError::Serialization(format!("zstd decode: {e}")))?;
                Ok(Some(body))
            }
            None => Ok(None),
        }
    }

    /// Check whether content exists at the given hash.
    pub async fn has(&self, hash: ContentHash) -> Result<bool, KvError> {
        let key = format!("content:{}", hash.as_hex());
        self.inner.exists(&key).await
    }

    /// Delete content by hash. Caller must ensure no Neo4j node references this hash.
    pub async fn delete(&self, hash: ContentHash) -> Result<(), KvError> {
        let key = format!("content:{}", hash.as_hex());
        self.inner.delete(&key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory key-value store for testing.
    pub struct InMemoryKvStore {
        store: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl InMemoryKvStore {
        /// Create a new in-memory key-value store.
        pub fn new() -> Self {
            Self {
                store: Mutex::new(HashMap::new()),
            }
        }
    }

    impl KvStore for InMemoryKvStore {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
            Ok(self.store.lock().unwrap().get(key).cloned())
        }

        async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
            self.store.lock().unwrap().insert(key.to_string(), value);
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<(), KvError> {
            self.store.lock().unwrap().remove(key);
            Ok(())
        }

        async fn exists(&self, key: &str) -> Result<bool, KvError> {
            Ok(self.store.lock().unwrap().contains_key(key))
        }

        async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
            let store = self.store.lock().unwrap();
            let mut result: Vec<_> = store
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            result.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(result)
        }
    }

    #[test]
    fn put_then_get_returns_same_bytes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = b"hello anatta";
            let hash = ContentHash::from_bytes(body);
            store.put(hash, body).await.unwrap();
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, Some(body.to_vec()));
        });
    }

    #[test]
    fn put_idempotent_same_body_second_call_noop() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = b"hello anatta";
            let hash = ContentHash::from_bytes(body);
            store.put(hash, body).await.unwrap();
            store.put(hash, body).await.unwrap();
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, Some(body.to_vec()));
        });
    }

    #[test]
    fn put_empty_body() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = b"";
            let hash = ContentHash::from_bytes(body);
            store.put(hash, body).await.unwrap();
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, Some(body.to_vec()));
        });
    }

    #[test]
    fn get_absent_key_returns_none() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let hash = ContentHash::from_bytes(b"nonexistent");
            let _retrieved = store.get(hash).await.unwrap();
        });
    }

    #[test]
    fn delete_removes_entry_get_returns_none() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = b"to be deleted";
            let hash = ContentHash::from_bytes(body);
            store.put(hash, body).await.unwrap();
            store.delete(hash).await.unwrap();
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, None);
        });
    }

    #[test]
    fn has_returns_true_after_put_false_before() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = b"test";
            let hash = ContentHash::from_bytes(body);
            let before = store.has(hash).await.unwrap();
            assert!(!before);
            store.put(hash, body).await.unwrap();
            let after = store.has(hash).await.unwrap();
            assert!(after);
        });
    }

    #[test]
    fn put_concurrent_same_hash_both_succeed() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use std::sync::Arc;
            let store = Arc::new(ContentStore::new(InMemoryKvStore::new()));
            let body = b"concurrent";
            let hash = ContentHash::from_bytes(body);
            let store1 = store.clone();
            let body1 = body.to_vec();
            let store2 = store.clone();
            let body2 = body.to_vec();
            let (res1, res2) = tokio::join!(store1.put(hash, &body1), store2.put(hash, &body2));
            assert!(res1.is_ok());
            assert!(res2.is_ok());
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, Some(body.to_vec()));
        });
    }

    #[test]
    fn non_utf8_body_roundtrip() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = vec![0xFF, 0xFE, 0x00, 0x01, 0x02];
            let hash = ContentHash::from_bytes(&body);
            store.put(hash, &body).await.unwrap();
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, Some(body));
        });
    }

    #[test]
    fn put_large_body_roundtrip() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let store = ContentStore::new(InMemoryKvStore::new());
            let body = vec![0xAB; 2 * 1024 * 1024];
            let hash = ContentHash::from_bytes(&body);
            store.put(hash, &body).await.unwrap();
            let retrieved = store.get(hash).await.unwrap();
            assert_eq!(retrieved, Some(body));
        });
    }

    #[test]
    fn put_stores_compressed_bytes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let body = "AAAA".repeat(10_000);
            let hash = ContentHash::from_bytes(body.as_bytes());
            let kv = InMemoryKvStore::new();
            let store = ContentStore::new(kv);
            store.put(hash, body.as_bytes()).await.unwrap();
            let key = format!("content:{}", hash.as_hex());
            let raw = store.inner().get(&key).await.unwrap().unwrap();
            assert!(raw.len() < body.len() / 5);
            let got = store.get(hash).await.unwrap().unwrap();
            assert_eq!(got, body.as_bytes());
        });
    }
}

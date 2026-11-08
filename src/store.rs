//! [`KvStore`] trait -- the abstract key-value interface.
//!
//! All keys are strings (Anatta entity IDs). Values are raw bytes;
//! serialization is the caller's responsibility.

use std::future::Future;

use crate::KvError;

/// Abstract async key-value store.
///
/// Implementations must be `Send + Sync` for use across async tasks.
/// Keys follow the Anatta ID convention (e.g. `evt:system:local_id`).
pub trait KvStore: Send + Sync {
    /// Retrieve the value associated with `key`, or `None` if absent.
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send;

    /// Insert or overwrite `key` with `value`.
    fn put(&self, key: &str, value: Vec<u8>) -> impl Future<Output = Result<(), KvError>> + Send;

    /// Remove `key`. No-op if the key does not exist.
    fn delete(&self, key: &str) -> impl Future<Output = Result<(), KvError>> + Send;

    /// Check whether `key` exists.
    fn exists(&self, key: &str) -> impl Future<Output = Result<bool, KvError>> + Send;

    /// Iterate all entries whose key starts with `prefix`.
    ///
    /// Returns pairs of `(key, value)` in lexicographic key order.
    fn scan_prefix(
        &self,
        prefix: &str,
    ) -> impl Future<Output = Result<Vec<(String, Vec<u8>)>, KvError>> + Send;

    /// Keys starting with `prefix`, strictly after `after`, ascending, at most `limit`; values not read.
    fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<String>, KvError>> + Send;
}

/// Blanket impl: any `&KvStore` is itself a `KvStore` (delegates to inner).
impl<T: KvStore + ?Sized> KvStore for &T {
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send {
        (**self).get(key)
    }

    fn put(&self, key: &str, value: Vec<u8>) -> impl Future<Output = Result<(), KvError>> + Send {
        (**self).put(key, value)
    }

    fn delete(&self, key: &str) -> impl Future<Output = Result<(), KvError>> + Send {
        (**self).delete(key)
    }

    fn exists(&self, key: &str) -> impl Future<Output = Result<bool, KvError>> + Send {
        (**self).exists(key)
    }

    fn scan_prefix(
        &self,
        prefix: &str,
    ) -> impl Future<Output = Result<Vec<(String, Vec<u8>)>, KvError>> + Send {
        (**self).scan_prefix(prefix)
    }

    fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<String>, KvError>> + Send {
        (**self).scan_keys(prefix, after, limit)
    }
}

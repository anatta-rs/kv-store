//! In-memory implementation of [`KvStore`] with lexicographically ordered keys.

use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::sync::Mutex;

use crate::{KvError, KvStore};

/// An ephemeral [`KvStore`] backed by a mutex-protected ordered map.
#[derive(Debug, Default)]
pub struct MemoryStore {
    map: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }

    fn guard(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>>, KvError> {
        self.map.lock().map_err(|e| KvError::Backend(e.to_string()))
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "Keep store operations deferred until the returned future is polled"
)]
impl KvStore for MemoryStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        let map = self.guard()?;
        Ok(map.get(key).cloned())
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        let mut map = self.guard()?;
        map.insert(key.to_owned(), value);
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        let mut map = self.guard()?;
        map.remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, KvError> {
        let map = self.guard()?;
        Ok(map.contains_key(key))
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
        let map = self.guard()?;
        Ok(map
            .range::<str, _>((Included(prefix), Unbounded))
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }

    async fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, KvError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let start = match after {
            Some(after) if after >= prefix => Excluded(after),
            _ => Included(prefix),
        };
        let map = self.guard()?;
        Ok(map
            .range::<str, _>((start, Unbounded))
            .take_while(|(key, _)| key.starts_with(prefix))
            .take(limit)
            .map(|(key, _)| key.clone())
            .collect())
    }
}

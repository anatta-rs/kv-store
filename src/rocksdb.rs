//! `RocksDB` implementation of [`KvStore`].

use std::path::Path;

use rocksdb::{DB, IteratorMode, Options};

use crate::error::KvError;
use crate::store::KvStore;

/// [`KvStore`] backed by an embedded `RocksDB` instance.
pub struct RocksDbStore {
    db: DB,
}

impl RocksDbStore {
    /// Open (or create) a `RocksDB` database at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`KvError::Backend`] if the database cannot be opened.
    pub fn open(path: &Path) -> Result<Self, KvError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(0));
        let db = DB::open(&opts, path)?;
        Ok(Self { db })
    }
}

impl KvStore for RocksDbStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        Ok(self.db.get(key.as_bytes())?)
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        self.db.delete(key.as_bytes())?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, KvError> {
        Ok(self.db.get_pinned(key.as_bytes())?.is_some())
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            rocksdb::Direction::Forward,
        ));

        let prefix_bytes = prefix.as_bytes();
        let mut results = Vec::new();

        for item in iter {
            let (key, value) = item?;
            if !key.starts_with(prefix_bytes) {
                break;
            }
            let key_str = String::from_utf8(key.to_vec())
                .map_err(|e| KvError::Backend(format!("non-UTF8 key: {e}")))?;
            results.push((key_str, value.to_vec()));
        }

        Ok(results)
    }
}

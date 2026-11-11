//! `RocksDB` implementation of [`KvStore`].

use std::path::Path;

use rocksdb::{DB, IteratorMode, Options, WriteBatch, WriteOptions};

use crate::error::KvError;
use crate::store::KvStore;

/// Settings of a [`RocksDbStore`] given to [`RocksDbStore::open_with`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RocksDbOptions {
    /// Fsync the write-ahead log on every write (`put`, `put_many`, `delete`,
    /// `delete_many`): a write survives a power loss once it returns. Off by default:
    /// the log is written but not fsynced, so a crash of the process loses nothing
    /// while a crash of the machine may lose the last writes.
    pub sync_writes: bool,
}

/// [`KvStore`] backed by an embedded `RocksDB` instance.
pub struct RocksDbStore {
    db: DB,
    sync_writes: bool,
}

impl RocksDbStore {
    /// Open (or create) a `RocksDB` database at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`KvError::Backend`] if the database cannot be opened.
    pub fn open(path: &Path) -> Result<Self, KvError> {
        Self::open_with(path, RocksDbOptions::default())
    }

    /// Open (or create) a `RocksDB` database at `path` with the given settings.
    ///
    /// # Errors
    ///
    /// Returns [`KvError::Backend`] if the database cannot be opened.
    pub fn open_with(path: &Path, options: RocksDbOptions) -> Result<Self, KvError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(0));
        let db = DB::open(&opts, path)?;
        Ok(Self {
            db,
            sync_writes: options.sync_writes,
        })
    }

    fn write_options(&self) -> WriteOptions {
        let mut options = WriteOptions::default();
        options.set_sync(self.sync_writes);
        options
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "Keep store operations deferred until the returned future is polled"
)]
impl KvStore for RocksDbStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        Ok(self.db.get(key.as_bytes())?)
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        self.db
            .put_opt(key.as_bytes(), &value, &self.write_options())?;
        Ok(())
    }

    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, KvError> {
        self.db
            .multi_get(keys.iter().map(String::as_bytes))
            .into_iter()
            .map(|value| value.map_err(KvError::from))
            .collect()
    }

    async fn put_many(&self, entries: Vec<(String, Vec<u8>)>) -> Result<(), KvError> {
        let mut batch = WriteBatch::default();
        for (key, value) in &entries {
            batch.put(key.as_bytes(), value);
        }
        self.db.write_opt(batch, &self.write_options())?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        self.db.delete_opt(key.as_bytes(), &self.write_options())?;
        Ok(())
    }

    async fn delete_many(&self, keys: &[String]) -> Result<(), KvError> {
        let mut batch = WriteBatch::default();
        for key in keys {
            batch.delete(key.as_bytes());
        }
        self.db.write_opt(batch, &self.write_options())?;
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

    async fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, KvError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let start = after.map_or(prefix, |after| prefix.max(after));
        let mut iter = self.db.raw_iterator();
        iter.seek(start.as_bytes());
        let mut results = Vec::new();

        while let Some(key) = iter.key() {
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            if Some(key) != after.map(str::as_bytes) {
                let key = std::str::from_utf8(key)
                    .map_err(|e| KvError::Backend(format!("non-UTF8 key: {e}")))?;
                results.push(key.to_owned());
                if results.len() == limit {
                    break;
                }
            }
            iter.next();
        }
        iter.status()?;
        Ok(results)
    }
}

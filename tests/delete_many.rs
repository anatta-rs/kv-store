//! Conformance cases for `delete_many`: a put always wins, a delete never destroys a
//! recent hash (task 004). Time is real: a short grace and a sleep past it.

use std::sync::Arc;
#[cfg(feature = "memory-backend")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[cfg(feature = "memory-backend")]
use kv_store::KvError;
#[cfg(feature = "memory-backend")]
use kv_store::MemoryStore;
use kv_store::{ContentHash, ContentOptions, ContentStore, DeleteOutcome, KvContentStore, KvStore};
#[cfg(feature = "rocksdb-backend")]
use kv_store::{RocksDbOptions, RocksDbStore};

const HOUR: Duration = Duration::from_secs(3600);
const SHORT: Duration = Duration::from_millis(150);

async fn sleep_past(since: Instant, grace: Duration) {
    let target = grace + Duration::from_millis(60);
    while since.elapsed() <= target {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[cfg(feature = "memory-backend")]
fn hashes_of(bodies: &[Vec<u8>]) -> Vec<ContentHash> {
    bodies.iter().map(|b| ContentHash::from_bytes(b)).collect()
}

fn many(n: usize, tag: &str) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| format!("{tag} body {i}").into_bytes())
        .collect()
}

// Stores one body and returns its hash, with the time of the put as `last_put`.
async fn held<K: KvStore>(s: &KvContentStore<K>, body: &[u8]) -> ContentHash {
    s.put(body).await.unwrap()
}

async fn time_key_present<K: KvStore>(s: &KvContentStore<K>, hash: ContentHash) -> bool {
    s.inner()
        .exists(&format!("content-at:{hash}"))
        .await
        .unwrap()
}

// grace = one hour ---------------------------------------------------------------

async fn put_then_delete_within_grace_keeps_the_bytes<K: KvStore>(s: &KvContentStore<K>) {
    let hash = held(s, b"recent").await;
    let outcome = s.delete_many(&[hash]).await.unwrap();
    assert_eq!(
        outcome,
        DeleteOutcome {
            destroyed: vec![],
            kept: vec![hash],
            absent: vec![]
        }
    );
    assert_eq!(s.get(hash).await.unwrap(), Some(b"recent".to_vec()));
    assert!(time_key_present(s, hash).await);
}

async fn delete_many_of_an_absent_hash_is_absent_not_kept<K: KvStore>(s: &KvContentStore<K>) {
    let hash = ContentHash::from_bytes(b"never there");
    let outcome = s.delete_many(&[hash]).await.unwrap();
    assert_eq!(
        outcome,
        DeleteOutcome {
            destroyed: vec![],
            kept: vec![],
            absent: vec![hash]
        }
    );
    let outcome = s.delete_many(&[]).await.unwrap();
    assert_eq!(outcome, DeleteOutcome::default());
}

async fn delete_then_put_leaves_the_bytes<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"delete first, put second";
    let hash = ContentHash::from_bytes(body);
    s.delete_many(&[hash]).await.unwrap();
    s.put(body).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), Some(body.to_vec()));
}

async fn put_delete_put_leaves_the_bytes<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"put, delete, put";
    let hash = held(s, body).await;
    let destroyed = s.delete_many_force(&[hash]).await.unwrap();
    assert_eq!(destroyed, vec![hash]);
    assert!(!s.has(hash).await.unwrap());
    s.put(body).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), Some(body.to_vec()));
    assert!(time_key_present(s, hash).await);
}

async fn force_ignores_the_grace_and_lists_only_the_destroyed<K: KvStore>(s: &KvContentStore<K>) {
    let recent = held(s, b"recent but forced").await;
    let absent = ContentHash::from_bytes(b"absent");
    let destroyed = s.delete_many_force(&[recent, absent]).await.unwrap();
    assert_eq!(destroyed, vec![recent]);
    assert!(!s.has(recent).await.unwrap());
    assert!(!time_key_present(s, recent).await);
    assert_eq!(s.delete_many_force(&[]).await.unwrap(), vec![]);
}

async fn duplicates_in_the_ask_are_answered_once<K: KvStore>(s: &KvContentStore<K>) {
    let hash = held(s, b"asked twice").await;
    let outcome = s.delete_many(&[hash, hash]).await.unwrap();
    assert_eq!(outcome.kept, vec![hash]);
    let destroyed = s.delete_many_force(&[hash, hash]).await.unwrap();
    assert_eq!(destroyed, vec![hash]);
}

async fn a_blob_without_a_time_is_older_than_the_grace<K: KvStore>(s: &KvContentStore<K>) {
    let hash = held(s, b"no recorded time").await;
    s.inner()
        .delete(&format!("content-at:{hash}"))
        .await
        .unwrap();
    let outcome = s.delete_many(&[hash]).await.unwrap();
    assert_eq!(outcome.destroyed, vec![hash]);
    assert!(!s.has(hash).await.unwrap());
}

// short grace --------------------------------------------------------------------

async fn after_the_grace_the_bytes_are_destroyed_and_listed<K: KvStore>(s: &KvContentStore<K>) {
    let start = Instant::now();
    let hash = held(s, b"old enough").await;
    let early = s.delete_many(&[hash]).await.unwrap();
    assert!(
        start.elapsed() < SHORT,
        "the first ask must be inside the grace"
    );
    assert_eq!(early.kept, vec![hash]);
    sleep_past(start, SHORT).await;
    let outcome = s.delete_many(&[hash]).await.unwrap();
    assert_eq!(
        outcome,
        DeleteOutcome {
            destroyed: vec![hash],
            kept: vec![],
            absent: vec![]
        }
    );
    assert!(!s.has(hash).await.unwrap());
    assert!(!time_key_present(s, hash).await);
}

async fn a_mixed_ask_splits_destroyed_kept_and_absent<K: KvStore>(s: &KvContentStore<K>) {
    let start = Instant::now();
    let old = held(s, b"old one").await;
    sleep_past(start, SHORT).await;
    let young = held(s, b"young one").await;
    let absent = ContentHash::from_bytes(b"absent one");
    let outcome = s.delete_many(&[old, young, absent]).await.unwrap();
    assert_eq!(
        outcome,
        DeleteOutcome {
            destroyed: vec![old],
            kept: vec![young],
            absent: vec![absent]
        }
    );
    assert!(s.has(young).await.unwrap());
    assert!(!s.has(old).await.unwrap());
}

async fn a_re_put_after_the_grace_keeps_the_hash_again<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"refreshed by a re-put";
    let start = Instant::now();
    let hash = held(s, body).await;
    sleep_past(start, SHORT).await;
    s.put(body).await.unwrap();
    let outcome = s.delete_many(&[hash]).await.unwrap();
    assert_eq!(
        outcome.kept,
        vec![hash],
        "the last put counts, not the first"
    );
    assert!(s.has(hash).await.unwrap());
}

async fn a_put_after_a_destroying_delete_recreates<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"destroyed then recreated";
    let start = Instant::now();
    let hash = held(s, body).await;
    sleep_past(start, SHORT).await;
    assert_eq!(s.delete_many(&[hash]).await.unwrap().destroyed, vec![hash]);
    s.put(body).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), Some(body.to_vec()));
    assert_eq!(s.delete_many(&[hash]).await.unwrap().kept, vec![hash]);
}

// grace = zero: everything is old -------------------------------------------------

#[cfg(feature = "memory-backend")]
async fn delete_many_of_1000_hashes_is_one_backend_batch<K: KvStore>(
    s: &KvContentStore<Counting<K>>,
) {
    let all = many(1000, "thousand");
    let slices: Vec<&[u8]> = all.iter().map(Vec::as_slice).collect();
    let hashes = s.put_many(&slices).await.unwrap();
    let c = &s.inner().delete_many_calls;
    let outcome = s.delete_many(&hashes).await.unwrap();
    assert_eq!(outcome.destroyed, hashes);
    assert_eq!(c.load(Ordering::SeqCst), 1, "one backend delete_many");
    assert_eq!(
        s.inner().delete_calls.load(Ordering::SeqCst),
        0,
        "no delete per hash"
    );
    assert_eq!(s.list(None, 10).await.unwrap().len(), 0);
}

#[cfg(feature = "memory-backend")]
async fn a_failing_batch_leaves_no_partial_state<K: KvStore>(
    s: &KvContentStore<FailingDeleteMany<K>>,
) {
    let all = many(5, "failing");
    let slices: Vec<&[u8]> = all.iter().map(Vec::as_slice).collect();
    let hashes = s.put_many(&slices).await.unwrap();
    assert!(s.delete_many(&hashes).await.is_err());
    assert!(s.delete_many_force(&hashes).await.is_err());
    for hash in hashes_of(&all) {
        assert!(s.has(hash).await.unwrap());
        assert!(time_key_present_failing(s, hash).await);
    }
}

#[cfg(feature = "memory-backend")]
async fn time_key_present_failing<K: KvStore>(
    s: &KvContentStore<FailingDeleteMany<K>>,
    hash: ContentHash,
) -> bool {
    s.inner()
        .0
        .exists(&format!("content-at:{hash}"))
        .await
        .unwrap()
}

// wrappers -------------------------------------------------------------------------

#[cfg(feature = "memory-backend")]
struct Counting<K> {
    inner: K,
    delete_many_calls: AtomicUsize,
    delete_calls: AtomicUsize,
}

#[cfg(feature = "memory-backend")]
impl<K> Counting<K> {
    fn new(inner: K) -> Self {
        Self {
            inner,
            delete_many_calls: AtomicUsize::new(0),
            delete_calls: AtomicUsize::new(0),
        }
    }
}

#[cfg(feature = "memory-backend")]
impl<K: KvStore> KvStore for Counting<K> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        self.inner.get(key).await
    }
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, KvError> {
        self.inner.get_many(keys).await
    }
    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        self.inner.put(key, value).await
    }
    async fn put_many(&self, entries: Vec<(String, Vec<u8>)>) -> Result<(), KvError> {
        self.inner.put_many(entries).await
    }
    async fn delete(&self, key: &str) -> Result<(), KvError> {
        self.delete_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(key).await
    }
    async fn delete_many(&self, keys: &[String]) -> Result<(), KvError> {
        self.delete_many_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.delete_many(keys).await
    }
    async fn exists(&self, key: &str) -> Result<bool, KvError> {
        self.inner.exists(key).await
    }
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
        self.inner.scan_prefix(prefix).await
    }
    async fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, KvError> {
        self.inner.scan_keys(prefix, after, limit).await
    }
}

#[cfg(feature = "memory-backend")]
// A backend whose batch delete fails before touching anything.
struct FailingDeleteMany<K>(K);

#[cfg(feature = "memory-backend")]
impl<K: KvStore> KvStore for FailingDeleteMany<K> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        self.0.get(key).await
    }
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, KvError> {
        self.0.get_many(keys).await
    }
    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        self.0.put(key, value).await
    }
    async fn put_many(&self, entries: Vec<(String, Vec<u8>)>) -> Result<(), KvError> {
        self.0.put_many(entries).await
    }
    async fn delete(&self, key: &str) -> Result<(), KvError> {
        self.0.delete(key).await
    }
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the trait method is async; this one only fails"
    )]
    async fn delete_many(&self, _keys: &[String]) -> Result<(), KvError> {
        Err(KvError::Backend("batch refused".into()))
    }
    async fn exists(&self, key: &str) -> Result<bool, KvError> {
        self.0.exists(key).await
    }
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
        self.0.scan_prefix(prefix).await
    }
    async fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, KvError> {
        self.0.scan_keys(prefix, after, limit).await
    }
}

// Every call yields first, so a join! can interleave a put between the steps of a
// delete_many: only the write lock keeps them apart.
#[cfg(feature = "memory-backend")]
struct Yielding<K>(K);

#[cfg(feature = "memory-backend")]
impl<K: KvStore> KvStore for Yielding<K> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        tokio::task::yield_now().await;
        self.0.get(key).await
    }
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, KvError> {
        tokio::task::yield_now().await;
        self.0.get_many(keys).await
    }
    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        tokio::task::yield_now().await;
        self.0.put(key, value).await
    }
    async fn put_many(&self, entries: Vec<(String, Vec<u8>)>) -> Result<(), KvError> {
        tokio::task::yield_now().await;
        self.0.put_many(entries).await
    }
    async fn delete(&self, key: &str) -> Result<(), KvError> {
        tokio::task::yield_now().await;
        self.0.delete(key).await
    }
    async fn delete_many(&self, keys: &[String]) -> Result<(), KvError> {
        tokio::task::yield_now().await;
        self.0.delete_many(keys).await
    }
    async fn exists(&self, key: &str) -> Result<bool, KvError> {
        tokio::task::yield_now().await;
        self.0.exists(key).await
    }
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
        tokio::task::yield_now().await;
        self.0.scan_prefix(prefix).await
    }
    async fn scan_keys(
        &self,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, KvError> {
        tokio::task::yield_now().await;
        self.0.scan_keys(prefix, after, limit).await
    }
}

// Seeds `body` as held and old: its time is one second after the epoch.
async fn seed_old<K: KvStore>(s: &KvContentStore<K>, body: &[u8]) -> ContentHash {
    let hash = s.put(body).await.unwrap();
    s.inner()
        .put(
            &format!("content-at:{hash}"),
            1000_u64.to_be_bytes().to_vec(),
        )
        .await
        .unwrap();
    hash
}

// A put and a delete_many of an old hash, spawned together: whatever the order, the
// put issued in the round leaves the bytes present (put first: kept; delete first:
// recreated).
async fn a_put_racing_a_delete_many_always_wins<K: KvStore + 'static>(s: Arc<KvContentStore<K>>) {
    let body: &'static [u8] = b"race put against delete_many";
    for round in 0..200 {
        let hash = seed_old(&s, body).await;
        let (a, b) = (Arc::clone(&s), Arc::clone(&s));
        let delete = tokio::spawn(async move { a.delete_many(&[hash]).await });
        let put = tokio::spawn(async move { b.put(body).await });
        put.await.unwrap().unwrap();
        delete.await.unwrap().unwrap();
        assert_eq!(
            s.get(hash).await.unwrap(),
            Some(body.to_vec()),
            "round {round}"
        );
        assert!(time_key_present(&s, hash).await, "round {round}");
    }
}

// The same race on one thread, with suspension points between the backend calls.
#[cfg(feature = "memory-backend")]
async fn a_put_joined_with_a_delete_many_always_wins<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"join put against delete_many";
    for round in 0..200 {
        let hash = seed_old(s, body).await;
        let asked = [hash];
        let (delete, put) = tokio::join!(s.delete_many(&asked), s.put(body));
        delete.unwrap();
        put.unwrap();
        assert_eq!(
            s.get(hash).await.unwrap(),
            Some(body.to_vec()),
            "round {round}"
        );
        assert!(time_key_present(s, hash).await, "round {round}");
    }
}

fn opts(grace: Duration) -> ContentOptions {
    ContentOptions { grace }
}

macro_rules! backends {
    ($case:ident, $grace:expr, $memory:ident, $rocksdb:ident) => {
        #[cfg(feature = "memory-backend")]
        #[tokio::test]
        async fn $memory() {
            $case(&KvContentStore::with_options(
                MemoryStore::new(),
                opts($grace),
            ))
            .await;
        }

        #[cfg(feature = "rocksdb-backend")]
        #[tokio::test]
        async fn $rocksdb() {
            let dir = tempfile::tempdir().unwrap();
            let store = RocksDbStore::open(dir.path()).unwrap();
            $case(&KvContentStore::with_options(store, opts($grace))).await;
        }
    };
}

backends!(
    put_then_delete_within_grace_keeps_the_bytes,
    HOUR,
    memory_put_then_delete_within_grace_keeps_the_bytes,
    rocksdb_put_then_delete_within_grace_keeps_the_bytes
);
backends!(
    delete_many_of_an_absent_hash_is_absent_not_kept,
    HOUR,
    memory_delete_many_of_an_absent_hash_is_absent_not_kept,
    rocksdb_delete_many_of_an_absent_hash_is_absent_not_kept
);
backends!(
    delete_then_put_leaves_the_bytes,
    HOUR,
    memory_delete_then_put_leaves_the_bytes,
    rocksdb_delete_then_put_leaves_the_bytes
);
backends!(
    put_delete_put_leaves_the_bytes,
    HOUR,
    memory_put_delete_put_leaves_the_bytes,
    rocksdb_put_delete_put_leaves_the_bytes
);
backends!(
    force_ignores_the_grace_and_lists_only_the_destroyed,
    HOUR,
    memory_force_ignores_the_grace_and_lists_only_the_destroyed,
    rocksdb_force_ignores_the_grace_and_lists_only_the_destroyed
);
backends!(
    duplicates_in_the_ask_are_answered_once,
    HOUR,
    memory_duplicates_in_the_ask_are_answered_once,
    rocksdb_duplicates_in_the_ask_are_answered_once
);
backends!(
    a_blob_without_a_time_is_older_than_the_grace,
    HOUR,
    memory_a_blob_without_a_time_is_older_than_the_grace,
    rocksdb_a_blob_without_a_time_is_older_than_the_grace
);
backends!(
    after_the_grace_the_bytes_are_destroyed_and_listed,
    SHORT,
    memory_after_the_grace_the_bytes_are_destroyed_and_listed,
    rocksdb_after_the_grace_the_bytes_are_destroyed_and_listed
);
backends!(
    a_mixed_ask_splits_destroyed_kept_and_absent,
    SHORT,
    memory_a_mixed_ask_splits_destroyed_kept_and_absent,
    rocksdb_a_mixed_ask_splits_destroyed_kept_and_absent
);
backends!(
    a_re_put_after_the_grace_keeps_the_hash_again,
    SHORT,
    memory_a_re_put_after_the_grace_keeps_the_hash_again,
    rocksdb_a_re_put_after_the_grace_keeps_the_hash_again
);
backends!(
    a_put_after_a_destroying_delete_recreates,
    SHORT,
    memory_a_put_after_a_destroying_delete_recreates,
    rocksdb_a_put_after_a_destroying_delete_recreates
);

#[cfg(feature = "memory-backend")]
#[tokio::test]
async fn memory_delete_many_of_1000_hashes_is_one_backend_batch() {
    let s = KvContentStore::with_options(Counting::new(MemoryStore::new()), opts(Duration::ZERO));
    delete_many_of_1000_hashes_is_one_backend_batch(&s).await;
}

#[cfg(feature = "memory-backend")]
#[tokio::test]
async fn memory_a_failing_batch_leaves_no_partial_state() {
    let s =
        KvContentStore::with_options(FailingDeleteMany(MemoryStore::new()), opts(Duration::ZERO));
    a_failing_batch_leaves_no_partial_state(&s).await;
}

#[cfg(feature = "memory-backend")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn memory_a_put_racing_a_delete_many_always_wins() {
    let s = KvContentStore::with_options(Yielding(MemoryStore::new()), opts(HOUR));
    a_put_racing_a_delete_many_always_wins(Arc::new(s)).await;
}

#[cfg(feature = "memory-backend")]
#[tokio::test]
async fn memory_a_put_joined_with_a_delete_many_always_wins() {
    let s = KvContentStore::with_options(Yielding(MemoryStore::new()), opts(HOUR));
    a_put_joined_with_a_delete_many_always_wins(&s).await;
}

#[cfg(feature = "rocksdb-backend")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rocksdb_a_put_racing_a_delete_many_always_wins() {
    let dir = tempfile::tempdir().unwrap();
    let s = KvContentStore::with_options(RocksDbStore::open(dir.path()).unwrap(), opts(HOUR));
    a_put_racing_a_delete_many_always_wins(Arc::new(s)).await;
}

#[cfg(feature = "rocksdb-backend")]
#[tokio::test]
async fn rocksdb_delete_many_is_durable_across_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let all = many(50, "durable");
    let slices: Vec<&[u8]> = all.iter().map(Vec::as_slice).collect();
    let hashes = {
        let s = KvContentStore::new(RocksDbStore::open(dir.path()).unwrap());
        let hashes = s.put_many(&slices).await.unwrap();
        assert_eq!(s.delete_many_force(&hashes).await.unwrap(), hashes);
        hashes
    };
    let s = KvContentStore::new(RocksDbStore::open(dir.path()).unwrap());
    assert_eq!(s.get_many(&hashes).await.unwrap(), vec![None; 50]);
}

#[cfg(feature = "rocksdb-backend")]
#[tokio::test]
async fn rocksdb_sync_writes_option_roundtrips_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let options = RocksDbOptions { sync_writes: true };
    let hash = {
        let s = KvContentStore::new(RocksDbStore::open_with(dir.path(), options).unwrap());
        let all = many(3, "synced");
        let slices: Vec<&[u8]> = all.iter().map(Vec::as_slice).collect();
        let hashes = s.put_many(&slices).await.unwrap();
        let hash = s.put(b"synced single").await.unwrap();
        assert_eq!(
            s.delete_many_force(&hashes[..1]).await.unwrap(),
            hashes[..1]
        );
        hash
    };
    let s = KvContentStore::new(RocksDbStore::open(dir.path()).unwrap());
    assert_eq!(s.get(hash).await.unwrap(), Some(b"synced single".to_vec()));
    assert_eq!(s.list(None, 10).await.unwrap().len(), 3);
}

#[cfg(feature = "rocksdb-backend")]
#[test]
fn the_defaults_are_one_hour_and_no_sync() {
    assert_eq!(ContentOptions::default().grace, HOUR);
    assert!(!RocksDbOptions::default().sync_writes);
}

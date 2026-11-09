//! Shared CAS conformance cases for the memory and `RocksDB` backends.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "memory-backend")]
use kv_store::MemoryStore;
#[cfg(feature = "rocksdb-backend")]
use kv_store::RocksDbStore;
use kv_store::{ContentHash, ContentStore, KvContentStore, KvError, KvStore};

async fn put_returns_sha256_of_body<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"the store computes the hash";
    assert_eq!(s.put(body).await.unwrap(), ContentHash::from_bytes(body));
}

async fn put_then_get_roundtrip<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"hello anatta";
    let hash = s.put(body).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), Some(body.to_vec()));
}

async fn put_twice_is_one_entry<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"one entry";
    let hash = s.put(body).await.unwrap();
    assert_eq!(s.put(body).await.unwrap(), hash);
    let entries = s.list(None, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, hash);
}

async fn put_empty_body<K: KvStore>(s: &KvContentStore<K>) {
    let hash = s.put(b"").await.unwrap();
    assert_eq!(hash, ContentHash::from_bytes(b""));
    assert_eq!(s.get(hash).await.unwrap(), Some(Vec::new()));
}

async fn get_absent_is_none<K: KvStore>(s: &KvContentStore<K>) {
    assert_eq!(
        s.get(ContentHash::from_bytes(b"absent")).await.unwrap(),
        None
    );
}

async fn has_false_before_true_after<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"present after put";
    let hash = ContentHash::from_bytes(body);
    assert!(!s.has(hash).await.unwrap());
    assert_eq!(s.put(body).await.unwrap(), hash);
    assert!(s.has(hash).await.unwrap());
}

async fn non_utf8_roundtrip<K: KvStore>(s: &KvContentStore<K>) {
    let body = [0xff, 0xfe, 0x00, 0x80, 0x01];
    let hash = s.put(&body).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), Some(body.to_vec()));
}

async fn large_body_2mib_roundtrip<K: KvStore>(s: &KvContentStore<K>) {
    let body = vec![0xab; 2 * 1024 * 1024];
    let hash = s.put(&body).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), Some(body));
}

async fn stored_bytes_are_compressed<K: KvStore>(s: &KvContentStore<K>) {
    let body = vec![0x42; 64 * 1024];
    let hash = s.put(&body).await.unwrap();
    let raw = s
        .inner()
        .get(&format!("content:{hash}"))
        .await
        .unwrap()
        .unwrap();
    assert!(raw.len() < body.len());
}

async fn concurrent_puts_same_body<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"eight concurrent puts";
    let (first, second, third, fourth, fifth, sixth, seventh, eighth) = tokio::join!(
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
    );
    let expected = ContentHash::from_bytes(body);
    for result in [first, second, third, fourth, fifth, sixth, seventh, eighth] {
        assert_eq!(result.unwrap(), expected);
    }
    let entries = s.list(None, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, expected);
}

#[cfg(feature = "memory-backend")]
async fn racing_put_and_delete_keep_time_with_blob<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"race put against delete";
    let hash = ContentHash::from_bytes(body);
    for round in 0..200 {
        let (put, delete) = tokio::join!(s.put(body), s.delete(hash));
        assert_eq!(put.unwrap(), hash);
        delete.unwrap();
        let blob = s.inner().exists(&format!("content:{hash}")).await.unwrap();
        let time = s
            .inner()
            .exists(&format!("content-at:{hash}"))
            .await
            .unwrap();
        assert_eq!(blob, time, "blob/time presence differs in round {round}");
    }
}

#[cfg(feature = "memory-backend")]
async fn racing_puts_keep_one_time<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"race eight first puts";
    let (first, second, third, fourth, fifth, sixth, seventh, eighth) = tokio::join!(
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
        s.put(body),
    );
    let hash = ContentHash::from_bytes(body);
    for put in [first, second, third, fourth, fifth, sixth, seventh, eighth] {
        assert_eq!(put.unwrap(), hash);
    }
    let raw = s
        .inner()
        .get(&format!("content-at:{hash}"))
        .await
        .unwrap()
        .unwrap();
    let millis = u64::from_be_bytes(raw.try_into().unwrap());
    assert_eq!(
        s.list(None, 10).await.unwrap(),
        vec![(hash, UNIX_EPOCH + Duration::from_millis(millis))]
    );
}

async fn distinct_bodies_distinct_hashes<K: KvStore>(s: &KvContentStore<K>) {
    let first = s.put(b"first").await.unwrap();
    let second = s.put(b"second").await.unwrap();
    assert_ne!(first, second);
}

async fn get_detects_corruption<K: KvStore>(s: &KvContentStore<K>) {
    let hash = s.put(b"original").await.unwrap();
    let corrupted = zstd::encode_all(&b"another body"[..], 3).unwrap();
    s.inner()
        .put(&format!("content:{hash}"), corrupted)
        .await
        .unwrap();
    match s.get(hash).await {
        Err(KvError::HashMismatch(message)) => assert!(message.contains(&hash.as_hex())),
        result => panic!("expected a hash mismatch, got {result:?}"),
    }
}

async fn delete_then_absent<K: KvStore>(s: &KvContentStore<K>) {
    let hash = s.put(b"delete me").await.unwrap();
    s.delete(hash).await.unwrap();
    assert_eq!(s.get(hash).await.unwrap(), None);
    assert!(!s.has(hash).await.unwrap());
    assert_eq!(s.list(None, 10).await.unwrap().len(), 0);
    s.delete(hash).await.unwrap();
}

async fn list_pages_in_hex_order<K: KvStore>(s: &KvContentStore<K>) {
    let mut expected = Vec::new();
    for i in 0..25 {
        expected.push(s.put(format!("blob {i}").as_bytes()).await.unwrap());
    }
    expected.sort_by_key(ContentHash::as_hex);
    let mut after = None;
    let mut found = Vec::new();
    for size in [10, 10, 5, 0] {
        let page = s.list(after, 10).await.unwrap();
        assert_eq!(page.len(), size);
        after = page.last().map(|(hash, _)| *hash);
        found.extend(page.into_iter().map(|(hash, _)| hash));
    }
    assert!(
        found
            .windows(2)
            .all(|pair| pair[0].as_hex() < pair[1].as_hex())
    );
    assert_eq!(found, expected);
}

async fn list_ignores_other_prefixes<K: KvStore>(s: &KvContentStore<K>) {
    for key in ["cont:x", "contents", "other:1"] {
        s.inner().put(key, vec![42]).await.unwrap();
    }
    let hash = s.put(b"listed content").await.unwrap();
    let entries = s.list(None, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, hash);
}

async fn list_skips_foreign_content_keys<K: KvStore>(s: &KvContentStore<K>) {
    let mut expected = Vec::new();
    for i in 0..12 {
        expected.push(
            s.put(format!("foreign-key fixture {i}").as_bytes())
                .await
                .unwrap(),
        );
    }
    expected.sort_by_key(ContentHash::as_hex);
    for key in [
        "content:XYZ".to_owned(),
        format!("content:{}", "ABCDEF01".repeat(8)),
        format!("content:{}", &expected[4].as_hex()[..63]),
    ] {
        s.inner().put(&key, vec![42]).await.unwrap();
    }
    let mut found = Vec::new();
    let mut after = None;
    for size in [5, 5, 2] {
        let page = s.list(after, 5).await.unwrap();
        assert_eq!(page.len(), size);
        after = page.last().map(|(hash, _)| *hash);
        found.extend(page.into_iter().map(|(hash, _)| hash));
    }
    assert_eq!(found, expected);
    assert_eq!(s.list(after, 5).await.unwrap().len(), 0);
    assert_eq!(s.list(None, 0).await.unwrap().len(), 0);
}

async fn list_reports_stored_at<K: KvStore>(s: &KvContentStore<K>) {
    let before = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let before = UNIX_EPOCH + Duration::from_millis(u64::try_from(before.as_millis()).unwrap());
    let hash = s.put(b"timestamped content").await.unwrap();
    let after = SystemTime::now();
    let entries = s.list(None, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, hash);
    assert!(entries[0].1 >= before);
    assert!(entries[0].1 <= after);
}

async fn a_re_put_of_held_bytes_refreshes_stored_at_and_keeps_the_bytes<K: KvStore>(
    s: &KvContentStore<K>,
) {
    let body = b"refresh the time, keep the bytes";
    let hash = s.put(body).await.unwrap();
    s.inner()
        .put(
            &format!("content-at:{hash}"),
            1000u64.to_be_bytes().to_vec(),
        )
        .await
        .unwrap();
    let before = SystemTime::now() - Duration::from_secs(60);
    assert_eq!(s.put(body).await.unwrap(), hash);
    let entries = s.list(None, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, hash);
    assert!(entries[0].1 > UNIX_EPOCH + Duration::from_millis(1000));
    assert!(entries[0].1 >= before);
    assert_eq!(s.get(hash).await.unwrap(), Some(body.to_vec()));
    assert!(s.has(hash).await.unwrap());
}

async fn a_re_put_of_a_blob_without_a_time_records_one<K: KvStore>(s: &KvContentStore<K>) {
    let body = b"legacy blob re-imported";
    let hash = s.put(body).await.unwrap();
    s.inner()
        .delete(&format!("content-at:{hash}"))
        .await
        .unwrap();
    s.put(body).await.unwrap();
    let entries = s.list(None, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].1 > UNIX_EPOCH);
}

async fn list_without_time_is_epoch<K: KvStore>(s: &KvContentStore<K>) {
    let hash = s.put(b"legacy content without a time").await.unwrap();
    s.inner()
        .delete(&format!("content-at:{hash}"))
        .await
        .unwrap();
    assert_eq!(s.list(None, 10).await.unwrap(), vec![(hash, UNIX_EPOCH)]);
}

async fn delete_removes_stored_at<K: KvStore>(s: &KvContentStore<K>) {
    let hash = s.put(b"delete time too").await.unwrap();
    let key = format!("content-at:{hash}");
    assert!(s.inner().exists(&key).await.unwrap());
    s.delete(hash).await.unwrap();
    assert!(!s.inner().exists(&key).await.unwrap());
}

// The real backends complete each call without yielding. Introduce suspension
// points so join! can expose races between the CAS's multiple backend calls.
#[cfg(feature = "memory-backend")]
struct Yielding<K>(K);

#[cfg(feature = "memory-backend")]
impl<K: KvStore> KvStore for Yielding<K> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        tokio::task::yield_now().await;
        self.0.get(key).await
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        tokio::task::yield_now().await;
        self.0.put(key, value).await
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        tokio::task::yield_now().await;
        self.0.delete(key).await
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

#[cfg(feature = "memory-backend")]
#[tokio::test]
async fn memory_racing_put_and_delete_keep_time_with_blob() {
    racing_put_and_delete_keep_time_with_blob(&KvContentStore::new(Yielding(MemoryStore::new())))
        .await;
}

#[cfg(feature = "memory-backend")]
#[tokio::test]
async fn memory_racing_puts_keep_one_time() {
    racing_puts_keep_one_time(&KvContentStore::new(Yielding(MemoryStore::new()))).await;
}

macro_rules! both_backends {
    ($case:ident, $memory:ident, $rocksdb:ident) => {
        #[cfg(feature = "memory-backend")]
        #[tokio::test]
        async fn $memory() {
            $case(&KvContentStore::new(MemoryStore::new())).await;
        }

        #[cfg(feature = "rocksdb-backend")]
        #[tokio::test]
        async fn $rocksdb() {
            let dir = tempfile::tempdir().unwrap();
            let store = RocksDbStore::open(dir.path()).unwrap();
            $case(&KvContentStore::new(store)).await;
        }
    };
}

both_backends!(
    put_returns_sha256_of_body,
    memory_put_returns_sha256_of_body,
    rocksdb_put_returns_sha256_of_body
);
both_backends!(
    put_then_get_roundtrip,
    memory_put_then_get_roundtrip,
    rocksdb_put_then_get_roundtrip
);
both_backends!(
    put_twice_is_one_entry,
    memory_put_twice_is_one_entry,
    rocksdb_put_twice_is_one_entry
);
both_backends!(
    put_empty_body,
    memory_put_empty_body,
    rocksdb_put_empty_body
);
both_backends!(
    get_absent_is_none,
    memory_get_absent_is_none,
    rocksdb_get_absent_is_none
);
both_backends!(
    has_false_before_true_after,
    memory_has_false_before_true_after,
    rocksdb_has_false_before_true_after
);
both_backends!(
    non_utf8_roundtrip,
    memory_non_utf8_roundtrip,
    rocksdb_non_utf8_roundtrip
);
both_backends!(
    large_body_2mib_roundtrip,
    memory_large_body_2mib_roundtrip,
    rocksdb_large_body_2mib_roundtrip
);
both_backends!(
    stored_bytes_are_compressed,
    memory_stored_bytes_are_compressed,
    rocksdb_stored_bytes_are_compressed
);
both_backends!(
    concurrent_puts_same_body,
    memory_concurrent_puts_same_body,
    rocksdb_concurrent_puts_same_body
);
both_backends!(
    distinct_bodies_distinct_hashes,
    memory_distinct_bodies_distinct_hashes,
    rocksdb_distinct_bodies_distinct_hashes
);
both_backends!(
    get_detects_corruption,
    memory_get_detects_corruption,
    rocksdb_get_detects_corruption
);
both_backends!(
    delete_then_absent,
    memory_delete_then_absent,
    rocksdb_delete_then_absent
);
both_backends!(
    list_pages_in_hex_order,
    memory_list_pages_in_hex_order,
    rocksdb_list_pages_in_hex_order
);
both_backends!(
    list_ignores_other_prefixes,
    memory_list_ignores_other_prefixes,
    rocksdb_list_ignores_other_prefixes
);
both_backends!(
    list_skips_foreign_content_keys,
    memory_list_skips_foreign_content_keys,
    rocksdb_list_skips_foreign_content_keys
);
both_backends!(
    list_reports_stored_at,
    memory_list_reports_stored_at,
    rocksdb_list_reports_stored_at
);
both_backends!(
    a_re_put_of_held_bytes_refreshes_stored_at_and_keeps_the_bytes,
    memory_a_re_put_of_held_bytes_refreshes_stored_at_and_keeps_the_bytes,
    rocksdb_a_re_put_of_held_bytes_refreshes_stored_at_and_keeps_the_bytes
);
both_backends!(
    a_re_put_of_a_blob_without_a_time_records_one,
    memory_a_re_put_of_a_blob_without_a_time_records_one,
    rocksdb_a_re_put_of_a_blob_without_a_time_records_one
);
both_backends!(
    list_without_time_is_epoch,
    memory_list_without_time_is_epoch,
    rocksdb_list_without_time_is_epoch
);
both_backends!(
    delete_removes_stored_at,
    memory_delete_removes_stored_at,
    rocksdb_delete_removes_stored_at
);

#[cfg(feature = "rocksdb-backend")]
#[tokio::test]
async fn rocksdb_reopen_keeps_blobs() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"persist across reopen";
    let (hash, entries) = {
        let store = KvContentStore::new(RocksDbStore::open(dir.path()).unwrap());
        let hash = store.put(body).await.unwrap();
        let entries = store.list(None, 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, hash);
        assert!(entries[0].1 > UNIX_EPOCH);
        (hash, entries)
    };
    let store = KvContentStore::new(RocksDbStore::open(dir.path()).unwrap());
    assert_eq!(store.get(hash).await.unwrap(), Some(body.to_vec()));
    assert_eq!(store.list(None, 10).await.unwrap(), entries);
}

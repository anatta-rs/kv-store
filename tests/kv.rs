//! Shared key-value conformance cases for the memory and `RocksDB` backends.

use kv_store::KvStore;
#[cfg(feature = "memory-backend")]
use kv_store::MemoryStore;
#[cfg(feature = "rocksdb-backend")]
use kv_store::RocksDbStore;

async fn scan_prefix_sorted<K: KvStore>(k: &K) {
    for (key, value) in [("b:2", 4), ("a:3", 3), ("a:1", 1), ("b:1", 5), ("a:2", 2)] {
        k.put(key, vec![value]).await.unwrap();
    }

    assert_eq!(
        k.scan_prefix("a:").await.unwrap(),
        vec![
            ("a:1".into(), vec![1]),
            ("a:2".into(), vec![2]),
            ("a:3".into(), vec![3]),
        ]
    );
}

async fn scan_keys_after_and_limit<K: KvStore>(k: &K) {
    for key in ["q:1", "p:5", "p:3", "o:1", "p:1", "p:4", "p:2"] {
        k.put(key, vec![42]).await.unwrap();
    }

    assert_eq!(k.scan_keys("p:", None, 2).await.unwrap(), ["p:1", "p:2"]);
    assert_eq!(
        k.scan_keys("p:", Some("p:2"), 2).await.unwrap(),
        ["p:3", "p:4"]
    );
    assert_eq!(k.scan_keys("p:", Some("p:4"), 2).await.unwrap(), ["p:5"]);
    for (after, limit) in [(Some("p:5"), 2), (None, 0), (Some("q:1"), 2)] {
        assert_eq!(k.scan_keys("p:", after, limit).await.unwrap().len(), 0);
    }
    assert_eq!(
        k.scan_keys("p:", Some("o:1"), 2).await.unwrap(),
        ["p:1", "p:2"]
    );
    assert_eq!(
        k.scan_keys("p:", Some("p:25"), 2).await.unwrap(),
        ["p:3", "p:4"]
    );
    assert_eq!(
        k.scan_keys("p:", None, usize::MAX).await.unwrap(),
        ["p:1", "p:2", "p:3", "p:4", "p:5"]
    );
    assert_eq!(
        <&K as KvStore>::scan_keys(&k, "p:", None, 2).await.unwrap(),
        ["p:1", "p:2"]
    );
}

async fn delete_missing_is_ok<K: KvStore>(k: &K) {
    assert!(!k.exists("missing").await.unwrap());
    k.delete("missing").await.unwrap();
    assert!(!k.exists("missing").await.unwrap());
}

macro_rules! both_backends {
    ($case:ident, $memory:ident, $rocksdb:ident) => {
        #[cfg(feature = "memory-backend")]
        #[tokio::test]
        async fn $memory() {
            $case(&MemoryStore::new()).await;
        }

        #[cfg(feature = "rocksdb-backend")]
        #[tokio::test]
        async fn $rocksdb() {
            let dir = tempfile::tempdir().unwrap();
            let store = RocksDbStore::open(dir.path()).unwrap();
            $case(&store).await;
        }
    };
}

both_backends!(
    scan_prefix_sorted,
    memory_scan_prefix_sorted,
    rocksdb_scan_prefix_sorted
);
both_backends!(
    scan_keys_after_and_limit,
    memory_scan_keys_after_and_limit,
    rocksdb_scan_keys_after_and_limit
);
both_backends!(
    delete_missing_is_ok,
    memory_delete_missing_is_ok,
    rocksdb_delete_missing_is_ok
);

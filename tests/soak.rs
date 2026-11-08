//! Opt-in 1 GiB soak against the compressed, content-addressed `RocksDB` store.
#![cfg(feature = "rocksdb-backend")]

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kv_store::{ContentHash, ContentStore, KvContentStore, RocksDbStore};

const BODY_COUNT: u32 = 1024;
const BODY_BYTES: usize = 1024 * 1024;
const PAGE_SIZE: usize = 100;

#[ignore = "writes and reads 1 GiB; run explicitly with --ignored --nocapture"]
#[tokio::test]
async fn soak_1gib_put_get_rocksdb() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let store = KvContentStore::new(RocksDbStore::open(dir.path()).unwrap());
    let started_at = start_time();

    let started = Instant::now();
    let expected = put_all(&store).await;
    print_throughput("put", started.elapsed());

    let started = Instant::now();
    get_all(&store).await;
    print_throughput("get", started.elapsed());

    let started = Instant::now();
    list_all(&store, started_at, &expected).await;
    println!("list elapsed: {:.3}s", started.elapsed().as_secs_f64());

    // Close the database before walking its files so background work has stopped.
    drop(store);
    let bytes = directory_bytes(dir.path());
    let mib = 1024 * 1024;
    println!(
        "on-disk size: {}.{:02} MiB",
        bytes / mib,
        (bytes % mib) * 100 / mib
    );
}

fn start_time() -> SystemTime {
    // D22 persists milliseconds; compare the start at the same precision.
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    UNIX_EPOCH + Duration::from_millis(u64::try_from(elapsed.as_millis()).unwrap())
}

fn body(index: u32) -> Vec<u8> {
    // SplitMix64: a distinct seed per body, with no compressible repeating pattern.
    let mut state = u64::from(index);
    let mut bytes = vec![0; BODY_BYTES];
    for chunk in bytes.as_chunks_mut::<8>().0 {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

async fn put_all(store: &KvContentStore<RocksDbStore>) -> HashSet<ContentHash> {
    let mut hashes = HashSet::new();
    for index in 0..BODY_COUNT {
        let body = body(index);
        let hash = store.put(&body).await.unwrap();
        assert_eq!(hash, ContentHash::from_bytes(&body), "put body {index}");
        assert!(hashes.insert(hash), "duplicate body {index}");
    }
    hashes
}

async fn get_all(store: &KvContentStore<RocksDbStore>) {
    for index in 0..BODY_COUNT {
        let expected = ContentHash::from_bytes(&body(index));
        let actual = store.get(expected).await.unwrap().unwrap();
        assert_eq!(actual.len(), BODY_BYTES, "get body {index} length");
        assert_eq!(
            ContentHash::from_bytes(&actual),
            expected,
            "get body {index}"
        );
    }
}

async fn list_all(
    store: &KvContentStore<RocksDbStore>,
    started_at: SystemTime,
    expected: &HashSet<ContentHash>,
) {
    let mut after = None;
    let mut found = HashSet::new();
    loop {
        let page = store.list(after, PAGE_SIZE).await.unwrap();
        assert!(page.len() <= PAGE_SIZE);
        if page.is_empty() {
            break;
        }
        for &(hash, stored_at) in &page {
            assert!(stored_at >= started_at, "old timestamp for {hash}");
            assert!(expected.contains(&hash), "unexpected hash {hash}");
            assert!(found.insert(hash), "duplicate listed hash {hash}");
        }
        after = page.last().map(|(hash, _)| *hash);
    }
    assert_eq!(found.len(), usize::try_from(BODY_COUNT).unwrap());
    assert_eq!(&found, expected);
}

fn print_throughput(operation: &str, elapsed: Duration) {
    let seconds = elapsed.as_secs_f64();
    let mib = f64::from(BODY_COUNT); // Each body is exactly 1 MiB.
    println!(
        "{operation} elapsed: {seconds:.3}s, {:.2} MiB/s",
        mib / seconds
    );
}

fn directory_bytes(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                directory_bytes(&entry.path())
            } else if metadata.is_file() {
                metadata.len()
            } else {
                0
            }
        })
        .sum()
}

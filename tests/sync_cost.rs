//! Measures the cost of WAL fsync on 10,000 single puts (task 004).
//! Run: `cargo test --release --test sync_cost -- --ignored --nocapture`.

#![cfg(feature = "rocksdb-backend")]

use std::time::Instant;

use kv_store::{KvStore, RocksDbOptions, RocksDbStore};

async fn puts_per_second(sync_writes: bool, n: u32) -> f64 {
    let dir = tempfile::tempdir().unwrap();
    let store = RocksDbStore::open_with(dir.path(), RocksDbOptions { sync_writes }).unwrap();
    let start = Instant::now();
    for i in 0..n {
        store.put(&format!("k{i:06}"), vec![7; 256]).await.unwrap();
    }
    f64::from(n) / start.elapsed().as_secs_f64()
}

#[tokio::test]
#[ignore = "a measurement, not a conformance case"]
async fn measure_sync_cost_on_10000_puts() {
    let off = puts_per_second(false, 10_000).await;
    let on = puts_per_second(true, 10_000).await;
    println!(
        "sync off: {off:.0} puts/s; sync on: {on:.0} puts/s; ratio {:.1}x",
        off / on
    );
    assert!(off > 0.0 && on > 0.0);
}

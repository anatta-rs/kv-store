# 004 — A put always wins: delete_many never destroys a recent hash

- status: in progress (plan 2026-11-11; drafting agent under the orchestrator; wording the user's, 2026-11-11)
- rank: next

## Summary

GitSense 076 deletes content bytes after the graph nodes that referenced them, while another sync may put the same hash at the same time (a text shared by two repositories). Nothing orders the two: the engine's mutations are outside the CAS, and a RocksDB `WriteBatch` removes the interleaving inside the store but not the order. Target: the store's semantics make both orders correct without a lock between callers:

- **put creates and always wins.** A put recreates a destroyed hash, refreshes `last_put` (D21 kept the first time; it becomes the last), and cancels any pending delete of that hash.
- **delete destroys, except a recent hash.** `delete_many(hashes) -> destroyed: Vec<hash>` is one `WriteBatch`; a hash whose `last_put` is younger than the store's grace G (a setting of `open`, default one hour) is left in place and absent from `destroyed` (put → delete = cancelled); a put after a delete loses nothing (delete → put = recreated). `delete_many_force` ignores G for the integrity repair.
- The read of `last_put` and the batch write are atomic against a concurrent put (a store-level write lock in the rocksdb and memory backends).
- Durability knob: `put_many` with `sync: true` (WAL fsync per batch) behind an option of `open`, default off, measured (puts per second with and without), so GitSense can choose it for content bytes.

Red first: the two-task interleaving test (one `delete_many`, one `put` of a shared hash) in both orders, the hash present at the end whenever a put came within G; a hash older than G destroyed and listed; a failing batch leaves no partial state; the Breaking test on `last_put` (CHANGELOG under Breaking). The caller's side (GitSense): the gap between a put and the mutation that references it stays under G, tested (per-batch commits, D076-25).

## Plan (2026-11-11, rulings of the orchestrator applied)

- D004-1 Invariant: a put and a delete_many never interleave: the read of `content-at` and the batch run under the content store's write lock, so a put issued after a delete's read waits and re-creates (a put always wins). Test: a put racing a delete_many (spawned, both under the lock) ends with the bytes present whatever the order.
- D004-2 `last_put` is the existing `content-at:<hex>` sibling key (8-byte BE millis, refreshed by every put and re-put since 0.2.1): no new storage, no column family.
- D004-2 (cost) `delete_many` costs one `get_many` of the time keys, at most one `exists` per hash older than G or without a time, and one batch. A time younger than G with no body is KEPT without a body read (a put in flight or a crash between its two writes: the next put completes it); an old orphan time is swept in the same batch and listed absent. The "1,000 hashes = one batch" test counts batches, not reads.
- D004-3 API: `KvStore::delete_many(&[String])` (default loops `delete`; memory in one lock; rocksdb ONE `WriteBatch`). `ContentStore::delete_many(&[ContentHash]) -> DeleteOutcome { destroyed, kept, absent }` and `delete_many_force(&[ContentHash]) -> Vec<ContentHash>` (destroyed). Under the write mutex: one `get_many` of the time keys (and bodies, to tell absent), split by `now - last_put >= G`, ONE `inner.delete_many` of body+time keys of the destroyed.
- D004-4 Grace G: `KvContentStore::with_options(inner, ContentOptions { grace })`, `new` = one hour. WAL fsync: `RocksDbStore::open_with(path, RocksDbOptions { sync_writes })`, `open` = off; applies to every write. Cost measured on 10,000 puts, written here.
- D004-5 `ContentStore::delete(hash)` unchanged. A hash with no recorded time = epoch = older than G; duplicates in the ask answered once; an absent hash is in `absent` only.
- D004-6 Tests: G = 150 ms for "after G" (assert on elapsed time, not a sleep count), G = 1 h for "within G"; no clock trait.
- D004-7 Failing batch: a wrapper whose `delete_many` errs leaves everything; a counting wrapper proves 1,000 hashes = one backend `delete_many`.
- Breaking: two required `ContentStore` methods; CHANGELOG under Breaking; the version bump is the orchestrator's.

## Log

- 2026-11-11 drafting agent: plan written, ruled by the orchestrator (Q1-Q7 as proposed + D004-1 invariant + `absent`).
- 2026-11-11 drafting agent: red tests first (`tests/delete_many.rs`, `tests/kv.rs`, `tests/sync_cost.rs`: did not compile), then green: `KvStore::delete_many` (default loop; memory one lock; rocksdb one `WriteBatch`), `ContentStore::{delete_many, delete_many_force}`, `ContentOptions`, `DeleteOutcome`, `RocksDbOptions`/`open_with`. Mutant: removing the write lock in `delete_many` turns the joined and spawned race tests red (3 tests), restored.
- 2026-11-11 measured (D004-4), `cargo test --release --test sync_cost -- --ignored --nocapture`, 10,000 single puts of 256 bytes into a fresh RocksDB on an Apple M4 Max (macOS, APFS), three runs: sync off 420k / 490k / 550k puts/s; sync on 25k / 35k / 32k puts/s: a WAL fsync per write costs about 14x to 17x on single puts (about 30 to 40 microseconds per put). `put_many` pays one fsync per batch, so batching removes most of it. Default stays off.

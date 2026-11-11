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

## Plan (2026-11-11, awaiting the orchestrator's ruling, leans below)

- D004-1 `last_put` is the existing `content-at:<hex>` sibling key (8-byte BE millis, refreshed by every put and re-put since 0.2.1): no new storage, no new column family.
- D004-2 API: `KvStore::delete_many(&[String])` (default loops `delete`; memory in one lock; rocksdb ONE `WriteBatch`). `ContentStore::delete_many(&[ContentHash]) -> DeleteOutcome { destroyed, kept }` and `delete_many_force(&[ContentHash]) -> Vec<ContentHash>` (destroyed). Both under the existing write mutex: one `get_many` of the time keys, split by `now - last_put >= G`, ONE `inner.delete_many` of body+time keys of the destroyed. The mutex is what makes "read of `last_put` + batch" atomic against a concurrent put (put already takes it).
- D004-3 grace G on `KvContentStore::with_options(inner, ContentOptions { grace })`, `new` = one hour. WAL fsync: `RocksDbStore::open_with(path, RocksDbOptions { sync_writes })`, `open` = off; applies to every write of the store. Cost measured on 10,000 puts, written here.
- D004-4 `ContentStore::delete(hash)` unchanged (unconditional, one hash).
- D004-5 absent hash: in neither list; hash with no recorded time = epoch = older than G; duplicates answered once.
- D004-6 tests: tiny G + real sleep for "after G"; no injectable clock.
- D004-7 failing batch: a wrapper whose `delete_many` errs leaves everything; a counting wrapper proves 1,000 hashes = one backend `delete_many`.
- Breaking: two required `ContentStore` methods; CHANGELOG under Breaking; version bump not in this task.

## Log

- 2026-11-11 drafting agent: plan written; QUESTION sent to the orchestrator.

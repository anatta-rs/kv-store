# 004 — delete_many as one batch; a re-put refreshes stored_at; delete_stale

- status: proposed (2026-11-11, from GitSense 076 D076-28)
- rank: next

## Summary

GitSense 076 removes content bytes after the graph nodes that referenced them, while another sync may re-put the same hash (a text shared by two repositories): with per-key deletes the removal's deletes and the sync's puts interleave inside RocksDB, and even as whole batches nothing orders one before the other. Target, so the protocol is correct in both orders without a lock:
1. `ContentStore::delete_many(hashes)` as one `WriteBatch` (atomic, like `put_many`); `delete` stays for one hash.
2. A re-put of an existing hash sets `stored_at` to now (D21 kept the first time; the grace window needs the last put), with the CHANGELOG line under Breaking.
3. `ContentStore::delete_stale(hashes, older_than: Duration) -> deleted: Vec<hash>`: deletes, in one batch and atomically against concurrent puts (a store-level write lock around read-then-batch in the rocksdb backend, the same in memory), only the hashes whose `stored_at` is older than the window; returns what it deleted so the caller knows which bytes are gone. A hash a sync put within the window is never deleted by a removal.
4. Durability knob: `put_many` with `sync: true` (WAL fsync per batch) behind an option of `open`, default off, measured (puts per second with and without) so GitSense can choose it for content bytes.
Red first: an interleaving test with two tasks (one delete_stale, one put of a shared hash) in both orders, the hash present at the end whenever a put came within the window; the batch atomicity test (a failing delete_many leaves no partial state); the breaking re-put test.

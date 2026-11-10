# 004 — A put always wins: delete_many never destroys a recent hash

- status: proposed (2026-11-11, from GitSense 076 D076-29; wording the user's, 2026-11-11)
- rank: next

## Summary

GitSense 076 deletes content bytes after the graph nodes that referenced them, while another sync may put the same hash at the same time (a text shared by two repositories). Nothing orders the two: the engine's mutations are outside the CAS, and a RocksDB `WriteBatch` removes the interleaving inside the store but not the order. Target: the store's semantics make both orders correct without a lock between callers:

- **put creates and always wins.** A put recreates a destroyed hash, refreshes `last_put` (D21 kept the first time; it becomes the last), and cancels any pending delete of that hash.
- **delete destroys, except a recent hash.** `delete_many(hashes) -> destroyed: Vec<hash>` is one `WriteBatch`; a hash whose `last_put` is younger than the store's grace G (a setting of `open`, default one hour) is left in place and absent from `destroyed` (put → delete = cancelled); a put after a delete loses nothing (delete → put = recreated). `delete_many_force` ignores G for the integrity repair.
- The read of `last_put` and the batch write are atomic against a concurrent put (a store-level write lock in the rocksdb and memory backends).
- Durability knob: `put_many` with `sync: true` (WAL fsync per batch) behind an option of `open`, default off, measured (puts per second with and without), so GitSense can choose it for content bytes.

Red first: the two-task interleaving test (one `delete_many`, one `put` of a shared hash) in both orders, the hash present at the end whenever a put came within G; a hash older than G destroyed and listed; a failing batch leaves no partial state; the Breaking test on `last_put` (CHANGELOG under Breaking). The caller's side (GitSense): the gap between a put and the mutation that references it stays under G, tested (per-batch commits, D076-25).

# 003 Batched content reads and writes

status: in progress
level: standard: one crate, two ports gain batch methods, no format change
request: the user, 2026-11-10 (gitsense 076, D076-8): "cas need get_batches, éviter un get par blob".
repository: kv-store, branch `task/003-batched-content` from `e04334d`; not pushed (gitsense's orchestrator pushes)
depends: none; gitsense 076 lane A pins this checkout by `path` until a rev is pushed

## Summary

`ContentStore` gains `get_many(&[ContentHash]) -> Vec<Option<Vec<u8>>>` (one answer per asked hash, in the order asked, `None` in place for a missing one, every body verified as `get` does) and `put_many(&[&[u8]]) -> Vec<ContentHash>` (one hash per body, in order; a held body only refreshes its stored time, as `put`). `KvStore` gains `get_many(&[String])` and `put_many(Vec<(String, Vec<u8>)>)` with default bodies that loop (HTTP and other backends keep working); the memory backend answers in one lock, rocksdb in one `multi_get` and one `WriteBatch`. `KvContentStore` calls the backend once per batch: no `get` per hash.

## Lanes

| lane | red tests | mutant | hostile inputs |
|---|---|---|---|
| K | 40 hashes = one backend call (a call-counting `KvStore` wrapper over `MemoryStore` in the tests: `get_many` counted once, `get` zero times); order preserved; a missing hash is `None` in place; an empty list is an empty answer with no backend call; `put_many` then `get_many` round-trips; `put_many` of a held body keeps one copy and refreshes its time; a corrupted body in a batch is an `Err`; both backends | `get_many` looping over `get` (the counter goes red) | an empty list; the same hash twice in one ask (two answers); 1,000 hashes; an empty body in a batch |

## Log

- 2026-11-10 gitsense 076 lead: task written from D076-8 (the coordinator's brief); lane K launched.
- 2026-11-10 lane K: built `KvStore::{get_many, put_many}` (looping defaults; memory one lock; rocksdb `multi_get` + one `WriteBatch`) and `ContentStore::{get_many, put_many}` (one backend call per batch; `put_many` asks held bodies in one backend `get_many`, then one `put_many`; repeated bodies in a batch are written once). Red first (tests did not compile), then green on both backends; call-counting `KvStore` wrapper in `tests/cas.rs`.

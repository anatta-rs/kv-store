# Changelog

All notable changes to this project are documented here, following the
Keep a Changelog format and Semantic Versioning.

## Unreleased

### Breaking

- `ContentStore` gains two required methods, `delete_many` and `delete_many_force`: its
  implementors must add them. `KvContentStore::new` is unchanged; a new constructor
  `with_options` takes a `ContentOptions { grace }` (default one hour).

### Added

- `ContentStore::delete_many(&[ContentHash]) -> DeleteOutcome { destroyed, kept, absent }`:
  a put always wins. A hash whose last put (`content-at`) is younger than the grace is kept;
  an older one is destroyed, body and time, in one batch. The read of the times and the batch
  run under the store's write lock, so a put racing a delete ends with the bytes present
  whatever the order. `delete_many_force` ignores the grace, for an offline repair.
- `KvStore::delete_many` (default loops `delete`; memory in one lock, `RocksDB` in one
  `WriteBatch`).
- `RocksDbStore::open_with(path, RocksDbOptions { sync_writes })`: fsync the write-ahead log
  on every write, off by default (`open` is unchanged). Cost measured in task 004.

- `ContentStore::get_many` and `put_many`, and `KvStore::get_many` and `put_many` (default
  bodies loop over `get` and `put`; memory in one lock, `RocksDB` in one `multi_get` and one
  `WriteBatch`): `KvContentStore` makes one backend call per batch, not one per hash.
  `ContentStore` gains two required methods: its implementors must add them.

## 0.2.1 - 2026-11-09

### Fixed

- `put` of bytes already held now refreshes the stored time (`content-at:<hex>`) to now,
  without rewriting or recompressing the bytes. Before, the first time was kept, so a
  consumer's grace window was bypassed for an old blob orphaned and re-imported.
  The listed time is now that of the last put.

## 0.2.0 - 2026-11-08

### Breaking

- `KvStore::scan_keys` is a new required method: every `KvStore` implementor must add it.
- `ContentStore` is now the CAS port trait, implemented by `KvContentStore<K>`.
- `put(&[u8]) -> ContentHash` computes the hash; the caller-supplied hash is removed.
- The library is renamed from `kv_server` to `kv_store`.
- The `sled` backend is removed.
- The HTTP server and `HttpKvStore` are behind the off-by-default `http-server` feature.
- The license changes from MIT to Apache-2.0.

### Added

- `KvContentStore` serialises `put` and `delete` within one process (the store is single-process).
- `get` verifies the decompressed body's hash and reports `KvError::HashMismatch` on mismatch.
- `list(after, limit)` returns each blob's hash and first stored time, kept in `content-at:<hex>`.
- `KvStore::scan_keys` for paginated, keys-only scans.
- `MemoryStore`, enabled by the `memory-backend` feature.
- `KvError::Unsupported` for unsupported backend operations.
- Rust 1.99 toolchain, clippy pedantic checks, `scripts/check.sh`, and a committed `Cargo.lock`.
- An ignored 1 GiB RocksDB put/get soak with paged listing, timestamp checks, throughput, and disk-size reporting.

### Changed

- `list` skips keys under `content:` that are not a canonical lowercase hash, with a warning.

# Changelog

All notable changes to this project are documented here, following the
Keep a Changelog format and Semantic Versioning.

## Unreleased

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

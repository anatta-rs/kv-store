# kv-store

> Implementations of [`polystore::KvStore`](https://github.com/anatta-rs/polystore) for multiple backends, plus an optional HTTP server bin.

## Philosophy

- **Lib-first**: `KvStore` trait impls for multiple backends, each gated by a feature flag. Embed directly into your application.
- **Bin-optional**: an HTTP server (`kv-server` bin) for distributed access. Only compiled when a backend feature is enabled.
- **Backends as features**: pick one or many; runtime CLI flag selects which backend the bin uses.

## Backends

| Feature | Crate dep | Status |
|---|---|---|
| `rocksdb-backend` (default) | `rocksdb` | ready |
| `sled-backend` | `sled` | ready |
| `memory-backend` | — | dev / tests |

## Use as a library

```rust
use kv_store::{KvStore, RocksDbStore};

let store = RocksDbStore::open("/data/kv")?;
store.put(b"key", b"value").await?;
let v = store.get(b"key").await?;
```

Disable defaults to ship without RocksDB :

```toml
[dependencies]
kv-store = { git = "https://github.com/anatta-rs/kv-store", default-features = false, features = ["sled-backend"] }
```

## Use as a binary (HTTP server)

```bash
cargo install --git https://github.com/anatta-rs/kv-store --features rocksdb-backend
KV_DATA_PATH=/data/kv KV_PORT=8080 kv-server
```

CLI flags pick the backend at runtime when multiple are compiled in.

## HTTP API

```
GET    /health           → 200 OK
GET    /kv/{key}         → 200 + bytes | 404
PUT    /kv/{key}         → 201 (body = bytes)
DELETE /kv/{key}         → 204
GET    /scan?prefix=X    → 200 + JSON list (streamed)
```

## Roadmap

| | |
|---|---|
| ✅ rocksdb / sled / memory backends | |
| ✅ HTTP server bin | |
| 🟡 backend-agnostic CLI flag (`--backend rocksdb`) | |
| 🟡 stream `/scan` as NDJSON | |

## License

Apache-2.0.

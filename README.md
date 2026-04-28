# kv-server

Trait-generic HTTP server and library for key-value storage. Ships both:
- **HTTP server** (`kv-server` bin): standalone REST service with feature-gated backends
- **Library** (`kv_server` crate): trait implementations and storage abstractions for embedding

## Features

Pluggable KvStore backends via Cargo features:
- `rocksdb-backend` (default): embedded `RocksDB` for production use
- `sled-backend`: embedded sled KV store
- `memory-backend`: in-memory store (dev/tests only)

HTTP server capabilities:
- RESTful interface with CORS enabled
- Content-addressable storage (SHA-256 `ContentHash`)
- Automatic zstd compression via `ContentStore`
- Streaming scan with prefix filtering
- Request tracing via tower-http

## Usage

### Running the HTTP server

Run with default RocksDB backend:

```bash
cargo run --release
```

Or disable default features:

```bash
cargo run --no-default-features --features rocksdb-backend --release
```

Environment variables:

- `KV_DATA_PATH` — storage directory (default: `/data/kv`)
- `KV_PORT` — HTTP port (default: `8080`)
- `RUST_LOG` — tracing level (default: `kv_server=debug,tower_http=debug`)

## API

### Health check
```bash
GET /health → 200 OK
```

### Get value
```bash
GET /kv/{key}
→ 200 OK + binary value (on hit)
→ 404 NOT FOUND (on miss)
```

### Put value
```bash
PUT /kv/{key} + binary body
→ 201 CREATED
```

### Delete key
```bash
DELETE /kv/{key}
→ 204 NO CONTENT
```

### Check existence
```bash
HEAD /kv/{key}
→ 200 OK (exists)
→ 404 NOT FOUND (missing)
```

### Scan with prefix
```bash
GET /kv?prefix=foo
→ 200 OK + JSON array of {key, value (base64-encoded)}
```

### Embedding in other crates

Use the library to build custom KvStore implementations:

```rust
use kv_server::{RocksDbStore, KvStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = RocksDbStore::open(std::path::Path::new("./data"))?;
    store.put("key:123", b"value".to_vec()).await?;
    let val = store.get("key:123").await?;
    Ok(())
}
```

## Backends

### RocksDB (default, feature: `rocksdb-backend`)
Production-grade persistent embedded database.

### sled (feature: `sled-backend`)
Pure-Rust embedded database with ACID transactions.

### In-memory (feature: `memory-backend`)
Lightweight ephemeral store for testing and development.

## Building

```bash
# Default build (rocksdb-backend)
cargo build --release

# Verify all configurations
cargo check --all-targets --all-features
cargo check --no-default-features
cargo check --no-default-features --features memory-backend

# Code quality
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo fmt --check
```

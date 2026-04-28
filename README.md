# polystore-kv-server

HTTP server wrapping any `polystore::KvStore` implementation. Exposes a simple REST API for distributed key-value operations.

## Features

- Pluggable backends (RocksDB default, in-memory for dev/tests)
- RESTful HTTP interface with CORS enabled
- Comprehensive health checks and error handling
- Streaming scan with prefix filtering
- Request tracing via tower-http

## Usage

Run the server with default RocksDB backend:

```bash
cargo run --release
```

Environment variables:

- `KV_DATA_PATH` — storage directory (default: `/data/kv`)
- `KV_PORT` — HTTP port (default: `8080`)
- `RUST_LOG` — tracing level (default: `polystore_kv_server=debug,tower_http=debug`)

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

## Backends

### RocksDB (default)
Production-grade persistent storage. Requires `rocksdb` system libraries.

```bash
cargo run --release
```

### In-memory (development)
Lightweight ephemeral store for testing. Rebuild polystore with memory backend feature and add `memory-backend` feature to this crate (future work).

## Building

```bash
cargo build --release
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo fmt --check
```

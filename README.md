# kv-store

The deployment's content-addressed store (CAS): blobs addressed by SHA-256,
compressed with zstd level 3, and stored under `content:<hex>` in a key-value
backend. The store has no tenant.

The deployment's CAS is never on a transport a caller reaches. Services read a
blob in-process after `decide` on the graph node that references it.

## Content store

`ContentStore` is the async CAS port. Its implementation, `KvContentStore<K: KvStore>`,
wraps a key-value backend. Each operation returns a `Result` with `KvError` on failure:

| Operation | Successful result |
| --- | --- |
| `put(&[u8])` | `ContentHash`, computed from the uncompressed body |
| `get(ContentHash)` | `Option<Vec<u8>>`, the uncompressed body or `None` |
| `has(ContentHash)` | `bool`, whether the blob exists |
| `delete(ContentHash)` | `()`, removing the blob and its stored time; absent blobs are a no-op |
| `list(after: Option<ContentHash>, limit: usize)` | `Vec<(ContentHash, SystemTime)>` in ascending hash-hex order, strictly after the cursor, with at most `limit` entries |

`get` verifies the decompressed body's hash and returns `KvError::HashMismatch`
if it differs from the requested address.

Each listed time is when the blob was last put. Re-putting an existing blob
refreshes that time and leaves its bytes untouched. It lives in the sibling key `content-at:<hex>` as eight
bytes of big-endian `u64` Unix milliseconds, written before a new blob's bytes.
`list` uses `KvStore::scan_keys` to page over content keys and reads only their
small timestamp values. A blob without a recorded time is listed with
`std::time::UNIX_EPOCH`.

## Example

Enable `memory-backend` for `MemoryStore`. This example uses Tokio as its async
runtime and runs as a README doctest with
`cargo test --features memory-backend --doc`.

```rust
use kv_store::{ContentHash, ContentStore, KvContentStore, MemoryStore};

#[tokio::main]
async fn main() -> Result<(), kv_store::KvError> {
    let store = KvContentStore::new(MemoryStore::new());
    let body = b"hello anatta";

    let hash = store.put(body).await?;
    assert_eq!(hash, ContentHash::from_bytes(body));
    assert_eq!(store.get(hash).await?, Some(body.to_vec()));
    assert!(store.has(hash).await?);

    let page = store.list(None, 100).await?;
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].0, hash);
    assert!(page[0].1 > std::time::UNIX_EPOCH);
    assert!(store.list(Some(hash), 100).await?.is_empty());
    Ok(())
}
```

## Features

| Feature | Default | Provides |
| --- | --- | --- |
| `rocksdb-backend` | On | `RocksDbStore`, an embedded persistent backend |
| `memory-backend` | Off | `MemoryStore`, an ephemeral in-memory backend |
| `http-server` | Off | The `kv-store` binary and `HttpKvStore` client; the binary also requires `rocksdb-backend` |

Use `default-features = false` with `features = ["memory-backend"]` for a
memory-only library dependency. The crate is imported as `kv_store`.

## Optional HTTP server

**For the user's own use only.** The HTTP server has no authentication, binds
all interfaces (`0.0.0.0`), and enables permissive CORS. It serves the raw
`KvStore`, not the CAS. It must not expose the deployment's content store to
callers.

Build and run it with the default RocksDB backend:

```sh
cargo run --features http-server --bin kv-store
```

`KV_PORT` defaults to `8080`; `KV_DATA_PATH` defaults to `/data/kv`.
Set `KV_DATA_PATH` to a writable directory when running locally.

| Method | Route | Operation |
| --- | --- | --- |
| `GET` | `/health` | Health check |
| `GET` | `/kv/{key}` | Read a raw value |
| `PUT` | `/kv/{key}` | Write the request body as a raw value |
| `DELETE` | `/kv/{key}` | Delete a key |
| `HEAD` | `/kv/{key}` | Check whether a key exists |
| `GET` | `/kv?prefix=` | Scan key/value pairs by prefix |

`HttpKvStore::scan_keys` is unsupported and returns `KvError::Unsupported`;
there is no keys-only HTTP route.

## Checks and soak

Use the shared Rust 1.99 toolchain and sccache:

```sh
. scripts/env.sh
export CARGO_TARGET_DIR="$PWD/target"
export RUSTC_WRAPPER=sccache
sh scripts/check.sh
```

The ignored soak writes and reads 1024 distinct, deterministically generated,
incompressible 1 MiB bodies through `KvContentStore<RocksDbStore>`. It verifies
hashes and paged listing with stored times, and prints elapsed times,
put/get MiB/s, and the database directory's size. Timings include body generation
and verification. Bodies are regenerated as needed instead of retaining 1 GiB
in memory. The temporary database is created under Cargo's target temporary
directory and removed when the test finishes.

```sh
cargo test --release --test soak -- --ignored --nocapture
```

## License

Apache-2.0. See [LICENSE](LICENSE).

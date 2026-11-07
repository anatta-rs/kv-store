# 001 Revive kv-store as the anatta CAS

status: proposed
rank: 8 (wave 1's first task; 046, 157 and 185 depend on it; small surface, no engine coupling)
level: standard (4-6): one crate, one port reshaped, no rights; no deployed CAS data is known (unknown, not checked beyond this repository)
request: the user, 2026-11-07: "on n'a pas un repo kv/store ? je crois que dans mon ancienne version j'avais déjà ça". Plan wording (`~/.claude/plans/crispy-sauteeing-gosling.md`, Wave 1, task 001): "revive: workspace toolchain, clippy pedantic, `ContentStore` as the CAS port (`put(bytes)→hash`, `get`, `has`), rocksdb + memory, tag v0.2.0; HTTP bin kept but off by default"; oracle "15+ tests on both backends; a 1 GB put/get soak".
repository: kv-store (`~/projects/anatta/anatta-infra/services/kv-store`, remote `git@github.com:anatta-rs/kv-store.git`, branch `main`, HEAD `e0a3612` 2026-04-28, no tags)
depends: none
model: lanes on Codex (plan, Execution); Opus lead supervises

## Summary

**Asked.** Make the user's existing kv-store the one content-addressed store (CAS) of a deployment (plan §11): blobs by sha-256, no tenant, never on a transport a caller reaches; services read a blob after `decide` on the graph node that references it.

**Decided.**
- D1. The CAS is this crate, revived, not rewritten (plan, Context).
- D2. `put(&[u8]) -> ContentHash`: the store computes the hash itself. The caller-supplied hash of today (`src/content.rs:39`) goes; no `put_with_hash` (Fable, 2026-11-07).
- D3. Reads: `get(ContentHash)`, `has(ContentHash)`.
- D4. Backends: rocksdb and memory; the memory backend leaves the test module (`src/content.rs:78-121`) and becomes the `memory-backend` feature (today `Cargo.toml:24`, gating nothing).
- D5. The HTTP bin stays but is off by default.
- D6. Tag `v0.2.0` (annotated) on `main` after merge.
- D7. Clippy pedantic with `-D warnings`, functions under 50 lines, sccache, `scripts/check.sh` (rust-anatta skill).
- D8. Oracle: 15+ tests on both backends (one generic suite, instantiated twice); a 1 GiB put/get soak behind `#[ignore]`.
- D9. No Studio project exists for kv-store (project_list 2026-11-07); none is created by this task (Fable: "fine as recorded").
- D10 (Q1). `ContentStore` is a **trait, the port**, with one impl over `KvStore`. The port also has `list`, paged, over `scan_prefix("content:")`: anatta-graph 046 (sweep) reads it (Fable, 2026-11-07).
- D11 (Q2). `delete` is kept on the port.
- D12 (Q3). sled is dropped: the `sled` dependency and the `sled-backend` feature go.
- D13 (Q4). License Apache-2.0, as the README says: the user ruled for the one that lets us sell what we build, and Apache-2.0 adds the explicit patent grant; `Cargo.toml:6` changes from MIT, a `LICENSE` file is added (the user, 2026-11-07; replaces the MIT line).
- D14 (Q5). Lib renamed `kv_store`; `repository = "https://github.com/anatta-rs/kv-store"`.
- D15 (Q6). A single crate with the shared toolchain files (`rust-toolchain.toml`, `scripts/env.sh`, `clippy.toml`); no Cargo workspace.
- D16 (Q7). `HttpKvStore` and the bin both behind one off-by-default feature `http-server`.
- D17 (Q8). `Cargo.lock` is committed (removed from `.gitignore`); `scripts/check.sh` runs `--locked`.
- D18. The README is rewritten in 001: no polystore, the real API, the bin off by default, the port.

- D19 (Q9). Add `KvStore::scan_keys(prefix: &str, after: Option<&str>, limit: usize) -> Vec<String>`, keys only, paged; `ContentStore::list` is built on it (not on `scan_prefix`, which reads values). `HttpKvStore::scan_keys` returns an `Unsupported` error (new variant `KvError::Unsupported(String)`); the bin gets no keys-only route in this task (Fable, 2026-11-07).
- D20 (Q9). The impl over `KvStore` is named `KvContentStore<K>` (Fable, 2026-11-07).

**Ruled out.** A new CAS crate; the CAS behind HTTP for services (§11); tenancy or rights in the CAS; the sweep itself (046); sled; a Cargo workspace; `put_with_hash`.

**Open.** None (Q9 answered: D19, D20).
- Q9 (answered, kept for the record). `list` paged "over `scan_prefix`": today's `KvStore::scan_prefix` (`src/store.rs:30-33`) returns every key **with its value**, unpaged, so one page of hashes built on it reads every blob of the store into memory (1 GiB at the soak's size). Options: (a) add a keys-only paged method to `KvStore`, `scan_keys(prefix, after, limit)`, rocksdb seeking from `after`, and build `list` on it [lean; written below as proposed]; (b) build `list` on `scan_prefix` as is and accept the cost until 046 measures it; (c) `list` returns a `Stream` fed by a rocksdb iterator (adds `futures-core`; `KvStore` grows a stream method anyway). With (a), `HttpKvStore::scan_keys`: unsupported error, or a new keys-only route on the bin? Also confirm the impl's name `KvContentStore<K>` (proposed).

## Grounded context and impact

GitSense was down (ECONNREFUSED); kv-store's presence in the `anatta` index is unknown. Files read locally with the Read tool at `e0a3612`.

**History.** `52681dd` "initial extract from anatta-rs/Anatta"; `e0a3612` (2026-04-28) "absorb polystore-rocksdb impl as rocksdb-backend feature", renamed kv-server → kv-store, "polystore-rocksdb repo becomes redundant".

**polystore.** `README.md:3` says the crate implements `polystore::KvStore`. **The crate no longer depends on polystore**: `Cargo.toml:26-40` has no `polystore` entry; the trait is local, `src/store.rs:14`. The README is stale (D18).

**Files and symbols.**
- `Cargo.toml:1-56`: package `kv-store` 0.1.0, edition 2024; `[[bin]] kv-store` at `src/bin/kv-server.rs`, `required-features = ["rocksdb-backend"]` (`:11-14`), so today the bin is **built by default**; `[lib] kv_server` (`:16-18`); features `default = ["rocksdb-backend"]`, `sled-backend`, `memory-backend = []` (`:20-24`); `repository` `anatta-rs/kv-server` (`:7`); lints `unsafe_code = "deny"`, `missing_docs = "warn"`, clippy groups with `pedantic = warn` and four allows (`:42-56`). No `clippy.toml`, no `rust-toolchain.toml`, no `LICENSE`.
- Dependencies (`Cargo.toml:27-40`, requirements; resolved versions unknown, no `Cargo.lock`, `.gitignore:2`): axum 0.8, serde 1.0 (derive), serde_json 1.0, tokio 1 (full), tracing 0.1, tracing-subscriber 0.3 (env-filter), tower-http 0.6 (trace, cors), thiserror 2, sha2 0.10, hex 0.4, zstd 0.13, rocksdb 0.23 (optional), sled 0.34 (optional), reqwest 0.12 (json). axum, tokio full, tower-http, tracing-subscriber, serde_json and reqwest are **unconditional** today.
- `src/lib.rs:14-28`: modules and re-exports `ContentStore`, `KvError`, `ContentHash`, `HttpKvStore`, `RocksDbStore` (cfg), `KvStore`. Doc `:6-7` claims sled and memory backends: **neither exists as code**.
- `src/store.rs:14-34` `KvStore: Send + Sync`: `get`, `put(&str, Vec<u8>)`, `delete`, `exists`, `scan_prefix(&str) -> Vec<(String, Vec<u8>)>`, all `impl Future + Send` (RPITIT, the idiom the new port follows); blanket `impl KvStore for &T` (`:37-60`).
- `src/content.rs`: `ContentStore<K: KvStore> { inner: K }` (`:19-22`, `#[allow(dead_code)]`); `new` `:26`, `inner` `:31`; `put(hash, body)` `:39-44`, zstd level 3 (`:11`), key `content:<hex>`, **never checks the hash against the body**; `get` `:47-57`; `has` `:60-63`; `delete` `:66-69` (doc mentions Neo4j). Tests `:123-263`: 10, memory only, each with its own `Runtime`; `get_absent_key_returns_none` (`:164-171`) **asserts nothing**.
- `src/hash.rs`: `ContentHash([u8; 32])` serde-transparent (`:7-9`), `from_bytes` sha-256 (`:15-22`), `as_hex` `:25`, `hex` `:30`, `as_bytes` `:35`, `From<[u8;32]>` `:40`, `TryFrom<&str>` (`Error = String`, `:50-62`), `Display` `:64`; 5 tests `:74-107`.
- `src/error.rs:5-21` `KvError { Backend, InvalidKey, Serialization, HashMismatch }`; `HashMismatch` unused today; `From<rocksdb::Error>` `:23-28`.
- `src/rocksdb.rs`: `RocksDbStore { db }` (`:11-13`), `open(&Path)` with a zero-length fixed prefix extractor (`:21-27`, effect unknown), blocking calls inside `async fn` (`:31-69`), `scan_prefix` seeks with `IteratorMode::From` and stops at the first non-matching key (`:49-69`). **No tests.**
- `src/http.rs`: `HttpKvStore` client over reqwest (`:12-31`), `KvStore` impl over `/kv/{key}` (`:33-136`), hand-written base64 decoder (`:145-184`).
- `src/bin/kv-server.rs`: binds `0.0.0.0:$KV_PORT` (8080, `:28-30`), data `$KV_DATA_PATH` (`/data/kv`), routes `/health`, `GET|PUT|DELETE|HEAD /kv/{key}`, `GET /kv?prefix=` (`:35-41`), 50 MiB limit (`:43`), **permissive CORS** (`:44`), no auth; serves the raw `RocksDbStore`, not the CAS. Uses `kv_server::{KvStore, RocksDbStore}` (`:18`): renamed by D14. README's `/scan?prefix=` (`README.md:52`) is stale.
- `.github/workflows/ci.yml`: fmt, clippy/test/check `--all-features` on `dtolnay/rust-toolchain@stable`.
- `README.md:21-27`: example uses `kv_store::` with byte keys: stale.

**Why the HTTP bin stays off by default.** §11: the CAS is never on a transport a caller reaches; a blob is read after `decide` on its node. The bin listens on every interface with permissive CORS, no auth, raw keys (`kv-server.rs:30,44`); built by default it invites exactly that. Kept behind `http-server` for the user's own use (docker-compose, `kv-server.rs:5`).

**Impact.** No dependent repository is known (unknown: GitSense down; `get_impact` when it is up). Future callers: anatta-graph 046 (`list`, `delete`), anatta-auth 157, studio 185. Breaking changes: `put` signature, `ContentStore` from struct to trait, lib name `kv_server` → `kv_store`, sled removed: 0.1.0 → 0.2.0.

**Toolchain model (gitsense).** `rust-toolchain.toml` channel 1.99 + rustfmt, clippy; `clippy.toml` `too-many-lines-threshold = 50`; `scripts/env.sh` exports `RUSTUP_HOME=$HOME/.rustup-pc/rustup`, `CARGO_HOME=$HOME/.rustup-pc/cargo`, PATH; `scripts/check.sh` sources it. anatta-graph's `scripts/check.sh` adds `--locked --offline` and `RUSTDOCFLAGS="-D warnings" cargo doc`. anatta-graph has `AGENTS.md`, not `CLAUDE.md`; gitsense's `CLAUDE.md` is the model.

**Sippa.** `find_decision` (CAS, 2026-11-07): no CAS decision; nearest gitsense r4, d16, not binding. `discover`: rust-anatta, run-lane, development.

## Design

TARGET:

```mermaid
flowchart TB
  subgraph callers["graph services, in-process, after decide on the node"]
    gs["anatta-auth 157, studio 185"]
    sweep["anatta-graph 046 sweep"]
  end
  subgraph kvstore["kv-store crate (lib kv_store)"]
    port["trait ContentStore (the CAS port)\nput(&[u8]) -> ContentHash\nget(ContentHash) -> Option<Vec<u8>>\nhas(ContentHash) -> bool\ndelete(ContentHash)\nlist(after, limit) -> Vec<ContentHash>"]
    impl1["KvContentStore<K: KvStore>\nsha-256, zstd 3, key content:<hex>\nget verifies the hash"]
    hash["ContentHash::from_bytes"]
    kv["trait KvStore\nget put delete exists scan_prefix\nscan_keys(prefix, after, limit)"]
    rocks["RocksDbStore\nfeature rocksdb-backend (default)"]
    mem["MemoryStore\nfeature memory-backend"]
    subgraph http["feature http-server (off by default)"]
      httpc["HttpKvStore"]
      bin["bin kv-store (axum)"]
    end
  end
  gs --> port
  sweep --> port
  impl1 -. implements .-> port
  impl1 --> hash
  impl1 --> kv
  rocks -. implements .-> kv
  mem -. implements .-> kv
  httpc -. implements .-> kv
  bin --> rocks
```

`put` hashes the plain bytes (identity is the content, not its compressed form), returns early when `has` is true, compresses, writes. `get` decompresses and re-hashes: a mismatch is `KvError::HashMismatch` (`error.rs:19`). `list` strips `content:` and parses hex; a key that does not parse is `KvError::InvalidKey`. The bin depends on a backend, never the reverse; nothing in the default feature set opens a socket.

## AST target

Crate `kv-store` 0.2.0, lib `kv_store`, edition 2024, single crate (D15).

**`Cargo.toml`**
- `repository = "https://github.com/anatta-rs/kv-store"`, `license = "Apache-2.0"`, description "Content-addressed store (sha-256, zstd) over a key-value port; rocksdb and memory backends; optional HTTP server."; keywords without `polystore`.
- `[lib] name = "kv_store"`.
- `[features]`: `default = ["rocksdb-backend"]`; `rocksdb-backend = ["dep:rocksdb"]`; `memory-backend = []`; `http-server = ["dep:axum", "dep:tower-http", "dep:tracing-subscriber", "dep:reqwest", "dep:serde_json", "tokio/full"]`. sled removed (D12).
- `[dependencies]`: unconditional `thiserror`, `sha2`, `hex`, `zstd`, `serde` (derive, for `ContentHash`), `tracing`; `tokio` optional under `http-server` if the lib needs none of it (unknown until lane 1 builds; the lib code read today uses no tokio item outside tests and the bin); the HTTP stack optional.
- `[dev-dependencies]`: `tokio` (`rt-multi-thread`, `macros`), `tempfile`.
- `[[bin]] name = "kv-store"`, `path = "src/bin/kv-server.rs"`, `required-features = ["rocksdb-backend", "http-server"]`.
- `[lints.rust] unsafe_code = "forbid"`, `missing_docs = "warn"`; clippy groups kept.

**`src/lib.rs`**: `pub mod content; pub mod error; pub mod hash; pub mod store;` `#[cfg(feature = "memory-backend")] pub mod memory;` `#[cfg(feature = "rocksdb-backend")] pub mod rocksdb;` `#[cfg(feature = "http-server")] pub mod http;` Re-exports `ContentStore`, `KvContentStore`, `ContentHash`, `KvError`, `KvStore`, and the cfg'd `MemoryStore`, `RocksDbStore`, `HttpKvStore`. Crate doc: the CAS, §11's rule that it is never on a reachable transport.

**`src/content.rs`** (the port, D10)
```rust
pub const CONTENT_PREFIX: &str = "content:";
pub trait ContentStore: Send + Sync {
    fn put(&self, body: &[u8]) -> impl Future<Output = Result<ContentHash, KvError>> + Send;
    fn get(&self, hash: ContentHash) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send;
    fn has(&self, hash: ContentHash) -> impl Future<Output = Result<bool, KvError>> + Send;
    fn delete(&self, hash: ContentHash) -> impl Future<Output = Result<(), KvError>> + Send;
    /// Hashes in ascending hex order, strictly after `after`, at most `limit`.
    fn list(&self, after: Option<ContentHash>, limit: usize)
        -> impl Future<Output = Result<Vec<ContentHash>, KvError>> + Send;
}
pub struct KvContentStore<K: KvStore> { inner: K }   // D20
impl<K: KvStore> KvContentStore<K> { pub fn new(inner: K) -> Self; pub fn inner(&self) -> &K; }
impl<K: KvStore> ContentStore for KvContentStore<K> { /* five methods */ }
fn key(hash: ContentHash) -> String;                           // "content:<hex>"
fn compress(body: &[u8]) -> Result<Vec<u8>, KvError>;          // zstd level 3
fn decompress_verified(hash: ContentHash, raw: &[u8]) -> Result<Vec<u8>, KvError>;
fn hash_from_key(key: &str) -> Result<ContentHash, KvError>;
```
Today's test module moves to `tests/cas.rs`; its `InMemoryKvStore` is deleted (replaced by `MemoryStore`).

**`src/store.rs`** (D19): add
```rust
/// Keys starting with `prefix`, strictly after `after`, ascending, at most `limit`; values not read.
fn scan_keys(&self, prefix: &str, after: Option<&str>, limit: usize)
    -> impl Future<Output = Result<Vec<String>, KvError>> + Send;
```
and its blanket `&T` delegation. Implemented in `rocksdb.rs` (`IteratorMode::From(after or prefix)`, skip `after` itself, stop at prefix end or `limit`), `memory.rs` (`BTreeMap::range`), `http.rs` (returns `KvError::Unsupported`, D19). `src/error.rs` gains `#[error("unsupported: {0}")] Unsupported(String)`.

**`src/memory.rs`** (new, `memory-backend`): `#[derive(Default)] pub struct MemoryStore { map: std::sync::Mutex<BTreeMap<String, Vec<u8>>> }`, `pub fn new() -> Self`, `impl KvStore` (a poisoned lock maps to `KvError::Backend`, no `unwrap`).

**Unchanged API**: `src/rocksdb.rs` (plus `scan_keys`), `src/hash.rs`, `src/error.rs` (plus `Unsupported`). `src/http.rs`: cfg `http-server`. `src/bin/kv-server.rs`: `use kv_store::…`; otherwise unchanged (it serves the raw `KvStore`; the README says so).

**Repository files**
- `rust-toolchain.toml`: channel `1.99`, components rustfmt, clippy (gitsense's). `clippy.toml`: `too-many-lines-threshold = 50`. `scripts/env.sh`: gitsense's three exports.
- `scripts/check.sh` (sh, `set -eu`, sources `scripts/env.sh` when `$HOME/.rustup-pc` exists, never touches `RUSTC_WRAPPER`): `cargo fmt --all --check`; `cargo clippy --locked --all-targets -- -D warnings` (default); `cargo clippy --locked --no-default-features --features memory-backend --all-targets -- -D warnings`; `cargo clippy --locked --features memory-backend,http-server --all-targets -- -D warnings`; `cargo test --locked --features memory-backend`; `RUSTDOCFLAGS="-D warnings" cargo doc --locked --features memory-backend,http-server --no-deps`.
- `.gitignore`: the `Cargo.lock` line removed; `Cargo.lock` committed (D17).
- `.github/workflows/ci.yml`: one job, `dtolnay/rust-toolchain@1.99` with rustfmt, clippy, then `sh scripts/check.sh`.
- `CLAUDE.md` (new): the Anatta header (as gitsense's: Sippa calls, GitSense reading); the crate is the deployment's CAS (plan §11); never on a reachable transport, `http-server` off by default; the check; sccache and `cargo sweep --time 7`; releases (semver, `CHANGELOG.md`, annotated tag on `main`); commits (Conventional, staged by path, squash merge).
- `LICENSE` (Apache-2.0, 2026, holder as the user's GitHub identity). `CHANGELOG.md` (new): `## 0.2.0` with the breaking changes. `README.md` rewritten (D18): what it is, the port with an example on `KvContentStore<MemoryStore>` (a doctest), features table (rocksdb default, memory, http-server off), the bin's real routes (`/kv?prefix=`) and its warning (no auth, all interfaces), Apache-2.0.

**Tests**
- `tests/cas.rs`: each case `async fn <case><S: ContentStore>(s: &S)`, called by two `#[tokio::test]` wrappers `memory_<case>` and `rocksdb_<case>` (tempdir), generated by `macro_rules! both_backends`. Cases (15): `put_returns_sha256_of_body`, `put_then_get_roundtrip`, `put_twice_is_one_entry`, `put_empty_body`, `get_absent_is_none`, `has_false_before_true_after`, `non_utf8_roundtrip`, `large_body_2mib_roundtrip`, `stored_bytes_are_compressed`, `concurrent_puts_same_body`, `distinct_bodies_distinct_hashes`, `get_detects_corruption` (raw key overwritten through `inner()`: `HashMismatch`), `delete_then_absent`, `list_pages_in_hex_order` (25 blobs, limit 10: 10, 10, 5, no repeats, all found), `list_ignores_other_prefixes`. Rocksdb only: `rocksdb_reopen_keeps_blobs`. Total 31.
- `tests/kv.rs` (both backends): `scan_prefix_sorted`, `scan_keys_after_and_limit`, `delete_missing_is_ok` (6).
- `src/hash.rs` tests kept (5).
- `tests/soak.rs`: `#[ignore] #[tokio::test] soak_1gib_put_get_rocksdb`: 1024 distinct 1 MiB seeded incompressible bodies; put all, get all and compare hashes, `list` pages count 1024; prints elapsed and on-disk size.

**Release**: `version = "0.2.0"` in the diff; after merge, `CHANGELOG.md` dated, annotated tag `v0.2.0` on `main`.

## Lanes

Codex lanes, sequential (lanes 1-3 share `Cargo.toml`, `src/lib.rs`, `src/content.rs`); lane 4's docs may run beside lane 3. Each lane gets D1-D20, writes tests red first, runs one check, commits, and writes `QUESTION.md` instead of guessing.

| # | Lane | Files | After | Tokens | Calls | Commit | State | Questions |
|---|---|---|---|---|---|---|---|---|
| 1 | toolchain and features | `rust-toolchain.toml`, `clippy.toml`, `scripts/env.sh`, `scripts/check.sh`, `Cargo.toml` (lib rename, features, sled out, lints, optional deps), `Cargo.lock`, `.gitignore`, `src/lib.rs` (cfgs), `src/http.rs` (cfg only), `src/bin/kv-server.rs` (`kv_store::`), `.github/workflows/ci.yml`, `CLAUDE.md`, `LICENSE`; oracle: `scripts/check.sh` green on today's tests; `cargo tree` default has no axum | — | | | | ready (after the user's gate) | 0 |
| 2 | memory backend and scan_keys | `src/memory.rs`, `src/store.rs`, `src/rocksdb.rs`, `src/http.rs` (`scan_keys` → `Unsupported`), `src/error.rs`, `src/lib.rs`, `tests/kv.rs`; oracle: `tests/kv.rs` green on both backends | 1 | | | | ready after 1 (D19) | 0 |
| 3 | the CAS port | `src/content.rs`, `tests/cas.rs`; tests red first; oracle: 31 tests green | 2 | | | | ready after 2 (D19, D20) | 0 |
| 4 | soak and docs | `tests/soak.rs`, `README.md`, `CHANGELOG.md`; oracle: `--ignored soak_1gib_put_get_rocksdb` passes, README doctest compiles | soak after 3; docs ∥ 3 | | | | blocked | 0 |

## Acceptance

- `scripts/check.sh` green on toolchain 1.99: clippy pedantic `-D warnings` on three feature sets, rustdoc `-D warnings`, `--locked`.
- `cargo tree -e normal` with default features lists no axum, tower-http, reqwest.
- 15 CAS cases pass on memory and on rocksdb (31 tests with the reopen case); `put` returns `ContentHash::from_bytes(body)`; `list` pages exactly.
- `cargo test --release --test soak -- --ignored` passes; time and disk size recorded in the Log.
- `LICENSE` Apache-2.0; README and CHANGELOG match the code; annotated tag `v0.2.0` on `main`.

## Workflow contract

Current step: ast-target (complete), no question open
Input: plan Wave 1 row 001, Fable's answers Q1-Q9 (2026-11-07), this grounded context
Artifact: this TARGET and AST target
Target thread: none (no Studio project, D9)
Next persona: the user for the TARGET and AST target gates
Validation gate: TARGET and AST target validated by the user before lane 1

## Studio

No Studio project for kv-store (project_list, 2026-11-07: 15 projects, none for kv-store). Not created (D9). The eight threads are opened if a project is created later.

## Log

- 2026-11-07 Opus lead: grounded at `e0a3612` by local reads (GitSense down); Sippa find_decision and discover; Q1-Q8 sent to Fable.
- 2026-11-07 Fable answered Q1-Q8 (D10-D18). AST target and lanes completed; Q9 (paging `list` over an unpaged, value-loading `scan_prefix`; impl name) sent to Fable. Nothing committed, no build run.
- 2026-11-07 Fable answered Q9: keys-only paged `scan_keys`, `HttpKvStore` returns `Unsupported`, no bin route; `KvContentStore<K>` confirmed (D19, D20). Lanes 2 and 3 unblocked; all lanes wait for the user's gate.
- 2026-11-07: the user ruled the license: Apache-2.0 (D13 revised); the manifest aligns to the README.

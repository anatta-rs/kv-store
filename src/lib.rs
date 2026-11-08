//! The deployment's content-addressed store (CAS).
//!
//! [`ContentStore`] stores SHA-256-addressed blobs with zstd compression over
//! the [`KvStore`] port. [`ContentHash`] provides the SHA-256 content hash.
//! The `rocksdb-backend` feature (enabled by default) provides embedded `RocksDB`.
//!
//! The deployment's CAS is never put on a transport a caller reaches. Services
//! read blobs in-process after `decide` on the graph node that references them;
//! the store itself has no tenant. The HTTP server and `HttpKvStore` client are
//! off by default, behind `http-server`, and only for the user's own use.

pub mod content;
pub mod error;
pub mod hash;
#[cfg(feature = "http-server")]
pub mod http;
#[cfg(feature = "memory-backend")]
pub mod memory;
#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb;
pub mod store;

pub use content::{CONTENT_PREFIX, ContentStore, KvContentStore, STORED_AT_PREFIX};
pub use error::KvError;
pub use hash::ContentHash;
#[cfg(feature = "http-server")]
pub use http::HttpKvStore;
#[cfg(feature = "memory-backend")]
pub use memory::MemoryStore;
#[cfg(feature = "rocksdb-backend")]
pub use rocksdb::RocksDbStore;
pub use store::KvStore;

// Compile and run the README example whenever the memory backend's doctests run.
#[cfg(all(doctest, feature = "memory-backend"))]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

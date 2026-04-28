//! Key-value storage trait and implementations.
//!
//! This crate provides the [`KvStore`] trait for abstract key-value operations.
//! Backends are feature-gated:
//! - `rocksdb-backend` (default): embedded `RocksDB` via [`RocksDbStore`]
//! - `sled-backend`: embedded sled KV store
//! - `memory-backend`: in-memory `KvStore` (dev/tests only)
//!
//! Also includes:
//! - Content-addressable storage via [`ContentStore`] with automatic zstd compression
//! - SHA-256 content hashing via [`ContentHash`]
//! - HTTP client backend via [`HttpKvStore`] for remote KV service

pub mod content;
pub mod error;
pub mod hash;
pub mod http;
#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb;
pub mod store;

pub use content::ContentStore;
pub use error::KvError;
pub use hash::ContentHash;
pub use http::HttpKvStore;
#[cfg(feature = "rocksdb-backend")]
pub use rocksdb::RocksDbStore;
pub use store::KvStore;

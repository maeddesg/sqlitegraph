//! Transactional Key-Value Store Module
//!
//! This module provides an in-memory key-value storage system built as a VIEW over
//! Native V2 storage. The KV store is NOT a separate storage system - it uses the
//! existing infrastructure (WAL, allocator, etc.) for persistence and transactional
//! guarantees.
//!
//! # Architecture
//!
//! - **In-memory HashMap storage**: Fast lookups with O(1) complexity
//! - **WAL integration**: KV operations are logged to WAL for durability (plan 02)
//! - **Snapshot isolation**: Versioned reads using snapshot_id (plan 03)
//! - **Lazy TTL cleanup**: Expired entries filtered on read, no background threads (plan 04)
//!
//! # Key Design Decisions
//!
//! 1. **No internal version counter**: Versions come from WAL commit LSN, matching
//!    the DeltaIndex pattern where `commit_lsn` is assigned by the WAL system.
//!
//! 2. **Byte keys**: Keys are `Vec<u8>` for maximum flexibility (strings, hashes,
//!    composite keys).
//!
//! 3. **Typed values**: `KvValue` enum supports common Rust types with JSON for
//!    complex structured data.
//!
//! # Module Organization
//!
//! - [`types`]: Core data structures (KvEntry, KvMetadata, KvValue, KvStoreError)
//! - [`store`]: KvStore implementation with HashMap storage
//! - [`wal`]: WAL integration helpers for KV persistence and recovery
//! - [`wal_tests`]: WAL integration tests (serialization, recovery, edge cases)
//! - [`tests`]: Unit tests for KV store operations
//! - [`ttl`]: TTL helpers and lazy cleanup utilities
//! - [`integration_tests`]: Comprehensive integration test suite

#[cfg(test)]
pub mod integration_tests;

#[cfg(test)]
pub mod snapshot_tests;

pub mod store;
#[cfg(test)]
pub mod tests;
pub mod ttl;
pub mod types;
pub mod wal;
pub mod wal_tests;

// Re-export public API
pub use store::KvStore;
pub use types::{KvEntry, KvMetadata, KvStoreError, KvValue};

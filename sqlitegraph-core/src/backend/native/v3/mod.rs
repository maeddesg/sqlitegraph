//! V3 native backend implementation
//!
//! This module implements V3 storage format with:
//! - B+Tree node index for unlimited capacity
//! - Page-based node storage
//! - Delta/varint compression (Phase 63b)
//! - Page allocator (Phase 64)
//! - Write-Ahead Logging for crash recovery (Phase 65)

pub mod allocator;
pub mod backend;
pub mod btree;
pub mod compression;
pub mod constants;
pub mod edge_compat;
pub mod header;
pub mod index;
pub mod kv_store;
pub mod node;
pub mod pubsub;
pub mod wal;
pub mod write_batch;

// Re-export V3 types
pub use header::{offset as header_offset, size as header_size, PersistentHeaderV3};

/// V3 magic bytes for file format identification
pub use constants::V3_MAGIC;

/// V3 format version
pub use constants::V3_FORMAT_VERSION;

/// V3 header size
pub use constants::V3_HEADER_SIZE;

// Re-export index types
pub use index::{IndexPage, IndexPageType};

// Re-export node types
pub use node::{NodeRecordV3, NodePage, FIXED_METADATA_SIZE, MAX_INLINE_DATA,
              PAGE_HEADER_SIZE as NODE_PAGE_HEADER_SIZE,
              MAX_PAGE_SIZE as NODE_PAGE_SIZE,
              USABLE_SIZE as NODE_PAGE_USABLE_SIZE,
              MAX_NODE_CAPACITY,
              TraversalCache, TraversalCacheBuilder,
              DEFAULT_CACHE_CAPACITY, MAX_CACHE_CAPACITY, MIN_CACHE_CAPACITY,
              NodeStore};

// Re-export allocator types
pub use allocator::{PageAllocator, PageState, FreePageHeader};

// Re-export edge compatibility types
pub use edge_compat::{Direction as EdgeDirection, PageType, V3EdgeCluster, V3EdgeStore};

// Re-export KV store types
pub use kv_store::{hash_key as kv_hash_key, KvEntry, KvMetadata, KvStore, KvValue};

// Re-export pub/sub types
pub use pubsub::{Publisher, PubSubEvent, PubSubEventType, SubscriberId, SubscriptionFilter};

// Re-export WAL types
pub use wal::{WALWriter, WALRecovery, WALRecoveryStats, V3WALRecord};

// Re-export V3Backend
pub use backend::V3Backend;

// Re-export WriteBatch
pub use write_batch::WriteBatch;

// Algorithm integration tests
#[cfg(test)]
mod tests;

// Lazy initialization tests
#[cfg(test)]
mod lazy_init_tests;

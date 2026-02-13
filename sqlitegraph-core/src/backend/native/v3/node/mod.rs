//! Node storage structure for V3 native backend
//!
//! This module defines the NodePage and NodeRecordV3 formats with full ID encoding.
//! Delta/varint compression is deferred to Phase 63b.
//!
//! ## Submodules
//!
//! - `record` - NodeRecordV3 with full ID encoding
//! - `page` - NodePage with delta/varint compression
//! - `store` - NodeStore with B+Tree lookup and TraversalCache
//! - `tests` - Comprehensive unit tests for NodeStore V3 components

pub mod page;
pub mod record;
pub mod store;

// Comprehensive tests module
#[cfg(test)]
pub mod tests;

// Re-export node record types
pub use record::{NodeRecordV3, FIXED_METADATA_SIZE, MAX_INLINE_DATA};

// Re-export node page types
pub use page::{NodePage, PAGE_HEADER_SIZE, MAX_PAGE_SIZE, USABLE_SIZE, MAX_NODE_CAPACITY};

// Re-export store types
pub use store::{
    NodeStore,
    TraversalCache, TraversalCacheBuilder,
    DEFAULT_CACHE_CAPACITY, MAX_CACHE_CAPACITY, MIN_CACHE_CAPACITY
};

/// Node record constants
pub mod constants {
    use super::record::constants::*;

    /// Fixed metadata size in bytes
    pub const FIXED_METADATA_SIZE: usize = 44;

    /// Maximum inline data size in bytes
    pub const MAX_INLINE_DATA: usize = 64;

    /// Node ID size (full i64, no delta)
    pub const ID_SIZE: usize = 8;

    /// Flags size (u32)
    pub const FLAGS_SIZE: usize = 4;

    /// Kind offset size (u16)
    pub const KIND_OFFSET_SIZE: usize = 2;

    /// Name offset size (u16)
    pub const NAME_OFFSET_SIZE: usize = 2;

    /// Data length size (u16)
    pub const DATA_LEN_SIZE: usize = 2;

    /// Outgoing cluster offset size (u64)
    pub const OUTGOING_OFFSET_SIZE: usize = 8;

    /// Outgoing edge count size (u32)
    pub const OUTGOING_COUNT_SIZE: usize = 4;

    /// Incoming cluster offset size (u64)
    pub const INCOMING_OFFSET_SIZE: usize = 8;

    /// Incoming edge count size (u32)
    pub const INCOMING_COUNT_SIZE: usize = 4;
}

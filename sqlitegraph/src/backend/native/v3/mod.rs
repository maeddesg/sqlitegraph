//! V3 native backend implementation
//!
//! This module implements V3 storage format with:
//! - B+Tree node index for unlimited capacity
//! - Page-based node storage
//! - Delta/varint compression (Phase 63b)
//! - Page allocator (Phase 64)
//! - Write-Ahead Logging for crash recovery (Phase 65)

pub mod allocator;
pub mod compression;
pub mod constants;
pub mod header;
pub mod index;
pub mod node;
pub mod wal;

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
              MAX_NODE_CAPACITY};

// Re-export allocator types
pub use allocator::{PageAllocator, PageState, FreePageHeader};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v3_module_exports() {
        // Verify key exports are available
        let _header = PersistentHeaderV3::new_v3();
        assert_eq!(V3_HEADER_SIZE, 112);
        assert_eq!(V3_FORMAT_VERSION, 4);
    }
}

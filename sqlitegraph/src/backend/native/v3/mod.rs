//! V3 native backend implementation
//!
//! This module implements the V3 storage format with:
//! - B+Tree node index for unlimited capacity
//! - Page-based node storage
//! - Fixed-size node records (compression deferred to Phase 63b)
//! - Append-only page allocation (free list deferred to Phase 64)

pub mod constants;
pub mod header;

// Re-export V3 types
pub use header::{offset as header_offset, size as header_size, PersistentHeaderV3};

/// V3 magic bytes for file format identification
pub use constants::V3_MAGIC;

/// V3 format version
pub use constants::V3_FORMAT_VERSION;

/// V3 header size
pub use constants::V3_HEADER_SIZE;

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

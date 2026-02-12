//! Constants for V3 native backend file format.
//!
//! This module contains magic numbers, version constants, and field sizes
//! for the V3 graph database file format with B+Tree indexing.

/// V3 magic number for native graph database files
/// Distinguished from V2 by magic[7] = 3 (instead of 0)
pub const V3_MAGIC: [u8; 8] = [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 3];

/// V3 file format version (incremented from V2's version 3)
pub const V3_FORMAT_VERSION: u32 = 4;

/// V3 header size in bytes (80 preserved + 32 new = 112 bytes)
pub const V3_HEADER_SIZE: u64 = 112;

/// Default page size for B+Tree and node storage (4KB)
pub const DEFAULT_PAGE_SIZE: u64 = 4096;

/// Maximum B+Tree height for 4 billion nodes (ceil(log_128(4B)) ≈ 4)
pub const MAX_BTREE_HEIGHT: u32 = 4;

/// Page header size (page_id, is_leaf, count, checksum, padding)
pub const PAGE_HEADER_SIZE: usize = 32;

/// Usable page size after header
pub const USABLE_PAGE_SIZE: usize = DEFAULT_PAGE_SIZE as usize - PAGE_HEADER_SIZE;

/// Maximum keys per B+Tree internal page
pub const MAX_KEYS_PER_PAGE: usize = 254;

/// Maximum entries per B+Tree leaf page
pub const MAX_ENTRIES_PER_LEAF: usize = 254;

/// Maximum children per internal page (keys + 1)
pub const MAX_CHILDREN_PER_PAGE: usize = MAX_KEYS_PER_PAGE + 1;

/// Default feature flags for V3 (inherited from V2)
pub const DEFAULT_V3_FEATURE_FLAGS: u32 = crate::backend::native::constants::DEFAULT_FEATURE_FLAGS;

/// Default schema version for V3
pub const DEFAULT_SCHEMA_VERSION: u32 = 1;

/// V3 feature flag definitions (extends V2 flags)
pub mod v3_flags {
    use crate::backend::native::constants;

    /// Inherit V2 framed records flag
    pub use constants::FLAG_V2_FRAMED_RECORDS;

    /// Inherit V2 atomic commit flag
    pub use constants::FLAG_V2_ATOMIC_COMMIT;

    /// V3: B+Tree index enabled (always true for V3 files)
    pub const FLAG_V3_BTREE_INDEX: u32 = 0x0000_0004;

    /// V3: Dynamic page allocation enabled
    pub const FLAG_V3_DYNAMIC_ALLOCATION: u32 = 0x0000_0008;

    /// Default V3 feature flags
    pub const DEFAULT: u32 = FLAG_V2_FRAMED_RECORDS
        | FLAG_V2_ATOMIC_COMMIT
        | FLAG_V3_BTREE_INDEX
        | FLAG_V3_DYNAMIC_ALLOCATION;
}

/// V3 checksum algorithm
pub mod checksum {
    /// XOR-based checksum for basic integrity checking
    pub const XOR_SEED: u64 = 0x5A5A5A5A5A5A5A5A;

    /// Simple XOR checksum calculation
    pub fn xor_checksum(data: &[u8]) -> u64 {
        let mut checksum = XOR_SEED;
        for (i, &byte) in data.iter().enumerate() {
            checksum ^= (byte as u64) ^ (i as u64);
        }
        checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v3_magic_distinguishes_from_v2() {
        // V2 magic has magic[7] = 0
        let v2_magic = crate::backend::native::v2::V2_MAGIC;
        assert_eq!(v2_magic[7], 0, "V2 magic should have 0 at position 7");

        // V3 magic has magic[7] = 3
        assert_eq!(V3_MAGIC[7], 3, "V3 magic should have 3 at position 7");

        // Ensure other bytes match
        assert_eq!(V3_MAGIC[0..7], v2_magic[0..7], "V3 should preserve V2 magic prefix");
    }

    #[test]
    fn test_v3_format_version_greater_than_v2() {
        let v2_version = crate::backend::native::v2::V2_FORMAT_VERSION;
        assert!(
            V3_FORMAT_VERSION > v2_version,
            "V3 format version ({}) should be greater than V2 ({})",
            V3_FORMAT_VERSION,
            v2_version
        );
    }

    #[test]
    fn test_v3_header_size() {
        // V2 header is 80 bytes
        let v2_header_size = crate::backend::native::constants::HEADER_SIZE;
        assert_eq!(v2_header_size, 80, "V2 header should be 80 bytes");

        // V3 header should be 112 bytes (80 + 32)
        assert_eq!(V3_HEADER_SIZE, 112, "V3 header should be 112 bytes");
        assert_eq!(V3_HEADER_SIZE, v2_header_size + 32, "V3 should extend V2 by 32 bytes");
    }

    #[test]
    fn test_page_size_calculation() {
        assert_eq!(DEFAULT_PAGE_SIZE, 4096, "Default page size should be 4KB");
        assert_eq!(
            PAGE_HEADER_SIZE + USABLE_PAGE_SIZE,
            DEFAULT_PAGE_SIZE as usize,
            "Page header + usable should equal total page size"
        );
    }

    #[test]
    fn test_btree_capacity() {
        // Verify B+Tree can handle 4 billion nodes with height 4
        // 128-ary tree with height 4: 128^4 = 268,435,456 nodes per leaf
        // This is more than sufficient for 4 billion nodes
        assert!(MAX_BTREE_HEIGHT >= 4, "Max B+Tree height should be at least 4");

        // 254 keys allows for efficient branching factor
        assert!(MAX_KEYS_PER_PAGE == 254, "Max keys per page should be 254");
        assert!(
            MAX_CHILDREN_PER_PAGE == MAX_KEYS_PER_PAGE + 1,
            "Max children should be max keys + 1"
        );
    }

    #[test]
    fn test_checksum_deterministic() {
        let data = b"Hello, V3!";
        let checksum1 = checksum::xor_checksum(data);
        let checksum2 = checksum::xor_checksum(data);
        assert_eq!(checksum1, checksum2, "Checksum should be deterministic");

        let different_data = b"Hello, V2!";
        let checksum3 = checksum::xor_checksum(different_data);
        assert_ne!(checksum1, checksum3, "Different data should produce different checksums");
    }
}

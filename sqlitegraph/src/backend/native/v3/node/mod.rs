//! Node record structure for V3 native backend
//!
//! This module defines the simplified NodeRecordV3 format with full ID encoding.
//! Delta/varint compression is deferred to Phase 63b.

pub mod record;

// Re-export node record types
pub use record::{NodeRecordV3, FIXED_METADATA_SIZE, MAX_INLINE_DATA};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_constants() {
        assert_eq!(constants::FIXED_METADATA_SIZE, 44);
        assert_eq!(constants::MAX_INLINE_DATA, 64);
        assert_eq!(constants::ID_SIZE, 8);
        assert_eq!(constants::FLAGS_SIZE, 4);
    }
}

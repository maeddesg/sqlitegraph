//! NodePage - Fixed-size page storage for NodeRecordV3
//!
//! This module implements the NodePage structure for storing fixed-size NodeRecordV3
//! records in a 4KB page. Uses full ID encoding (no delta).
//! Delta/varint compression is deferred to Phase 63b.

use crate::backend::native::NativeBackendError;
use crate::backend::native::NativeResult;
use crate::backend::native::v3::constants as v3_constants;
use super::record::NodeRecordV3;

/// NodePage header and layout constants
pub mod constants {
    use super::NodeRecordV3;

    /// Page header size in bytes
    ///
    /// Layout:
    /// - page_id: 8 bytes (u64)
    /// - next_page_id: 8 bytes (u64, overflow link)
    /// - node_count: 2 bytes (u16)
    /// - checksum: 4 bytes (u32)
    /// - reserved: 10 bytes (padding to 32 bytes)
    pub const PAGE_HEADER_SIZE: usize = 32;

    /// Fixed metadata size from NodeRecordV3
    pub const FIXED_METADATA_SIZE: usize = 44;

    /// Maximum inline data size from NodeRecordV3
    pub const MAX_INLINE_DATA: usize = 64;

    /// Page ID offset within header
    pub const PAGE_ID_OFFSET: usize = 0;

    /// Next page ID offset (overflow link)
    pub const NEXT_PAGE_ID_OFFSET: usize = 8;

    /// Node count offset (u16)
    pub const NODE_COUNT_OFFSET: usize = 16;

    /// Checksum offset (u32)
    pub const CHECKSUM_OFFSET: usize = 18;

    /// Total page size (4KB)
    pub const MAX_PAGE_SIZE: usize = 4096;

    /// Usable page size after header
    pub const USABLE_SIZE: usize = MAX_PAGE_SIZE - PAGE_HEADER_SIZE;

    /// Estimated fixed node slot size (conservative estimate for non-compressed records)
    /// 44 bytes metadata + 32 bytes average inline data = 76 bytes
    /// Rounded up to 80 bytes for safety
    pub const ESTIMATED_NODE_SLOT_SIZE: usize = 80;

    /// Fixed node capacity (conservative estimate)
    /// USABLE_SIZE (4064) / ESTIMATED_NODE_SLOT_SIZE (80) ≈ 50 nodes
    /// Using 50 as max capacity
    pub const MAX_NODE_CAPACITY: usize = USABLE_SIZE / ESTIMATED_NODE_SLOT_SIZE;

    // Field sizes
    pub const PAGE_ID_SIZE: usize = 8;
    pub const NEXT_PAGE_ID_SIZE: usize = 8;
    pub const NODE_COUNT_SIZE: usize = 2;
    pub const CHECKSUM_SIZE: usize = 4;
}

/// Re-export constants for convenience
pub use constants::{
    PAGE_HEADER_SIZE, MAX_PAGE_SIZE, USABLE_SIZE,
    MAX_NODE_CAPACITY, ESTIMATED_NODE_SLOT_SIZE
};

/// NodePage for storing fixed-size NodeRecordV3 records
///
/// Pages store nodes with full ID encoding (no delta compression).
/// Overflow pages are linked via next_page_id for large nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePage {
    /// Page ID for this page
    pub page_id: u64,

    /// Next page ID for overflow (0 if none)
    pub next_page_id: u64,

    /// Node records stored in this page
    pub nodes: Vec<NodeRecordV3>,

    /// Page checksum for validation
    pub checksum: u32,
}

impl NodePage {
    /// Create a new empty node page
    pub fn new(page_id: u64) -> Self {
        NodePage {
            page_id,
            next_page_id: 0,
            nodes: Vec::new(),
            checksum: 0,
        }
    }

    /// Create a new node page with the given capacity pre-allocated
    pub fn with_capacity(page_id: u64, capacity: usize) -> Self {
        NodePage {
            page_id,
            next_page_id: 0,
            nodes: Vec::with_capacity(capacity.min(MAX_NODE_CAPACITY)),
            checksum: 0,
        }
    }

    /// Get the number of nodes in this page
    pub fn node_count(&self) -> u16 {
        self.nodes.len() as u16
    }

    /// Check if the page has an overflow page
    pub fn has_overflow(&self) -> bool {
        self.next_page_id != 0
    }

    /// Check if the page is full (at estimated capacity)
    pub fn is_full(&self) -> bool {
        // Check if adding an average-sized node would exceed usable size
        self.estimated_used_size() + ESTIMATED_NODE_SLOT_SIZE > USABLE_SIZE
    }

    /// Check if the page is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Calculate the actual used size in bytes
    pub fn used_size(&self) -> usize {
        self.nodes.iter()
            .map(|n| n.serialized_size())
            .sum()
    }

    /// Estimate the used size (faster approximation)
    pub fn estimated_used_size(&self) -> usize {
        self.nodes.len() * ESTIMATED_NODE_SLOT_SIZE
    }

    /// Calculate remaining capacity in bytes
    pub fn remaining_capacity(&self) -> usize {
        USABLE_SIZE.saturating_sub(self.used_size())
    }

    /// Add a node to the page
    ///
    /// Returns an error if the node would cause the page to overflow.
    /// Accounts for the 2-byte record length prefix that will be added during packing.
    pub fn add_node(&mut self, node: NodeRecordV3) -> NativeResult<()> {
        // Check if adding this node would exceed page size
        // Include 2 bytes for record length prefix (stored during pack)
        let record_size = node.serialized_size();
        let new_size = self.used_size() + record_size + (self.nodes.len() + 1) * 2;

        // Actually we need to account for ALL record prefixes, not just the new one
        // Current used: sum of all node serialized sizes
        // We also need: (current_count + 1) * 2 for all record prefixes
        let current_with_prefixes = self.used_size() + self.nodes.len() * 2;
        let new_with_prefixes = current_with_prefixes + record_size + 2;

        if new_with_prefixes > USABLE_SIZE {
            return Err(NativeBackendError::InvalidHeader {
                field: "node_page".to_string(),
                reason: format!(
                    "adding node would exceed page capacity: {} + {} + 2 > {}",
                    current_with_prefixes,
                    record_size,
                    USABLE_SIZE
                ),
            });
        }

        self.nodes.push(node);
        Ok(())
    }

    /// Calculate checksum for page data
    fn calculate_checksum(&self, data: &[u8]) -> u32 {
        v3_constants::checksum::xor_checksum(data) as u32
    }

    /// Calculate checksum for header and node data (including record lengths)
    fn calculate_checksum_with_nodes(&self) -> u32 {
        let mut data = Vec::with_capacity(PAGE_HEADER_SIZE + self.used_size());

        // Serialize header
        data.extend_from_slice(&self.page_id.to_be_bytes());
        data.extend_from_slice(&self.next_page_id.to_be_bytes());
        data.extend_from_slice(&(self.nodes.len() as u16).to_be_bytes());
        data.extend_from_slice(&[0u8; 4]); // checksum placeholder
        data.extend_from_slice(&[0u8; 10]); // reserved

        // Serialize nodes WITH record length prefixes (matching pack format)
        for node in &self.nodes {
            let serialized = match node.serialize() {
                Ok(bytes) => bytes,
                Err(_) => continue, // Skip nodes that fail to serialize
            };
            // Include record length (u16 big-endian)
            data.extend_from_slice(&(serialized.len() as u16).to_be_bytes());
            // Include record data
            data.extend_from_slice(&serialized);
        }

        v3_constants::checksum::xor_checksum(&data) as u32
    }

    /// Pack the page into a 4KB byte array
    ///
    /// Serializes the page with big-endian encoding for cross-platform compatibility.
    pub fn pack(&self) -> NativeResult<[u8; MAX_PAGE_SIZE]> {
        let mut bytes = [0u8; MAX_PAGE_SIZE];

        // Write page header
        bytes[constants::PAGE_ID_OFFSET..constants::PAGE_ID_OFFSET + 8]
            .copy_from_slice(&self.page_id.to_be_bytes());

        bytes[constants::NEXT_PAGE_ID_OFFSET..constants::NEXT_PAGE_ID_OFFSET + 8]
            .copy_from_slice(&self.next_page_id.to_be_bytes());

        bytes[constants::NODE_COUNT_OFFSET..constants::NODE_COUNT_OFFSET + 2]
            .copy_from_slice(&(self.nodes.len() as u16).to_be_bytes());

        // Reserve space for checksum (calculated after data is written)
        let checksum_offset = constants::CHECKSUM_OFFSET;

        // Write node data
        let mut data_offset = PAGE_HEADER_SIZE;

        for node in &self.nodes {
            let serialized = node.serialize()?;

            if data_offset + serialized.len() > MAX_PAGE_SIZE {
                return Err(NativeBackendError::InvalidHeader {
                    field: "node_page".to_string(),
                    reason: format!(
                        "page overflow: offset {} + {} > {}",
                        data_offset,
                        serialized.len(),
                        MAX_PAGE_SIZE
                    ),
                });
            }

            // Store record length before record data
            if data_offset + 2 > MAX_PAGE_SIZE {
                return Err(NativeBackendError::InvalidHeader {
                    field: "node_page".to_string(),
                    reason: "no space for record length".to_string(),
                });
            }

            // Write record length as u16 big-endian
            let record_len = serialized.len() as u16;
            bytes[data_offset..data_offset + 2].copy_from_slice(&record_len.to_be_bytes());
            data_offset += 2;

            // Write record data
            bytes[data_offset..data_offset + serialized.len()].copy_from_slice(&serialized);
            data_offset += serialized.len();
        }

        // Calculate and write checksum (over header + node data)
        let checksum = self.calculate_checksum(&bytes[..data_offset]);
        bytes[checksum_offset..checksum_offset + 4].copy_from_slice(&checksum.to_be_bytes());

        Ok(bytes)
    }

    /// Unpack a page from a byte array
    ///
    /// Deserializes the page and validates the checksum.
    pub fn unpack(bytes: &[u8]) -> NativeResult<Self> {
        if bytes.len() < MAX_PAGE_SIZE {
            return Err(NativeBackendError::InvalidHeader {
                field: "node_page".to_string(),
                reason: format!(
                    "insufficient bytes: expected {}, found {}",
                    MAX_PAGE_SIZE,
                    bytes.len()
                ),
            });
        }

        // Read page header
        let page_id = u64::from_be_bytes(
            bytes[constants::PAGE_ID_OFFSET..constants::PAGE_ID_OFFSET + 8]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_page.page_id".to_string(),
                    reason: "invalid page_id bytes".to_string(),
                })?,
        );

        let next_page_id = u64::from_be_bytes(
            bytes[constants::NEXT_PAGE_ID_OFFSET..constants::NEXT_PAGE_ID_OFFSET + 8]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_page.next_page_id".to_string(),
                    reason: "invalid next_page_id bytes".to_string(),
                })?,
        );

        let node_count = u16::from_be_bytes(
            bytes[constants::NODE_COUNT_OFFSET..constants::NODE_COUNT_OFFSET + 2]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_page.node_count".to_string(),
                    reason: "invalid node_count bytes".to_string(),
                })?,
        ) as usize;

        let checksum = u32::from_be_bytes(
            bytes[constants::CHECKSUM_OFFSET..constants::CHECKSUM_OFFSET + 4]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_page.checksum".to_string(),
                    reason: "invalid checksum bytes".to_string(),
                })?,
        );

        // Read node data
        let mut nodes = Vec::with_capacity(node_count);
        let mut data_offset = PAGE_HEADER_SIZE;

        for _ in 0..node_count {
            // Read record length
            if data_offset + 2 > MAX_PAGE_SIZE {
                return Err(NativeBackendError::InvalidHeader {
                    field: "node_page".to_string(),
                    reason: "unexpected end of page reading record length".to_string(),
                });
            }

            let record_len = u16::from_be_bytes(
                bytes[data_offset..data_offset + 2].try_into().map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_page".to_string(),
                    reason: "invalid record length bytes".to_string(),
                })?,
            ) as usize;
            data_offset += 2;

            // Validate record length
            if data_offset + record_len > MAX_PAGE_SIZE {
                return Err(NativeBackendError::InvalidHeader {
                    field: "node_page".to_string(),
                    reason: format!(
                        "record exceeds page boundary: offset {} + {} > {}",
                        data_offset,
                        record_len,
                        MAX_PAGE_SIZE
                    ),
                });
            }

            // Read record data
            let record_data = &bytes[data_offset..data_offset + record_len];
            data_offset += record_len;

            // Deserialize node record
            let node = NodeRecordV3::deserialize(record_data)?;
            nodes.push(node);
        }

        // Verify checksum
        let mut page = NodePage {
            page_id,
            next_page_id,
            nodes,
            checksum,
        };

        // Calculate checksum on all data up to actual end
        let calculated_checksum = page.calculate_checksum_with_nodes();
        if calculated_checksum != checksum {
            return Err(NativeBackendError::InvalidHeader {
                field: "node_page_checksum".to_string(),
                reason: format!(
                    "checksum mismatch: expected {}, found {}",
                    calculated_checksum,
                    checksum
                ),
            });
        }

        Ok(page)
    }

    /// Get the total size this page would consume on disk
    pub fn disk_size(&self) -> usize {
        MAX_PAGE_SIZE
    }

    /// Calculate space efficiency (ratio of used to total space)
    pub fn space_efficiency(&self) -> f64 {
        if MAX_PAGE_SIZE == 0 {
            return 0.0;
        }
        (self.used_size() as f64) / (USABLE_SIZE as f64)
    }
}

/// Create a new empty page with default capacity
impl Default for NodePage {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::constants::*;
    use crate::backend::native::types::NodeFlags;

    #[test]
    fn test_constants() {
        assert_eq!(PAGE_HEADER_SIZE, 32);
        assert_eq!(MAX_PAGE_SIZE, 4096);
        assert_eq!(USABLE_SIZE, 4064);
        assert!(MAX_NODE_CAPACITY > 0);
        assert!(MAX_NODE_CAPACITY <= 100); // Should be reasonable
    }

    #[test]
    fn test_new_page() {
        let page = NodePage::new(42);
        assert_eq!(page.page_id, 42);
        assert_eq!(page.next_page_id, 0);
        assert_eq!(page.node_count(), 0);
        assert!(page.is_empty());
        assert!(!page.is_full());
        assert!(!page.has_overflow());
    }

    #[test]
    fn test_page_with_capacity() {
        let page = NodePage::with_capacity(100, 20);
        assert_eq!(page.page_id, 100);
        assert_eq!(page.node_count(), 0);
        assert!(page.nodes.capacity() >= 20);
    }

    #[test]
    fn test_add_node() {
        let page = &mut NodePage::new(1);

        let node = NodeRecordV3::new_inline(
            123,
            NodeFlags::empty(),
            10,
            20,
            b"test data".to_vec(),
            1000,
            5,
            2000,
            3,
        );

        assert!(page.add_node(node).is_ok());
        assert_eq!(page.node_count(), 1);
        assert!(!page.is_empty());
    }

    #[test]
    fn test_add_multiple_nodes() {
        let page = &mut NodePage::new(1);

        for i in 0..10 {
            let node = NodeRecordV3::new_inline(
                i,
                NodeFlags::empty(),
                i as u16 * 10,
                i as u16 * 20,
                vec![i as u8; 20],
                i as u64 * 1000,
                i as u32 * 5,
                i as u64 * 2000,
                i as u32 * 3,
            );
            assert!(page.add_node(node).is_ok());
        }

        assert_eq!(page.node_count(), 10);
    }

    #[test]
    fn test_pack_unpack_round_trip() {
        let page = &mut NodePage::new(1);

        // Add some nodes
        for i in 0..5 {
            let node = NodeRecordV3::new_inline(
                i * 100,
                NodeFlags::empty(),
                i as u16 * 10,
                i as u16 * 20,
                format!("node_{}_data", i).into_bytes(),
                i as u64 * 1000,
                i as u32 * 5,
                i as u64 * 2000,
                i as u32 * 3,
            );
            page.add_node(node).unwrap();
        }

        // Pack the page
        let bytes = page.pack().unwrap();
        assert_eq!(bytes.len(), MAX_PAGE_SIZE);

        // Unpack and verify
        let restored = NodePage::unpack(&bytes).unwrap();
        assert_eq!(restored.page_id, 1);
        assert_eq!(restored.node_count(), 5);
        assert_eq!(restored.nodes.len(), 5);

        // Verify node data
        for (i, node) in restored.nodes.iter().enumerate() {
            assert_eq!(node.id(), (i * 100) as i64);
            assert_eq!(node.kind_offset, (i * 10) as u16);
            assert_eq!(node.name_offset, (i * 20) as u16);
        }
    }

    #[test]
    fn test_pack_unpack_preserves_all_fields() {
        let page = &mut NodePage::new(99);
        page.next_page_id = 200;

        let node = NodeRecordV3::new_inline(
            -12345,
            NodeFlags::DELETED,
            42,
            84,
            b"Test node data for full preservation".to_vec(),
            0x123456789ABCDEF0,
            42,
            0xFEDCBA9876543210,
            99,
        );

        page.add_node(node).unwrap();

        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        assert_eq!(restored.page_id, 99);
        assert_eq!(restored.next_page_id, 200);
        assert_eq!(restored.node_count(), 1);

        let restored_node = &restored.nodes[0];
        assert_eq!(restored_node.id(), -12345);
        assert_eq!(restored_node.flags, NodeFlags::DELETED);
        assert_eq!(restored_node.kind_offset, 42);
        assert_eq!(restored_node.name_offset, 84);
        assert_eq!(restored_node.data_inline, Some(b"Test node data for full preservation".to_vec()));
        assert_eq!(restored_node.outgoing_cluster_offset, 0x123456789ABCDEF0);
        assert_eq!(restored_node.outgoing_edge_count, 42);
        assert_eq!(restored_node.incoming_cluster_offset, 0xFEDCBA9876543210);
        assert_eq!(restored_node.incoming_edge_count, 99);
    }

    #[test]
    fn test_checksum_validation() {
        let page = &mut NodePage::new(1);

        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            b"data".to_vec(),
            0,
            0,
            0,
            0,
        );
        page.add_node(node).unwrap();

        let bytes = page.pack().unwrap();

        // Valid unpack should work
        assert!(NodePage::unpack(&bytes).is_ok());

        // Corrupt the checksum
        let mut corrupted = bytes.clone();
        corrupted[constants::CHECKSUM_OFFSET] ^= 0xFF;

        // Should fail checksum validation
        assert!(NodePage::unpack(&corrupted).is_err());
    }

    #[test]
    fn test_empty_page_round_trip() {
        let page = NodePage::new(0);

        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        assert_eq!(restored.page_id, 0);
        assert_eq!(restored.node_count(), 0);
        assert!(restored.is_empty());
    }

    #[test]
    fn test_overflow_page_link() {
        let mut page = NodePage::new(10);
        page.next_page_id = 20;

        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        assert_eq!(restored.next_page_id, 20);
        assert!(restored.has_overflow());
    }

    #[test]
    fn test_used_size_calculation() {
        let page = &mut NodePage::new(1);

        let empty_node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            vec![],
            0,
            0,
            0,
            0,
        );

        page.add_node(empty_node).unwrap();
        assert_eq!(page.used_size(), FIXED_METADATA_SIZE);

        let node_with_data = NodeRecordV3::new_inline(
            2,
            NodeFlags::empty(),
            0,
            0,
            vec![1u8; 32],
            0,
            0,
            0,
            0,
        );

        page.add_node(node_with_data).unwrap();
        assert_eq!(page.used_size(), FIXED_METADATA_SIZE * 2 + 32);
    }

    #[test]
    fn test_remaining_capacity() {
        let page = &mut NodePage::new(1);
        assert_eq!(page.remaining_capacity(), USABLE_SIZE);

        // Use 50 bytes which is less than MAX_INLINE_DATA (64)
        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            vec![0u8; 50],
            0,
            0,
            0,
            0,
        );

        page.add_node(node).unwrap();
        assert!(page.remaining_capacity() < USABLE_SIZE);
    }

    #[test]
    fn test_space_efficiency() {
        let page = &mut NodePage::new(1);

        // Empty page has 0 efficiency
        assert_eq!(page.space_efficiency(), 0.0);

        // Add a node that uses some space
        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            vec![1u8; FIXED_METADATA_SIZE],
            0,
            0,
            0,
            0,
        );

        page.add_node(node).unwrap();

        // Efficiency should be > 0 and < 1
        let efficiency = page.space_efficiency();
        assert!(efficiency > 0.0);
        assert!(efficiency < 1.0);
    }

    #[test]
    fn test_disk_size() {
        let page = NodePage::new(1);
        assert_eq!(page.disk_size(), MAX_PAGE_SIZE);
    }

    #[test]
    fn test_pack_returns_exact_size() {
        let page = NodePage::new(1);
        let bytes = page.pack().unwrap();
        assert_eq!(bytes.len(), MAX_PAGE_SIZE);
    }

    #[test]
    fn test_insufficient_bytes_error() {
        let short_data = vec![0u8; 100];
        let result = NodePage::unpack(&short_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_full_id_encoding_preserved() {
        let page = &mut NodePage::new(1);

        // Test with various ID values including negative
        let test_ids = vec![0, 1, -1, 1000000, -1000000, i64::MAX, i64::MIN];

        for id in &test_ids {
            let node = NodeRecordV3::new_inline(
                *id,
                NodeFlags::empty(),
                0,
                0,
                vec![],
                0,
                0,
                0,
                0,
            );
            page.add_node(node).unwrap();
        }

        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        for (i, node) in restored.nodes.iter().enumerate() {
            assert_eq!(
                node.id(),
                test_ids[i],
                "ID at index {} not preserved",
                i
            );
        }
    }

    #[test]
    fn test_page_capacity_limits() {
        let page = &mut NodePage::new(1);

        // Keep adding nodes until page would be full
        // Each node: 44 (metadata) + 50 (data) = 94 bytes, + 2 byte length prefix = 96 bytes
        // 4064 / 96 = 42.3, so we should fit 42 nodes
        for count in 0..50 {
            let node = NodeRecordV3::new_inline(
                count as i64,
                NodeFlags::empty(),
                0,
                0,
                vec![count as u8; 50], // 50 bytes of data
                0,
                0,
                0,
                0,
            );

            // Try to add the node - it may fail near capacity
            if page.add_node(node).is_err() {
                // Page is full
                break;
            }
        }

        // Verify round-trip works
        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        // Should fit at least 20 nodes
        assert!(restored.node_count() >= 20, "Should fit at least 20 nodes, got {}", restored.node_count());
    }

    #[test]
    fn test_max_inline_data_node() {
        let page = &mut NodePage::new(1);

        // Add node with max inline data
        let max_data = vec![0xFFu8; MAX_INLINE_DATA];
        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            max_data,
            0,
            0,
            0,
            0,
        );

        page.add_node(node).unwrap();

        // Verify round-trip preserves max inline data
        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        assert_eq!(restored.node_count(), 1);
        assert_eq!(restored.nodes[0].data_inline.as_ref().unwrap().len(), MAX_INLINE_DATA);
    }

    #[test]
    fn test_external_node_record() {
        let page = &mut NodePage::new(1);

        // External data node (data > MAX_INLINE_DATA)
        let node = NodeRecordV3::new_external(
            1,
            NodeFlags::empty(),
            0,
            0,
            5000,    // external data offset
            200,     // data length
            0,
            0,
            0,
            0,
        );

        page.add_node(node).unwrap();

        let bytes = page.pack().unwrap();
        let restored = NodePage::unpack(&bytes).unwrap();

        assert_eq!(restored.node_count(), 1);
        assert!(restored.nodes[0].is_external());
        assert_eq!(restored.nodes[0].data_len(), 200);
    }

    #[test]
    fn test_multiple_page_ids() {
        for page_id in vec![0, 1, 100, u64::MAX] {
            let page = NodePage::new(page_id);
            assert_eq!(page.page_id, page_id);

            let bytes = page.pack().unwrap();
            let restored = NodePage::unpack(&bytes).unwrap();
            assert_eq!(restored.page_id, page_id);
        }
    }

    #[test]
    fn test_default_trait() {
        let page = NodePage::default();
        assert_eq!(page.page_id, 0);
        assert!(page.is_empty());
    }
}

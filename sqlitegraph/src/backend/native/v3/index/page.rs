//! B+Tree index page structure
//!
//! Defines the IndexPage enum with Internal and Leaf variants for B+Tree node index.
//! Uses big-endian serialization for cross-platform compatibility.

use crate::backend::native::NativeBackendError;
use crate::backend::native::NativeResult;
use crate::backend::native::v3::constants as v3_constants;

/// Page header offset and size constants
pub mod constants {
    /// Page header size in bytes
    pub const PAGE_HEADER_SIZE: usize = 32;

    /// Page ID offset within header
    pub const PAGE_ID_OFFSET: usize = 0;

    /// Is leaf flag offset (1 byte, 1 = leaf, 0 = internal)
    pub const IS_LEAF_OFFSET: usize = 8;

    /// Count offset (number of keys or entries, u16)
    pub const COUNT_OFFSET: usize = 9;

    /// Checksum offset (u32)
    pub const CHECKSUM_OFFSET: usize = 11;

    /// Reserved/padding offset
    pub const PADDING_OFFSET: usize = 15;

    /// Key/data start offset after header
    pub const DATA_START_OFFSET: usize = PAGE_HEADER_SIZE;
}

/// Maximum keys per internal page
pub const MAX_KEYS: usize = 254;

/// Maximum entries per leaf page
pub const MAX_ENTRIES: usize = 254;

/// Maximum children per internal page
pub const MAX_CHILDREN: usize = MAX_KEYS + 1;

/// Key size in bytes (u64 node_id)
pub const KEY_SIZE: usize = 8;

/// Page ID size in bytes (u64)
pub const PAGE_ID_SIZE: usize = 8;

/// Entry size in bytes (node_id + page_id)
pub const ENTRY_SIZE: usize = KEY_SIZE + PAGE_ID_SIZE;

/// B+Tree index page type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexPageType {
    /// Internal node with keys and child pointers
    Internal,
    /// Leaf node with (node_id, page_id) entries
    Leaf,
}

/// B+Tree index page
///
/// Internal pages contain split keys and child page pointers for tree navigation.
/// Leaf pages contain the actual (node_id, page_id) mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexPage {
    /// Internal index page with keys and child pointers
    Internal {
        /// Page ID for this page
        page_id: u64,
        /// Split keys (max 254) - keys[i] is the minimum key in child i+1
        keys: Vec<u64>,
        /// Child page IDs (max 255, one more than keys)
        children: Vec<u64>,
        /// Page checksum for validation
        checksum: u32,
    },
    /// Leaf index page with node ID to page ID mappings
    Leaf {
        /// Page ID for this page
        page_id: u64,
        /// (node_id, page_id) entries (max 254)
        entries: Vec<(u64, u64)>,
        /// Next leaf page ID (0 if none)
        next_leaf: u64,
        /// Page checksum for validation
        checksum: u32,
    },
}

impl IndexPage {
    /// Get the page ID for this page
    pub fn page_id(&self) -> u64 {
        match self {
            IndexPage::Internal { page_id, .. } => *page_id,
            IndexPage::Leaf { page_id, .. } => *page_id,
        }
    }

    /// Get the page type
    pub fn page_type(&self) -> IndexPageType {
        match self {
            IndexPage::Internal { .. } => IndexPageType::Internal,
            IndexPage::Leaf { .. } => IndexPageType::Leaf,
        }
    }

    /// Get the number of keys (internal) or entries (leaf)
    pub fn count(&self) -> usize {
        match self {
            IndexPage::Internal { keys, .. } => keys.len(),
            IndexPage::Leaf { entries, .. } => entries.len(),
        }
    }

    /// Calculate checksum for page data
    fn calculate_checksum(&self, data: &[u8]) -> u32 {
        v3_constants::checksum::xor_checksum(data) as u32
    }

    /// Create a new empty internal page
    pub fn new_internal(page_id: u64) -> Self {
        IndexPage::Internal {
            page_id,
            keys: Vec::new(),
            children: Vec::new(),
            checksum: 0,
        }
    }

    /// Create a new empty leaf page
    pub fn new_leaf(page_id: u64) -> Self {
        IndexPage::Leaf {
            page_id,
            entries: Vec::new(),
            next_leaf: 0,
            checksum: 0,
        }
    }

    /// Check if internal page is full (at max capacity)
    pub fn is_full_internal(&self) -> bool {
        match self {
            IndexPage::Internal { keys, .. } => keys.len() >= MAX_KEYS,
            _ => false,
        }
    }

    /// Check if leaf page is full (at max capacity)
    pub fn is_full_leaf(&self) -> bool {
        match self {
            IndexPage::Leaf { entries, .. } => entries.len() >= MAX_ENTRIES,
            _ => false,
        }
    }

    /// Pack the page into a 4KB byte array
    ///
    /// Serializes the page with big-endian encoding for cross-platform compatibility.
    pub fn pack(&self) -> NativeResult<[u8; 4096]> {
        let mut bytes = [0u8; 4096];

        // Write page header
        bytes[constants::PAGE_ID_OFFSET..constants::PAGE_ID_OFFSET + 8]
            .copy_from_slice(&self.page_id().to_be_bytes());

        // Write is_leaf flag (1 byte)
        match self {
            IndexPage::Internal { .. } => {
                bytes[constants::IS_LEAF_OFFSET] = 0;
            }
            IndexPage::Leaf { .. } => {
                bytes[constants::IS_LEAF_OFFSET] = 1;
            }
        }

        // Write count (u16)
        let count = self.count() as u16;
        bytes[constants::COUNT_OFFSET..constants::COUNT_OFFSET + 2]
            .copy_from_slice(&count.to_be_bytes());

        // Reserve space for checksum (will be calculated at end)
        let checksum_offset = constants::CHECKSUM_OFFSET;

        // Write data based on page type
        let mut data_offset = constants::DATA_START_OFFSET;

        match self {
            IndexPage::Internal { keys, children, .. } => {
                // Validate invariants
                // Special case: Empty page (0 keys, 0 children) is allowed for newly created pages
                // When the page has keys, children must be keys.len() + 1
                if !keys.is_empty() && children.len() != keys.len() + 1 {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "internal_page".to_string(),
                        reason: format!(
                            "children count ({}) must be keys count ({}) + 1",
                            children.len(),
                            keys.len()
                        ),
                    });
                }
                // Non-empty page must have correct child count
                if keys.is_empty() && children.len() > 1 {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "internal_page".to_string(),
                        reason: format!(
                            "empty page has too many children: {}",
                            children.len()
                        ),
                    });
                }

                if keys.len() > MAX_KEYS {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "internal_page".to_string(),
                        reason: format!("keys count ({}) exceeds max ({})", keys.len(), MAX_KEYS),
                    });
                }

                // Write keys
                for &key in keys {
                    if data_offset + KEY_SIZE > 4096 {
                        return Err(NativeBackendError::InvalidHeader {
                            field: "internal_page".to_string(),
                            reason: "page overflow writing keys".to_string(),
                        });
                    }
                    bytes[data_offset..data_offset + KEY_SIZE].copy_from_slice(&key.to_be_bytes());
                    data_offset += KEY_SIZE;
                }

                // Write children
                for &child in children {
                    if data_offset + PAGE_ID_SIZE > 4096 {
                        return Err(NativeBackendError::InvalidHeader {
                            field: "internal_page".to_string(),
                            reason: "page overflow writing children".to_string(),
                        });
                    }
                    bytes[data_offset..data_offset + PAGE_ID_SIZE]
                        .copy_from_slice(&child.to_be_bytes());
                    data_offset += PAGE_ID_SIZE;
                }
            }
            IndexPage::Leaf { entries, next_leaf, .. } => {
                if entries.len() > MAX_ENTRIES {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "leaf_page".to_string(),
                        reason: format!(
                            "entries count ({}) exceeds max ({})",
                            entries.len(),
                            MAX_ENTRIES
                        ),
                    });
                }

                // Write entries (node_id, page_id) pairs
                for &(node_id, page_id) in entries {
                    if data_offset + ENTRY_SIZE > 4096 {
                        return Err(NativeBackendError::InvalidHeader {
                            field: "leaf_page".to_string(),
                            reason: "page overflow writing entries".to_string(),
                        });
                    }
                    bytes[data_offset..data_offset + KEY_SIZE].copy_from_slice(&node_id.to_be_bytes());
                    data_offset += KEY_SIZE;
                    bytes[data_offset..data_offset + PAGE_ID_SIZE]
                        .copy_from_slice(&page_id.to_be_bytes());
                    data_offset += PAGE_ID_SIZE;
                }

                // Write next_leaf pointer
                if data_offset + PAGE_ID_SIZE > 4096 {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "leaf_page".to_string(),
                        reason: "page overflow writing next_leaf".to_string(),
                    });
                }
                bytes[data_offset..data_offset + PAGE_ID_SIZE].copy_from_slice(&next_leaf.to_be_bytes());
                data_offset += PAGE_ID_SIZE;
            }
        }

        // Calculate and write checksum
        let checksum = self.calculate_checksum(&bytes[..data_offset]);
        bytes[checksum_offset..checksum_offset + 4].copy_from_slice(&checksum.to_be_bytes());

        Ok(bytes)
    }

    /// Unpack a page from a byte array
    ///
    /// Deserializes the page and validates the checksum.
    pub fn unpack(bytes: &[u8]) -> NativeResult<Self> {
        if bytes.len() < 4096 {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_data".to_string(),
                reason: format!("insufficient bytes: expected 4096, found {}", bytes.len()),
            });
        }

        // Read page header
        let page_id = u64::from_be_bytes(
            bytes[constants::PAGE_ID_OFFSET..constants::PAGE_ID_OFFSET + 8]
                .try_into()
                .unwrap(),
        );

        let is_leaf = bytes[constants::IS_LEAF_OFFSET] == 1;

        let count = u16::from_be_bytes(
            bytes[constants::COUNT_OFFSET..constants::COUNT_OFFSET + 2]
                .try_into()
                .unwrap(),
        ) as usize;

        let checksum = u32::from_be_bytes(
            bytes[constants::CHECKSUM_OFFSET..constants::CHECKSUM_OFFSET + 4]
                .try_into()
                .unwrap(),
        );

        // Read data based on page type
        let mut data_offset = constants::DATA_START_OFFSET;

        if is_leaf {
            // Leaf page: read (node_id, page_id) entries
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                if data_offset + ENTRY_SIZE > 4096 {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "leaf_page".to_string(),
                        reason: "page overflow reading entries".to_string(),
                    });
                }
                let node_id = u64::from_be_bytes(
                    bytes[data_offset..data_offset + KEY_SIZE].try_into().unwrap(),
                );
                data_offset += KEY_SIZE;
                let page_id = u64::from_be_bytes(
                    bytes[data_offset..data_offset + PAGE_ID_SIZE]
                        .try_into()
                        .unwrap(),
                );
                data_offset += PAGE_ID_SIZE;
                entries.push((node_id, page_id));
            }

            // Read next_leaf pointer
            let next_leaf = if data_offset + PAGE_ID_SIZE <= 4096 {
                let ptr = u64::from_be_bytes(
                    bytes[data_offset..data_offset + PAGE_ID_SIZE]
                        .try_into()
                        .unwrap(),
                );
                data_offset += PAGE_ID_SIZE;
                ptr
            } else {
                return Err(NativeBackendError::InvalidHeader {
                    field: "leaf_page".to_string(),
                    reason: "missing next_leaf pointer".to_string(),
                });
            };

            // Verify checksum
            let calculated_checksum = Self::calculate_checksum_leaf(page_id, &entries, next_leaf);
            if calculated_checksum != checksum {
                return Err(NativeBackendError::InvalidHeader {
                    field: "leaf_checksum".to_string(),
                    reason: format!(
                        "checksum mismatch: expected {}, found {}",
                        calculated_checksum, checksum
                    ),
                });
            }

            Ok(IndexPage::Leaf {
                page_id,
                entries,
                next_leaf,
                checksum,
            })
        } else {
            // Internal page: read keys and children
            let mut keys = Vec::with_capacity(count);
            for _ in 0..count {
                if data_offset + KEY_SIZE > 4096 {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "internal_page".to_string(),
                        reason: "page overflow reading keys".to_string(),
                    });
                }
                let key = u64::from_be_bytes(
                    bytes[data_offset..data_offset + KEY_SIZE].try_into().unwrap(),
                );
                data_offset += KEY_SIZE;
                keys.push(key);
            }

            // Children count is keys + 1
            let child_count = count + 1;
            let mut children = Vec::with_capacity(child_count);
            for _ in 0..child_count {
                if data_offset + PAGE_ID_SIZE > 4096 {
                    return Err(NativeBackendError::InvalidHeader {
                        field: "internal_page".to_string(),
                        reason: "page overflow reading children".to_string(),
                    });
                }
                let child = u64::from_be_bytes(
                    bytes[data_offset..data_offset + PAGE_ID_SIZE]
                        .try_into()
                        .unwrap(),
                );
                data_offset += PAGE_ID_SIZE;
                children.push(child);
            }

            // Verify checksum
            let calculated_checksum = Self::calculate_checksum_internal(page_id, &keys, &children);
            if calculated_checksum != checksum {
                return Err(NativeBackendError::InvalidHeader {
                    field: "internal_checksum".to_string(),
                    reason: format!(
                        "checksum mismatch: expected {}, found {}",
                        calculated_checksum, checksum
                    ),
                });
            }

            Ok(IndexPage::Internal {
                page_id,
                keys,
                children,
                checksum,
            })
        }
    }

    /// Calculate checksum for leaf page
    fn calculate_checksum_leaf(page_id: u64, entries: &[(u64, u64)], next_leaf: u64) -> u32 {
        let mut data = Vec::with_capacity(4096);
        data.extend_from_slice(&page_id.to_be_bytes());
        data.push(1); // is_leaf
        data.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        data.extend_from_slice(&[0u8; 4]); // reserved for checksum
        data.extend_from_slice(&[0u8; 17]); // padding to 32 bytes

        for &(node_id, page_id) in entries {
            data.extend_from_slice(&node_id.to_be_bytes());
            data.extend_from_slice(&page_id.to_be_bytes());
        }
        data.extend_from_slice(&next_leaf.to_be_bytes());

        v3_constants::checksum::xor_checksum(&data) as u32
    }

    /// Calculate checksum for internal page
    fn calculate_checksum_internal(page_id: u64, keys: &[u64], children: &[u64]) -> u32 {
        let mut data = Vec::with_capacity(4096);
        data.extend_from_slice(&page_id.to_be_bytes());
        data.push(0); // is_leaf (internal)
        data.extend_from_slice(&(keys.len() as u16).to_be_bytes());
        data.extend_from_slice(&[0u8; 4]); // reserved for checksum
        data.extend_from_slice(&[0u8; 17]); // padding to 32 bytes

        for &key in keys {
            data.extend_from_slice(&key.to_be_bytes());
        }
        for &child in children {
            data.extend_from_slice(&child.to_be_bytes());
        }

        v3_constants::checksum::xor_checksum(&data) as u32
    }

    /// Search for a node_id in a leaf page using binary search
    ///
    /// Returns the index where the node_id is found, or Err(idx) with the insertion point.
    pub fn binary_search_leaf(entries: &[(u64, u64)], target: u64) -> Result<usize, usize> {
        entries.binary_search_by_key(&target, |&(node_id, _)| node_id)
    }

    /// Find the appropriate child index for a key in an internal page
    ///
    /// Returns the index of the child that should contain the target key.
    pub fn find_child_index(keys: &[u64], target: u64) -> usize {
        match keys.binary_search(&target) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(constants::PAGE_HEADER_SIZE, 32);
        assert_eq!(MAX_KEYS, 254);
        assert_eq!(MAX_ENTRIES, 254);
        assert_eq!(MAX_CHILDREN, 255);
        assert_eq!(KEY_SIZE, 8);
        assert_eq!(PAGE_ID_SIZE, 8);
        assert_eq!(ENTRY_SIZE, 16);
    }

    #[test]
    fn test_new_internal_page() {
        let page = IndexPage::new_internal(42);
        assert_eq!(page.page_id(), 42);
        assert_eq!(page.page_type(), IndexPageType::Internal);
        assert_eq!(page.count(), 0);
        assert!(!page.is_full_internal());
    }

    #[test]
    fn test_new_leaf_page() {
        let page = IndexPage::new_leaf(99);
        assert_eq!(page.page_id(), 99);
        assert_eq!(page.page_type(), IndexPageType::Leaf);
        assert_eq!(page.count(), 0);
        assert!(!page.is_full_leaf());
    }

    #[test]
    fn test_internal_page_round_trip() {
        let original = IndexPage::Internal {
            page_id: 1,
            keys: vec![100, 200, 300],
            children: vec![10, 11, 12, 13],
            checksum: 0,
        };

        let bytes = original.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();

        // Compare fields that should be preserved
        match restored {
            IndexPage::Internal { page_id, keys, children, .. } => {
                assert_eq!(page_id, 1);
                assert_eq!(keys, vec![100, 200, 300]);
                assert_eq!(children, vec![10, 11, 12, 13]);
            }
            _ => panic!("Expected Internal page"),
        }
    }

    #[test]
    fn test_leaf_page_round_trip() {
        let original = IndexPage::Leaf {
            page_id: 2,
            entries: vec![(1, 10), (5, 11), (9, 12)],
            next_leaf: 3,
            checksum: 0,
        };

        let bytes = original.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();

        // Compare fields that should be preserved
        match restored {
            IndexPage::Leaf { page_id, entries, next_leaf, .. } => {
                assert_eq!(page_id, 2);
                assert_eq!(entries, vec![(1, 10), (5, 11), (9, 12)]);
                assert_eq!(next_leaf, 3);
            }
            _ => panic!("Expected Leaf page"),
        }
    }

    #[test]
    fn test_full_internal_page_round_trip() {
        let mut keys = Vec::with_capacity(MAX_KEYS);
        let mut children = Vec::with_capacity(MAX_CHILDREN);

        // Calculate max keys that fit: (4064 - 8) / 16 = 253
        // Each key (8) + child (8) = 16 bytes, minus 8 bytes reserved for count/flags
        // So 253 keys + 254 children fits in 4064 usable bytes
        let max_fit = 253;
        for i in 0..max_fit {
            keys.push((i as u64) * 100 + 100);
        }
        for i in 0..(max_fit + 1) {
            children.push(i as u64);
        }

        let original = IndexPage::Internal {
            page_id: 5,
            keys,
            children,
            checksum: 0,
        };

        let bytes = original.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();

        assert_eq!(restored.count(), max_fit);
        match restored {
            IndexPage::Internal { keys: k, children: c, .. } => {
                assert_eq!(k.len(), max_fit);
                assert_eq!(c.len(), max_fit + 1);
            }
            _ => panic!("Expected internal page"),
        }
    }

    #[test]
    fn test_full_leaf_page_round_trip() {
        let mut entries = Vec::with_capacity(MAX_ENTRIES);

        // Use MAX_ENTRIES - 1 to leave room for next_leaf pointer
        // Each entry is 16 bytes, so 253 entries = 4048 bytes
        // Plus 8 bytes for next_leaf = 4056 bytes, which fits in 4064 usable
        for i in 0..(MAX_ENTRIES - 1) {
            entries.push((i as u64, (i as u64) * 100));
        }

        let original = IndexPage::Leaf {
            page_id: 6,
            entries,
            next_leaf: 0,
            checksum: 0,
        };

        let bytes = original.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();

        assert_eq!(restored.count(), MAX_ENTRIES - 1);
        match restored {
            IndexPage::Leaf { entries: e, .. } => {
                assert_eq!(e.len(), MAX_ENTRIES - 1);
            }
            _ => panic!("Expected leaf page"),
        }
    }

    #[test]
    fn test_binary_search_leaf_found() {
        let entries = vec![(10, 1), (20, 2), (30, 3), (40, 4), (50, 5)];
        let result = IndexPage::binary_search_leaf(&entries, 30);
        assert_eq!(result, Ok(2));
    }

    #[test]
    fn test_binary_search_leaf_not_found() {
        let entries = vec![(10, 1), (20, 2), (40, 4), (50, 5)];
        let result = IndexPage::binary_search_leaf(&entries, 30);
        assert_eq!(result, Err(2)); // Should insert at index 2
    }

    #[test]
    fn test_find_child_index() {
        let keys = vec![100, 200, 300, 400];

        // Exact match: go to right child (idx + 1)
        assert_eq!(IndexPage::find_child_index(&keys, 200), 2);

        // Between keys: go to left child at that index
        assert_eq!(IndexPage::find_child_index(&keys, 150), 1);
        assert_eq!(IndexPage::find_child_index(&keys, 50), 0);
        assert_eq!(IndexPage::find_child_index(&keys, 500), 4);
    }

    #[test]
    fn test_checksum_validation_internal() {
        // Create a valid page
        let page = IndexPage::Internal {
            page_id: 1,
            keys: vec![100, 200],
            children: vec![10, 11, 12],
            checksum: 0, // Will be calculated in pack()
        };

        let bytes = page.pack().unwrap();

        // Valid unpack should work
        assert!(IndexPage::unpack(&bytes).is_ok());

        // Corrupt the checksum
        let mut corrupted = bytes.clone();
        corrupted[constants::CHECKSUM_OFFSET] ^= 0xFF;

        // Should fail checksum validation
        assert!(IndexPage::unpack(&corrupted).is_err());
    }

    #[test]
    fn test_checksum_validation_leaf() {
        let page = IndexPage::Leaf {
            page_id: 1,
            entries: vec![(1, 10), (2, 20)],
            next_leaf: 0,
            checksum: 0,
        };

        let bytes = page.pack().unwrap();

        // Valid unpack should work
        assert!(IndexPage::unpack(&bytes).is_ok());

        // Corrupt the checksum
        let mut corrupted = bytes.clone();
        corrupted[constants::CHECKSUM_OFFSET] ^= 0xFF;

        // Should fail checksum validation
        assert!(IndexPage::unpack(&corrupted).is_err());
    }

    #[test]
    fn test_invalid_children_count() {
        let page = IndexPage::Internal {
            page_id: 1,
            keys: vec![100, 200],
            children: vec![10, 11], // Should be 3 children for 2 keys
            checksum: 0,
        };

        assert!(page.pack().is_err());
    }

    #[test]
    fn test_empty_pages_round_trip() {
        let internal = IndexPage::new_internal(0);
        let bytes = internal.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();
        assert_eq!(restored.page_id(), 0);
        assert_eq!(restored.count(), 0);

        let leaf = IndexPage::new_leaf(0);
        let bytes = leaf.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();
        assert_eq!(restored.page_id(), 0);
        assert_eq!(restored.count(), 0);
    }

    #[test]
    fn test_leaf_with_next_pointer() {
        let page = IndexPage::Leaf {
            page_id: 10,
            entries: vec![(1, 100), (2, 200)],
            next_leaf: 11,
            checksum: 0,
        };

        let bytes = page.pack().unwrap();
        let restored = IndexPage::unpack(&bytes).unwrap();

        match restored {
            IndexPage::Leaf { next_leaf, .. } => {
                assert_eq!(next_leaf, 11);
            }
            _ => panic!("Expected leaf page"),
        }
    }
}

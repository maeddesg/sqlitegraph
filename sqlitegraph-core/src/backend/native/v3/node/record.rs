//! NodeRecordV3 - Simplified node record with full ID encoding
//!
//! This module defines the V3 node record format using full ID encoding.
//! Delta/varint compression is deferred to Phase 63b.

use crate::backend::native::types::NodeFlags;
use crate::backend::native::NativeBackendError;
use crate::backend::native::NativeResult;

/// Record layout constants
pub mod constants {
    /// Fixed metadata size in bytes
    ///
    /// Layout:
    /// - id: 8 bytes (i64, full encoding - no delta)
    /// - flags: 4 bytes (u32)
    /// - kind_offset: 2 bytes (u16, string table offset)
    /// - name_offset: 2 bytes (u16, string table offset)
    /// - data_len: 2 bytes (u16)
    /// - outgoing_cluster_offset: 8 bytes (u64)
    /// - outgoing_edge_count: 4 bytes (u32)
    /// - incoming_cluster_offset: 8 bytes (u64)
    /// - incoming_edge_count: 4 bytes (u32)
    /// Total: 8 + 4 + 2 + 2 + 2 + 8 + 4 + 8 + 4 = 42 bytes + 2 reserved = 44
    pub const FIXED_METADATA_SIZE: usize = 44;

    /// Maximum inline data size in bytes
    ///
    /// Data <= 64 bytes is stored inline in the record.
    /// Data > 64 bytes is stored externally with a reference.
    pub const MAX_INLINE_DATA: usize = 64;

    // Field offsets within fixed metadata

    /// Node ID offset (i64, full encoding)
    pub const ID_OFFSET: usize = 0;

    /// Flags offset
    pub const FLAGS_OFFSET: usize = 8;

    /// Kind string offset (string table)
    pub const KIND_OFFSET: usize = 12;

    /// Name string offset (string table)
    pub const NAME_OFFSET: usize = 14;

    /// Data length offset
    pub const DATA_LEN_OFFSET: usize = 16;

    /// Outgoing cluster offset
    pub const OUTGOING_CLUSTER_OFFSET: usize = 18;

    /// Outgoing edge count offset
    pub const OUTGOING_COUNT_OFFSET: usize = 26;

    /// Incoming cluster offset
    pub const INCOMING_CLUSTER_OFFSET: usize = 30;

    /// Incoming edge count offset
    pub const INCOMING_COUNT_OFFSET: usize = 38;

    /// External data flag offset (in data_len, high bit)
    pub const EXTERNAL_DATA_FLAG: u16 = 0x8000;

    /// Maximum data length (masking out external flag)
    pub const MAX_DATA_LEN: u16 = 0x7FFF;

    // Field sizes

    /// Node ID size (i64, full - no delta)
    pub const ID_SIZE: usize = 8;

    /// Flags size
    pub const FLAGS_SIZE: usize = 4;

    /// Kind offset size
    pub const KIND_OFFSET_SIZE: usize = 2;

    /// Name offset size
    pub const NAME_OFFSET_SIZE: usize = 2;

    /// Data length size
    pub const DATA_LEN_SIZE: usize = 2;

    /// Outgoing cluster offset size
    pub const OUTGOING_CLUSTER_SIZE: usize = 8;

    /// Outgoing count size
    pub const OUTGOING_COUNT_SIZE: usize = 4;

    /// Incoming cluster offset size
    pub const INCOMING_CLUSTER_SIZE: usize = 8;

    /// Incoming count size
    pub const INCOMING_COUNT_SIZE: usize = 4;
}

/// Fixed metadata size constant
pub const FIXED_METADATA_SIZE: usize = constants::FIXED_METADATA_SIZE;

/// Maximum inline data size constant
pub const MAX_INLINE_DATA: usize = constants::MAX_INLINE_DATA;

/// V3 Node record with simplified full ID encoding
///
/// This structure stores node metadata with full ID encoding (no delta).
/// Delta/varint compression is deferred to Phase 63b.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRecordV3 {
    /// Full node ID (i64, no delta encoding)
    pub id: i64,

    /// Node flags
    pub flags: NodeFlags,

    /// Kind string table offset (u16)
    pub kind_offset: u16,

    /// Name string table offset (u16)
    pub name_offset: u16,

    /// Data length (0-64 inline, >64 external)
    ///
    /// High bit (0x8000) indicates external data storage.
    pub data_len: u16,

    /// Inline data (<= 64 bytes) or None for external
    pub data_inline: Option<Vec<u8>>,

    /// External data offset (if data_len > 64)
    pub data_external_offset: Option<u64>,

    /// Outgoing edge cluster offset
    pub outgoing_cluster_offset: u64,

    /// Outgoing edge count
    pub outgoing_edge_count: u32,

    /// Incoming edge cluster offset
    pub incoming_cluster_offset: u64,

    /// Incoming edge count
    pub incoming_edge_count: u32,
}

impl NodeRecordV3 {
    /// Create a new node record with inline data
    pub fn new_inline(
        id: i64,
        flags: NodeFlags,
        kind_offset: u16,
        name_offset: u16,
        data: Vec<u8>,
        outgoing_cluster_offset: u64,
        outgoing_edge_count: u32,
        incoming_cluster_offset: u64,
        incoming_edge_count: u32,
    ) -> Self {
        let data_len = data.len() as u16;
        assert!(
            data_len <= MAX_INLINE_DATA as u16,
            "Inline data exceeds MAX_INLINE_DATA"
        );

        NodeRecordV3 {
            id,
            flags,
            kind_offset,
            name_offset,
            data_len,
            data_inline: Some(data),
            data_external_offset: None,
            outgoing_cluster_offset,
            outgoing_edge_count,
            incoming_cluster_offset,
            incoming_edge_count,
        }
    }

    /// Create a new node record with external data reference
    pub fn new_external(
        id: i64,
        flags: NodeFlags,
        kind_offset: u16,
        name_offset: u16,
        data_external_offset: u64,
        data_len: u16,
        outgoing_cluster_offset: u64,
        outgoing_edge_count: u32,
        incoming_cluster_offset: u64,
        incoming_edge_count: u32,
    ) -> Self {
        assert!(
            data_len > MAX_INLINE_DATA as u16,
            "External data must exceed MAX_INLINE_DATA"
        );

        NodeRecordV3 {
            id,
            flags,
            kind_offset,
            name_offset,
            data_len,
            data_inline: None,
            data_external_offset: Some(data_external_offset),
            outgoing_cluster_offset,
            outgoing_edge_count,
            incoming_cluster_offset,
            incoming_edge_count,
        }
    }

    /// Get the node ID (full encoding, no delta)
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Check if data is stored inline
    pub fn is_inline(&self) -> bool {
        self.data_inline.is_some()
    }

    /// Check if data is stored externally
    pub fn is_external(&self) -> bool {
        // Check both the external flag in data_len and the optional offset
        self.data_external_offset.is_some() || (self.data_len & constants::EXTERNAL_DATA_FLAG) != 0
    }

    /// Get the data length
    pub fn data_len(&self) -> u16 {
        self.data_len & constants::MAX_DATA_LEN
    }

    /// Calculate the serialized size of this record
    pub fn serialized_size(&self) -> usize {
        let mut size = FIXED_METADATA_SIZE;
        if self.is_inline() {
            size += self.data_inline.as_ref().map(|d| d.len()).unwrap_or(0);
        } else if self.is_external() {
            // External data: store 8-byte offset
            size += 8;
        }
        size
    }

    /// Serialize the node record to bytes
    ///
    /// Uses big-endian encoding for cross-platform compatibility.
    pub fn serialize(&self) -> NativeResult<Vec<u8>> {
        let mut buffer = Vec::with_capacity(self.serialized_size());

        // Serialize fixed metadata

        // ID: i64 in big-endian (full encoding, no delta)
        buffer.extend_from_slice(&self.id.to_be_bytes());

        // Flags: u32
        buffer.extend_from_slice(&self.flags.0.to_be_bytes());

        // Kind offset: u16
        buffer.extend_from_slice(&self.kind_offset.to_be_bytes());

        // Name offset: u16
        buffer.extend_from_slice(&self.name_offset.to_be_bytes());

        // Data length: u16 (with external flag if applicable)
        let encoded_data_len = if self.is_external() {
            self.data_len | constants::EXTERNAL_DATA_FLAG
        } else {
            self.data_len
        };
        buffer.extend_from_slice(&encoded_data_len.to_be_bytes());

        // Reserved: 2 bytes (padding to align outgoing_cluster_offset)
        buffer.extend_from_slice(&[0u8; 2]);

        // Outgoing cluster offset: u64
        buffer
            .extend_from_slice(&self.outgoing_cluster_offset.to_be_bytes());

        // Outgoing edge count: u32
        buffer.extend_from_slice(&self.outgoing_edge_count.to_be_bytes());

        // Incoming cluster offset: u64
        buffer
            .extend_from_slice(&self.incoming_cluster_offset.to_be_bytes());

        // Incoming edge count: u32
        buffer.extend_from_slice(&self.incoming_edge_count.to_be_bytes());

        // Verify metadata size
        assert_eq!(
            buffer.len(),
            FIXED_METADATA_SIZE,
            "Fixed metadata must be exactly {} bytes",
            FIXED_METADATA_SIZE
        );

        // Serialize inline data if present, or external offset if external
        if let Some(ref data) = self.data_inline {
            buffer.extend_from_slice(data);
        } else if let Some(offset) = self.data_external_offset {
            buffer.extend_from_slice(&offset.to_be_bytes());
        }

        Ok(buffer)
    }

    /// Deserialize a node record from bytes
    ///
    /// Parses big-endian encoded data with full ID reconstruction.
    pub fn deserialize(bytes: &[u8]) -> NativeResult<Self> {
        if bytes.len() < FIXED_METADATA_SIZE {
            return Err(NativeBackendError::InvalidHeader {
                field: "node_record".to_string(),
                reason: format!(
                    "insufficient bytes: expected at least {}, found {}",
                    FIXED_METADATA_SIZE,
                    bytes.len()
                ),
            });
        }

        let mut offset = 0;

        // Read ID: i64 (full encoding, no delta reconstruction needed)
        let id = i64::from_be_bytes(
            bytes[offset..offset + constants::ID_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.id".to_string(),
                    reason: "invalid ID bytes".to_string(),
                })?,
        );
        offset += constants::ID_SIZE;

        // Read flags: u32
        let flags = NodeFlags(u32::from_be_bytes(
            bytes[offset..offset + constants::FLAGS_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.flags".to_string(),
                    reason: "invalid flags bytes".to_string(),
                })?,
        ));
        offset += constants::FLAGS_SIZE;

        // Read kind offset: u16
        let kind_offset = u16::from_be_bytes(
            bytes[offset..offset + constants::KIND_OFFSET_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.kind_offset".to_string(),
                    reason: "invalid kind_offset bytes".to_string(),
                })?,
        );
        offset += constants::KIND_OFFSET_SIZE;

        // Read name offset: u16
        let name_offset = u16::from_be_bytes(
            bytes[offset..offset + constants::NAME_OFFSET_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.name_offset".to_string(),
                    reason: "invalid name_offset bytes".to_string(),
                })?,
        );
        offset += constants::NAME_OFFSET_SIZE;

        // Read data length: u16
        let encoded_data_len = u16::from_be_bytes(
            bytes[offset..offset + constants::DATA_LEN_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.data_len".to_string(),
                    reason: "invalid data_len bytes".to_string(),
                })?,
        );
        offset += constants::DATA_LEN_SIZE;

        // Check external data flag
        let is_external = (encoded_data_len & constants::EXTERNAL_DATA_FLAG) != 0;
        let data_len = encoded_data_len & constants::MAX_DATA_LEN;

        // Skip reserved: 2 bytes
        offset += 2;

        // Read outgoing cluster offset: u64
        let outgoing_cluster_offset = u64::from_be_bytes(
            bytes[offset..offset + constants::OUTGOING_CLUSTER_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.outgoing_cluster_offset".to_string(),
                    reason: "invalid outgoing_cluster_offset bytes".to_string(),
                })?,
        );
        offset += constants::OUTGOING_CLUSTER_SIZE;

        // Read outgoing edge count: u32
        let outgoing_edge_count = u32::from_be_bytes(
            bytes[offset..offset + constants::OUTGOING_COUNT_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.outgoing_edge_count".to_string(),
                    reason: "invalid outgoing_edge_count bytes".to_string(),
                })?,
        );
        offset += constants::OUTGOING_COUNT_SIZE;

        // Read incoming cluster offset: u64
        let incoming_cluster_offset = u64::from_be_bytes(
            bytes[offset..offset + constants::INCOMING_CLUSTER_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.incoming_cluster_offset".to_string(),
                    reason: "invalid incoming_cluster_offset bytes".to_string(),
                })?,
        );
        offset += constants::INCOMING_CLUSTER_SIZE;

        // Read incoming edge count: u32
        let incoming_edge_count = u32::from_be_bytes(
            bytes[offset..offset + constants::INCOMING_COUNT_SIZE]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "node_record.incoming_edge_count".to_string(),
                    reason: "invalid incoming_edge_count bytes".to_string(),
                })?,
        );
        offset += constants::INCOMING_COUNT_SIZE;

        // Verify we're at expected position
        assert_eq!(
            offset,
            FIXED_METADATA_SIZE,
            "Offset should be at end of fixed metadata"
        );

        // Read inline data or external reference
        let (data_inline, data_external_offset) = if is_external {
            // External data: data_len is the length, offset is stored separately
            // For V3 minimal, we'll parse external offset if bytes extend beyond metadata
            let external_offset = if bytes.len() > offset {
                let ext_offset = u64::from_be_bytes(
                    bytes[offset..offset + 8]
                        .try_into()
                        .unwrap_or([0u8; 8]),
                );
                Some(ext_offset)
            } else {
                // External offset not embedded (deferred to page handling)
                None
            };
            (None, external_offset)
        } else {
            // Inline data
            let inline_data = bytes[offset..].to_vec();
            (Some(inline_data), None)
        };

        Ok(NodeRecordV3 {
            id,
            flags,
            kind_offset,
            name_offset,
            data_len: encoded_data_len,
            data_inline,
            data_external_offset,
            outgoing_cluster_offset,
            outgoing_edge_count,
            incoming_cluster_offset,
            incoming_edge_count,
        })
    }

    /// Create an estimate for page capacity planning
    pub fn size_estimate() -> usize {
        FIXED_METADATA_SIZE + MAX_INLINE_DATA / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(FIXED_METADATA_SIZE, 44);
        assert_eq!(MAX_INLINE_DATA, 64);

        // Verify field offsets sum correctly
        assert_eq!(constants::ID_OFFSET, 0);
        assert_eq!(constants::FLAGS_OFFSET, 8);
        assert_eq!(constants::KIND_OFFSET, 12);
        assert_eq!(constants::NAME_OFFSET, 14);
        assert_eq!(constants::DATA_LEN_OFFSET, 16);
        assert_eq!(constants::OUTGOING_CLUSTER_OFFSET, 18);
        assert_eq!(constants::OUTGOING_COUNT_OFFSET, 26);
        assert_eq!(constants::INCOMING_CLUSTER_OFFSET, 30);
        assert_eq!(constants::INCOMING_COUNT_OFFSET, 38);
    }

    #[test]
    fn test_new_inline_node() {
        let node = NodeRecordV3::new_inline(
            12345,
            NodeFlags::empty(),
            100,
            200,
            b"test data".to_vec(),
            1000,
            5,
            2000,
            3,
        );

        assert_eq!(node.id(), 12345);
        assert!(node.is_inline());
        assert!(!node.is_external());
        assert_eq!(node.data_len(), 9);
    }

    #[test]
    fn test_new_external_node() {
        let node = NodeRecordV3::new_external(
            12345,
            NodeFlags::empty(),
            100,
            200,
            5000,
            100,
            0,    // outgoing_cluster_offset
            5,    // outgoing_edge_count
            0,    // incoming_cluster_offset
            3,    // incoming_edge_count
        );

        assert_eq!(node.id(), 12345);
        assert!(!node.is_inline());
        assert!(node.is_external());
        assert_eq!(node.data_len(), 100);
    }

    #[test]
    fn test_inline_data_max_size() {
        // Test max inline data
        let max_data = vec![0xFFu8; MAX_INLINE_DATA];
        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            max_data.clone(),
            0,
            0,
            0,
            0,
        );
        assert!(node.is_inline());
        assert_eq!(node.data_len(), MAX_INLINE_DATA as u16);
    }

    #[test]
    #[should_panic(expected = "Inline data exceeds MAX_INLINE_DATA")]
    fn test_inline_data_too_large_panics() {
        let too_large = vec![0xFFu8; MAX_INLINE_DATA + 1];
        let _ = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            too_large,
            0,
            0,
            0,
            0,
        );
    }

    #[test]
    fn test_serialize_inline_node() {
        let node = NodeRecordV3::new_inline(
            -12345, // Negative ID to test i64 encoding
            NodeFlags::empty(),
            100,
            200,
            b"Hello, V3!".to_vec(),
            1000,
            5,
            2000,
            3,
        );

        let serialized = node.serialize().unwrap();
        assert_eq!(
            serialized.len(),
            FIXED_METADATA_SIZE + "Hello, V3!".len()
        );
    }

    #[test]
    fn test_serialize_external_node() {
        let node = NodeRecordV3::new_external(
            12345,
            NodeFlags::empty(),
            100,
            200,
            5000,
            100,
            0,    // outgoing_cluster_offset
            5,    // outgoing_edge_count
            0,    // incoming_cluster_offset
            3,    // incoming_edge_count
        );

        let serialized = node.serialize().unwrap();
        // External nodes include the 8-byte external offset after metadata
        assert_eq!(serialized.len(), FIXED_METADATA_SIZE + 8);
    }

    #[test]
    fn test_round_trip_inline() {
        let original = NodeRecordV3::new_inline(
            999999,
            NodeFlags::DELETED,
            42,
            84,
            b"Test node data for round-trip".to_vec(),
            1111,
            10,
            2222,
            20,
        );

        let serialized = original.serialize().unwrap();
        let restored = NodeRecordV3::deserialize(&serialized).unwrap();

        assert_eq!(restored.id(), original.id());
        assert_eq!(restored.flags, original.flags);
        assert_eq!(restored.kind_offset, original.kind_offset);
        assert_eq!(restored.name_offset, original.name_offset);
        assert_eq!(restored.data_len(), original.data_len());
        assert_eq!(restored.data_inline, original.data_inline);
        assert_eq!(restored.outgoing_cluster_offset, original.outgoing_cluster_offset);
        assert_eq!(restored.outgoing_edge_count, original.outgoing_edge_count);
        assert_eq!(restored.incoming_cluster_offset, original.incoming_cluster_offset);
        assert_eq!(restored.incoming_edge_count, original.incoming_edge_count);
    }

    #[test]
    fn test_round_trip_external() {
        let original = NodeRecordV3::new_external(
            888888,
            NodeFlags::empty(),
            10,
            20,
            7777,
            200,
            0,    // outgoing_cluster_offset
            15,    // outgoing_edge_count
            0,    // incoming_cluster_offset
            25,    // incoming_edge_count
        );

        let serialized = original.serialize().unwrap();
        let restored = NodeRecordV3::deserialize(&serialized).unwrap();

        assert_eq!(restored.id(), original.id());
        assert_eq!(restored.flags, original.flags);
        assert_eq!(restored.kind_offset, original.kind_offset);
        assert_eq!(restored.name_offset, original.name_offset);
        assert_eq!(restored.data_len(), original.data_len());
        assert!(restored.is_external());
    }

    #[test]
    fn test_full_id_encoding() {
        // Test that full ID is preserved (no delta encoding)
        let test_ids = vec![0, 1, -1, 1000000, -1000000, i64::MAX, i64::MIN];

        for id in test_ids {
            let node = NodeRecordV3::new_inline(
                id,
                NodeFlags::empty(),
                0,
                0,
                vec![],
                0,
                0,
                0,
                0,
            );

            let serialized = node.serialize().unwrap();
            let restored = NodeRecordV3::deserialize(&serialized).unwrap();

            assert_eq!(
                restored.id(),
                id,
                "ID {} should be preserved through round-trip",
                id
            );
        }
    }

    #[test]
    fn test_serialized_size_calculation() {
        let empty_data = vec![];
        let small_data = vec![1u8; 10];
        let max_inline = vec![2u8; MAX_INLINE_DATA];

        let empty = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            empty_data,
            0,
            0,
            0,
            0,
        );
        assert_eq!(empty.serialized_size(), FIXED_METADATA_SIZE);

        let small = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            small_data.clone(),
            0,
            0,
            0,
            0,
        );
        assert_eq!(small.serialized_size(), FIXED_METADATA_SIZE + 10);

        let max = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            max_inline.clone(),
            0,
            0,
            0,
            0,
        );
        assert_eq!(
            max.serialized_size(),
            FIXED_METADATA_SIZE + MAX_INLINE_DATA
        );
    }

    #[test]
    fn test_edge_cluster_offsets_preserved() {
        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0,
            0,
            vec![],
            0x123456789ABCDEF0, // Large outgoing offset
            42,                 // Outgoing count
            0xFEDCBA9876543210, // Large incoming offset
            99,                 // Incoming count
        );

        let serialized = node.serialize().unwrap();
        let restored = NodeRecordV3::deserialize(&serialized).unwrap();

        assert_eq!(
            restored.outgoing_cluster_offset,
            0x123456789ABCDEF0
        );
        assert_eq!(restored.outgoing_edge_count, 42);
        assert_eq!(
            restored.incoming_cluster_offset,
            0xFEDCBA9876543210
        );
        assert_eq!(restored.incoming_edge_count, 99);
    }

    #[test]
    fn test_deserialize_insufficient_bytes() {
        let short_data = vec![0u8; 10]; // Way too short
        let result = NodeRecordV3::deserialize(&short_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_flags_encoding() {
        let flags = NodeFlags::DELETED;
        let node = NodeRecordV3::new_inline(
            1,
            flags,
            0,
            0,
            vec![],
            0,
            0,
            0,
            0,
        );

        let serialized = node.serialize().unwrap();
        let restored = NodeRecordV3::deserialize(&serialized).unwrap();

        assert_eq!(restored.flags, flags);
        assert!(restored.flags.contains(NodeFlags::DELETED));
    }

    #[test]
    fn test_string_table_offsets() {
        let node = NodeRecordV3::new_inline(
            1,
            NodeFlags::empty(),
            0x1234, // Kind offset
            0x5678, // Name offset
            vec![],
            0,
            0,
            0,
            0,
        );

        let serialized = node.serialize().unwrap();
        let restored = NodeRecordV3::deserialize(&serialized).unwrap();

        assert_eq!(restored.kind_offset, 0x1234);
        assert_eq!(restored.name_offset, 0x5678);
    }
}

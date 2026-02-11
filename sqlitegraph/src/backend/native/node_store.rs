//! Node storage implementation with V2 as default
//!
//! This module handles node record storage in the native graph file format.
//! Phase 31: V2 is now the default and unconditional format.

use super::constants;
use super::graph_file::GraphFile;
use super::types::*;
use crate::backend::native::v2::node_record_v2::NodeRecordV2;
use std::collections::HashMap;

/// Node storage manager for native graph database files
pub struct NodeStore<'a> {
    graph_file: &'a mut GraphFile,
    node_index: HashMap<NativeNodeId, FileOffset>,
}

impl<'a> NodeStore<'a> {
    /// Create a new node store from a graph file
    pub fn new(graph_file: &'a mut GraphFile) -> Self {
        Self {
            graph_file,
            node_index: HashMap::new(),
        }
    }

    /// Allocate the next available node ID
    pub fn allocate_node_id(&mut self) -> NativeResult<NativeNodeId> {
        let current_count = self.graph_file.persistent_header().node_count;
        let next_id = (current_count + 1) as NativeNodeId;

        // PHASE 2A FIX: Prevent node region overflow corruption
        // Check if allocating this node would exceed reserved node region
        let header = self.graph_file.persistent_header();
        let node_slot_offset = header.node_data_offset
            + ((next_id - 1) as u64 * super::constants::node::NODE_SLOT_SIZE);
        let max_node_offset =
            header.node_data_offset + super::graph_file::RESERVED_NODE_REGION_BYTES;

        if node_slot_offset >= max_node_offset {
            return Err(NativeBackendError::CorruptFreeSpace {
                reason: format!(
                    "Node region overflow: node_id={} would exceed reserved region (offset={} >= max_offset={}). \
                    Increase RESERVED_NODE_REGION_BYTES or implement node relocation.",
                    next_id, node_slot_offset, max_node_offset
                ),
            });
        }

        Ok(next_id)
    }

    /// Write a node record to the file (V2-ONLY: direct write)
    pub fn write_node(&mut self, node: &NodeRecord) -> NativeResult<()> {
        // NodeRecord is now a type alias to NodeRecordV2, write directly
        self.write_node_v2(node)
    }

    /// Write a V2 node record to the file
    pub fn write_node_v2(&mut self, record: &NodeRecordV2) -> NativeResult<()> {
        if record.id <= 0 {
            return Err(NativeBackendError::InvalidNodeId {
                id: record.id,
                max_id: 0,
            });
        }
        record.validate()?;

        // Use V2 serialization layer
        let serialized = record.serialize();

        let node_data_offset = self.graph_file.persistent_header().node_data_offset;
        let slot_offset = node_data_offset + ((record.id - 1) as u64 * 4096);

        // Ensure V2 record is padded to fill entire 4096-byte slot
        let mut slot_buffer = vec![0u8; 4096];
        slot_buffer[..serialized.len()].copy_from_slice(&serialized);

        let required_size = slot_offset + 4096;
        let current_size = self.graph_file.file_size()?;
        if required_size > current_size {
            self.graph_file.grow(required_size - current_size)?;
        }

        // Write the slot data using the appropriate I/O path
        #[cfg(feature = "v2_experimental")]
        {
            self.graph_file
                .mmap_write_bytes(slot_offset, &slot_buffer)?;
        }

        #[cfg(not(feature = "v2_experimental"))]
        {
            self.graph_file.write_bytes(slot_offset, &slot_buffer)?;
        }

        self.node_index.insert(record.id, slot_offset);

        if record.id as u64 > self.graph_file.persistent_header().node_count {
            self.graph_file.persistent_header_mut().node_count = record.id as u64;
            self.graph_file.write_header()?;
        }

        // Ensure all node data is flushed to disk before returning
        self.graph_file.flush()?;

        Ok(())
    }

    /// Read a node record from the file by ID (V2-only)
    pub fn read_node(&mut self, node_id: NativeNodeId) -> NativeResult<NodeRecord> {
        // V2-only: Always use V2 node reading (V1 format detection removed)
        self.read_node_v2(node_id)
    }

    /// Read a V2 node record from the file by ID
    pub fn read_node_v2(&mut self, node_id: NativeNodeId) -> NativeResult<NodeRecordV2> {
        let header = self.graph_file.header();
        if node_id <= 0 || node_id > header.node_count as NativeNodeId {
            return Err(NativeBackendError::InvalidNodeId {
                id: node_id,
                max_id: header.node_count as NativeNodeId,
            });
        }

        let node_data_offset = self.graph_file.persistent_header().node_data_offset;
        let slot_offset = node_data_offset + ((node_id - 1) as u64 * 4096);
        let file_size = self.graph_file.file_size()?;
        let remaining = file_size.checked_sub(slot_offset).ok_or_else(|| {
            NativeBackendError::CorruptNodeRecord {
                node_id,
                reason: format!("Slot offset {} beyond file size {}", slot_offset, file_size),
            }
        })?;

        // Read minimum required for V2 header (21 bytes for header parsing)
        if remaining < 21 {
            return Err(NativeBackendError::CorruptNodeRecord {
                node_id,
                reason: format!(
                    "Insufficient bytes ({}) for V2 header at offset {}",
                    remaining, slot_offset
                ),
            });
        }


        // Read V2 header to determine record size
        let mut header_buffer = vec![0u8; 21];
        #[cfg(feature = "v2_experimental")]
        {
            self.graph_file
                .mmap_read_bytes(slot_offset, &mut header_buffer)?;
        }

        #[cfg(not(feature = "v2_experimental"))]
        {
            self.graph_file
                .read_bytes(slot_offset, &mut header_buffer)?;
        }

        // Parse V2 header to get exact record size
        let (kind_len, name_len, data_len) =
            crate::backend::native::v2::node_record_v2::parse_v2_header_lengths(&header_buffer)?;
        let actual_record_size =
            21 + kind_len as usize + name_len as usize + data_len as usize + 32; // 32 for cluster metadata

        // Verify we have enough bytes for the actual record
        if remaining < actual_record_size as u64 {
            return Err(NativeBackendError::CorruptNodeRecord {
                node_id,
                reason: format!(
                    "V2 record truncated: need {} bytes, have {} at offset {}",
                    actual_record_size, remaining, slot_offset
                ),
            });
        }

        // Read the exact V2 record size (not the entire slot)
        let mut buffer = vec![0u8; actual_record_size];

        // Route node reads based on I/O mode
        #[cfg(all(feature = "v2_experimental", feature = "v2_io_exclusive_mmap"))]
        {
            self.graph_file.mmap_read_bytes(slot_offset, &mut buffer)?;
        }
        #[cfg(not(all(feature = "v2_experimental", feature = "v2_io_exclusive_mmap")))]
        {
            // DEFAULT MODE: Use canonical read_bytes API for V2
            self.graph_file.read_bytes(slot_offset, &mut buffer)?;
        }

        let record = NodeRecordV2::deserialize(&buffer)?;
        Ok(record)
    }

    /// Read multiple sequential node slots in a single I/O operation
    ///
    /// # Parameters
    /// - `start_node_id`: First node ID to read (must be >= 1)
    /// - `count`: Number of sequential slots to read (max 8 recommended)
    ///
    /// # Returns
    /// Vector of successfully decoded NodeRecordV2 instances
    ///
    /// # Preconditions
    /// - All node IDs must be valid (>= 1 and <= node_count)
    /// - start_node_id + count - 1 <= node_count (clamped internally)
    ///
    /// # I/O Behavior
    /// Single read_exact() call for all slots (e.g., 8 slots = 32KB in one syscall)
    ///
    /// # Example
    /// ```ignore
    /// // Read 8 sequential slots (32KB) in one I/O operation
    /// let nodes = node_store.read_slots_batch(100, 8)?;
    /// assert_eq!(nodes.len(), 8);  // All 8 nodes decoded successfully
    /// ```
    pub fn read_slots_batch(
        &mut self,
        start_node_id: NativeNodeId,
        count: usize,
    ) -> NativeResult<Vec<NodeRecordV2>> {
        let header = self.graph_file.persistent_header();

        // Validate start_node_id is within valid range
        if start_node_id < 1 {
            return Err(NativeBackendError::InvalidNodeId {
                id: start_node_id,
                max_id: header.node_count as NativeNodeId,
            });
        }

        if start_node_id > header.node_count as NativeNodeId {
            return Err(NativeBackendError::InvalidNodeId {
                id: start_node_id,
                max_id: header.node_count as NativeNodeId,
            });
        }

        // Clamp count to available nodes (prevent reading beyond file)
        let available = (header.node_count as NativeNodeId - start_node_id + 1) as usize;
        let actual_count = count.min(available);

        if actual_count == 0 {
            return Ok(Vec::new());
        }

        // Bounds checking: prevent overflow in byte calculation
        let node_slot_size = constants::node::NODE_SLOT_SIZE;
        let total_bytes = (actual_count as u64)
            .checked_mul(node_slot_size)
            .ok_or_else(|| NativeBackendError::CorruptNodeRecord {
                node_id: start_node_id,
                reason: format!(
                    "Byte count overflow for {} slots (would exceed u64::MAX)",
                    actual_count
                ),
            })?;

        // Calculate file offset for first slot
        let node_data_offset = header.node_data_offset;
        let start_offset = node_data_offset
            .checked_add((start_node_id - 1) as u64 * node_slot_size)
            .ok_or_else(|| NativeBackendError::CorruptNodeRecord {
                node_id: start_node_id,
                reason: format!("Start offset overflow for node_id={}", start_node_id),
            })?;

        // File size validation: ensure we don't read beyond EOF
        let file_size = self.graph_file.file_size()?;
        let end_offset = start_offset.checked_add(total_bytes).ok_or_else(|| {
            NativeBackendError::CorruptNodeRecord {
                node_id: start_node_id,
                reason: "End offset calculation overflow".to_string(),
            }
        })?;

        if end_offset > file_size {
            return Err(NativeBackendError::FileTooSmall {
                size: file_size,
                min_size: end_offset,
            });
        }

        // Single batch read - KEY optimization: 1 syscall instead of N
        let mut buffer = vec![0u8; total_bytes as usize];
        self.graph_file.read_bytes(start_offset, &mut buffer)?;

        // Decode each slot from the batch buffer
        let mut results = Vec::with_capacity(actual_count);
        let slot_size = node_slot_size as usize;

        for i in 0..actual_count {
            let slot_start = i * slot_size;
            let slot_end = slot_start + slot_size;

            // Extract slice for this slot
            let slot_data = &buffer[slot_start..slot_end];

            // Deserialize the node record from slot data
            match NodeRecordV2::deserialize(slot_data) {
                Ok(record) => results.push(record),
                Err(e) => {
                    // Propagate deserialization error with node_id context
                    return Err(NativeBackendError::CorruptNodeRecord {
                        node_id: start_node_id + i as NativeNodeId,
                        reason: format!("Deserialization failed: {}", e),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Delete a node record by ID (simple stub - doesn't handle edge cleanup)
    pub fn delete_node(&mut self, node_id: NativeNodeId) -> NativeResult<()> {
        // For now, just remove from index
        self.node_index.remove(&node_id);

        // TODO: Implement proper deletion with edge cleanup and space reclamation
        Ok(())
    }

    /// Get all node IDs in the file
    pub fn all_node_ids(&mut self) -> NativeResult<Vec<NativeNodeId>> {
        let max_id = self.max_node_id();
        let mut ids = Vec::new();

        // Iterate through all possible node IDs and check which ones exist
        for node_id in 1..=max_id {
            // Try to read the node - if it succeeds, the node exists
            if self.read_node_v2(node_id).is_ok() {
                ids.push(node_id);
            }
        }

        Ok(ids)
    }

    /// Get the maximum node ID in the file
    pub fn max_node_id(&self) -> NativeNodeId {
        self.graph_file.persistent_header().node_count as NativeNodeId
    }

    /// Rebuild V2 index (experimental feature)
    #[cfg(feature = "v2_experimental")]
    pub fn rebuild_v2_index(&mut self) -> NativeResult<()> {
        // Implementation stub for V2 index rebuilding
        Ok(())
    }

    /// Validate node record fields before writing (excluding ID range)
    fn validate_node_fields(&self, node: &NodeRecord) -> NativeResult<()> {
        // Validate kind string length
        if node.kind.len() > constants::node::MAX_STRING_LENGTH as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: node.kind.len() as u32,
                max_size: constants::node::MAX_STRING_LENGTH as u32,
            });
        }

        // Validate name string length
        if node.name.len() > constants::node::MAX_STRING_LENGTH as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: node.name.len() as u32,
                max_size: constants::node::MAX_STRING_LENGTH as u32,
            });
        }

        Ok(())
    }
}

/// Clear the node cache (no-op since we removed caching)
pub fn clear_node_cache() {
    // No cache to clear
}

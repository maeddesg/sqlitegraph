//! Edge record management and adjacency layout for native backend.
//!
//! This module handles edge record serialization, deserialization, and manages
//! the adjacency list layout for efficient neighbor lookups.

use super::node_store::NodeStore;
use crate::backend::native::constants::*;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::{node_slot_offset, *};
use crate::backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction};
use std::io::{Read, Seek};

/// Edge store manages edge records and adjacency layout in the graph file
pub struct EdgeStore<'a> {
    graph_file: &'a mut GraphFile,
}

impl<'a> EdgeStore<'a> {
    /// Create a new edge store from a graph file
    pub fn new(graph_file: &'a mut GraphFile) -> Self {
        Self { graph_file }
    }

    /// Write an edge record to the file
    pub fn write_edge(&mut self, edge: &EdgeRecord) -> NativeResult<()> {
        // Validate edge record - check node references against current node count
        self.validate_edge_fields(edge)?;

        // Serialize edge record
        let serialized = self.serialize_edge(edge)?;

        // Calculate offset where this edge should be written (fixed-size slot)
        let offset = self.edge_offset(edge.id);
        let fixed_slot_size = 256u64;

        // Ensure file is large enough for the fixed-size edge slot
        let edge_end = offset + fixed_slot_size;
        let current_file_size = self.graph_file.file_size()?;
        if edge_end > current_file_size {
            self.graph_file.grow(edge_end - current_file_size)?;
        }

        // Pad serialized data to fixed size
        let mut buffer = serialized;
        buffer.resize(fixed_slot_size as usize, 0);

        // Write to file
        self.graph_file.write_bytes(offset, &buffer)?;

        // Update node adjacency metadata
        self.update_node_adjacency(&edge)?;

        // Update header if this is a new edge
        if edge.id as u64 > self.graph_file.persistent_header().edge_count {
            self.graph_file.persistent_header_mut().edge_count = edge.id as u64;
            // Persist header changes to disk
            self.graph_file.flush()?;
        }

        Ok(())
    }

    /// Update node adjacency metadata when an edge is written
    fn update_node_adjacency(&mut self, edge: &EdgeRecord) -> NativeResult<()> {
        let header = self.graph_file.header();

        // Check if we need to use V2 atomic commit protocol
        let is_v2_framed = (header.flags & super::constants::FLAG_V2_FRAMED_RECORDS) != 0;
        let is_atomic_commit = (header.flags & super::constants::FLAG_V2_ATOMIC_COMMIT) != 0;

        // PHASE 5 DEBUG: Check V2 flag routing during edge insertion
        if std::env::var("V2_SLOT_DEBUG").is_ok() {
            println!(
                "[V2_SLOT_DEBUG] EDGE_INSERT: flags=0x{:08x}, is_v2_framed={}, is_atomic_commit={}",
                header.flags, is_v2_framed, is_atomic_commit
            );
        }

        // PHASE 2D: EDGE_CLUSTER_DEBUG - Probe node1 corruption before any operations
        if std::env::var("EDGE_CLUSTER_DEBUG").is_ok() {
            // Read node1 slot DIRECTLY from disk before any edge insertion operations
            let mut disk_file = std::fs::File::open(&self.graph_file.file_path())?;
            let mut node1_bytes = vec![0u8; 32];
            disk_file.seek(std::io::SeekFrom::Start(0x400))?;
            disk_file.read_exact(&mut node1_bytes)?;
            let version_before = node1_bytes[0];
            let file_size_before = self.graph_file.file_size().unwrap_or(0);
            println!(
                "[EDGE_CLUSTER_DEBUG] BEFORE_EDGE_INSERT: node1_version={}, file_size={}, node1_bytes={:02x?}",
                version_before, file_size_before, &node1_bytes
            );
        }

        // V2-ONLY: Only V2 atomic commit protocol is supported
        if is_v2_framed && is_atomic_commit {
            // Phase 70: Use SQLite-style atomic commit for V2 clusters
            self.update_node_adjacency_v2_atomic(edge)
        } else {
            // V1 adjacency updates are no longer supported
            return Err(NativeBackendError::UnsupportedVersion {
                version: 1,
                supported_version: 2,
            });
        }
    }

    /// Phase 70: V2 atomic commit protocol for clustered adjacency
    fn update_node_adjacency_v2_atomic(&mut self, edge: &EdgeRecord) -> NativeResult<()> {
        // STEP 1: Begin atomic transaction
        let next_tx_id = self.graph_file.tx_state().tx_id + 1;

        // PHASE 2D: Probe after transaction begin
        if std::env::var("EDGE_CLUSTER_DEBUG").is_ok() {
            let mut disk_file = std::fs::File::open(&self.graph_file.file_path())?;
            let mut node1_bytes = vec![0u8; 32];
            disk_file.seek(std::io::SeekFrom::Start(0x400))?;
            disk_file.read_exact(&mut node1_bytes)?;
            let version_after_tx_begin = node1_bytes[0];
            let file_size_after_tx_begin = self.graph_file.file_size().unwrap_or(0);
            println!(
                "[EDGE_CLUSTER_DEBUG] AFTER_TX_BEGIN: node1_version={}, file_size={}, node1_bytes={:02x?}",
                version_after_tx_begin, file_size_after_tx_begin, &node1_bytes
            );
        }

        self.graph_file.begin_transaction(next_tx_id)?;

        // STEP 2: Write V2 cluster data before updating node metadata
        let (actual_outgoing_size, actual_incoming_size) = self.write_v2_edge_clusters(edge)?;

        // STEP 3: Update node cluster metadata with ACTUAL sizes written to disk
        if let Err(e) = self.update_node_cluster_metadata_with_sizes(
            edge,
            actual_outgoing_size,
            actual_incoming_size,
        ) {
            // Rollback on metadata update failure
            let _ = self.graph_file.rollback_transaction();
            return Err(e);
        }

        // STEP 4: Update header offsets and checksum
        if let Err(e) = self.finalize_v2_header_updates() {
            // Rollback on header update failure
            let _ = self.graph_file.rollback_transaction();
            return Err(e);
        }

        // STEP 5: Commit transaction atomically
        self.graph_file.commit_transaction()?;

        Ok(())
    }

    /// Write V2 edge clusters for source and target nodes
    fn write_v2_edge_clusters(&mut self, edge: &EdgeRecord) -> NativeResult<(u64, u64)> {
        // Return (outgoing_size, incoming_size)

        // HOT PATH FIX: Only serialize edge data if it's non-empty/null
        // JSON serialization is expensive and unnecessary for neighbor queries
        let edge_data_bytes = if edge.data == serde_json::Value::Null {
            Vec::new() // Empty bytes for null data (common case)
        } else {
            serde_json::to_vec(&edge.data).map_err(|e| NativeBackendError::JsonError(e))?
        };

        // For outgoing cluster (source node)
        let outgoing_edge = CompactEdgeRecord::new(
            edge.to_id, // neighbor_id
            0,          // edge_type_offset (simplified - would use string table)
            edge_data_bytes.clone(),
        );
        let outgoing_size =
            self.write_or_update_v2_cluster(edge.from_id, outgoing_edge, Direction::Outgoing)?;

        // For incoming cluster (target node)
        let incoming_edge = CompactEdgeRecord::new(
            edge.from_id, // neighbor_id
            0,            // edge_type_offset (simplified - would use string table)
            edge_data_bytes,
        );
        let incoming_size =
            self.write_or_update_v2_cluster(edge.to_id, incoming_edge, Direction::Incoming)?;

        Ok((outgoing_size, incoming_size))
    }

    /// Write or update a V2 cluster for a specific node and direction
    fn write_or_update_v2_cluster(
        &mut self,
        node_id: NativeNodeId,
        edge: CompactEdgeRecord,
        direction: super::v2::edge_cluster::Direction,
    ) -> NativeResult<u64> {
        // Return actual bytes written
        use super::v2::edge_cluster::EdgeCluster;
        use super::v2::node_record_v2::NodeRecordV2;
        use super::v2::string_table::StringTable;

        // Convert CompactEdgeRecord back to EdgeRecord for proper cluster creation
        let edge_record = EdgeRecord::new(
            0, // Will be assigned proper edge_id by the system
            if matches!(direction, super::v2::edge_cluster::Direction::Outgoing) {
                node_id
            } else {
                edge.neighbor_id
            },
            if matches!(direction, super::v2::edge_cluster::Direction::Outgoing) {
                edge.neighbor_id
            } else {
                node_id
            },
            "edge_type".to_string(), // Will be handled by string table
            serde_json::from_slice(&edge.edge_data).unwrap_or_default(),
        );

        // Create cluster using the proper EdgeRecord->CompactEdgeRecord conversion with string table
        let mut string_table = StringTable::new();
        let edge_id = edge_record.id;
        let cluster =
            EdgeCluster::create_from_edges(&[edge_record], node_id, direction, &mut string_table)
                .map_err(|e| NativeBackendError::CorruptEdgeRecord {
                edge_id,
                reason: format!("Cluster creation failed: {}", e),
            })?;

        // Serialize the cluster with proper framing
        let cluster_data = cluster.serialize();

        // PHASE 5 CRITICAL FIX: Use proper cluster offset tracking to prevent overwrites
        // The issue was both clusters writing to same offset = corruption
        let header = self.graph_file.header();

        // CRITICAL FIX: Calculate proper cluster offsets to prevent node slot corruption
        // The header may have stale values from initialization when node_count=0
        let node_data_start = 1024u64;
        let node_slot_size = super::constants::node::NODE_SLOT_SIZE;
        let current_node_count = header.node_count;
        let node_region_end = node_data_start + (current_node_count * node_slot_size);

        // Ensure cluster offsets are positioned AFTER the node region
        let safe_cluster_offset = if header.outgoing_cluster_offset < node_region_end {
            println!(
                "🔥 CRITICAL FIX: Correcting outgoing_cluster_offset from {} to {} to prevent node slot corruption (node_count={})",
                header.outgoing_cluster_offset, node_region_end, current_node_count
            );
            node_region_end
        } else {
            header.outgoing_cluster_offset
        };

        let cluster_offset = match direction {
            super::v2::edge_cluster::Direction::Outgoing => safe_cluster_offset,
            super::v2::edge_cluster::Direction::Incoming => {
                // Position incoming clusters after outgoing with reasonable spacing
                let safe_incoming_offset = if header.incoming_cluster_offset < node_region_end {
                    let min_incoming_offset = safe_cluster_offset + (current_node_count * 256); // 256 bytes per node estimate
                    println!(
                        "🔥 CRITICAL FIX: Correcting incoming_cluster_offset from {} to {} to prevent node slot corruption (node_count={})",
                        header.incoming_cluster_offset, min_incoming_offset, current_node_count
                    );
                    min_incoming_offset
                } else {
                    header.incoming_cluster_offset
                };
                safe_incoming_offset
            }
        };

        // V2_CLUSTER_AUDIT: Log file write details
        if std::env::var("V2_CLUSTER_AUDIT").is_ok() {
            println!(
                "[V2_CLUSTER_AUDIT] {}:write_cluster(): file:{} line={}, node_id={}, direction={:?}, cluster_offset={}, cluster_size={}",
                std::module_path!(),
                file!(),
                line!(),
                node_id,
                direction,
                cluster_offset,
                cluster_data.len()
            );

            // CRITICAL: Check if this cluster write will corrupt any node slots
            let node_data_start = 1024u64;
            let node_slot_size = super::constants::node::NODE_SLOT_SIZE;
            let cluster_end = cluster_offset + cluster_data.len() as u64;

            // Check if cluster write overlaps with node 257 slot specifically
            let node_257_slot_start = node_slot_offset(node_data_start, 257);
            let node_257_slot_end = node_257_slot_start + node_slot_size;

            if cluster_offset <= node_257_slot_end && cluster_end >= node_257_slot_start {
                println!(
                    "🔥 CLUSTER CORRUPTION RISK: cluster write [0x{:x}-0x{:x}) overlaps with node 257 slot [0x{:x}-0x{:x})",
                    cluster_offset, cluster_end, node_257_slot_start, node_257_slot_end
                );
                println!(
                    "   THIS WILL CORRUPT NODE 257! node_id={}, direction={:?}",
                    node_id, direction
                );
            }
        }

        // PHASE 74 INSTRUMENTATION: Store checksum before write
        #[cfg(feature = "trace_v2_io")]
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            for byte in &cluster_data {
                byte.hash(&mut hasher);
            }
            let checksum32 = hasher.finish() as u32;

            println!(
                "[phase74] WRITE_PRE: tx_id={}, node_id={}, direction={:?}, checksum32=0x{:08x}, size={}",
                self.graph_file.tx_state().tx_id,
                node_id,
                direction,
                checksum32,
                cluster_data.len()
            );
        }

        if std::env::var("V2_SLOT_DEBUG").is_ok() {
            println!(
                "[V2_SLOT_DEBUG] CLUSTER_WRITE_FIXED: direction={:?}, cluster_offset={}, cluster_size={}, file_will_grow_to={}",
                direction,
                cluster_offset,
                cluster_data.len(),
                cluster_offset + cluster_data.len() as u64
            );
        }

        self.graph_file.write_bytes(cluster_offset, &cluster_data)?;

        // PHASE 74 INSTRUMENTATION: Post-write verification
        #[cfg(feature = "trace_v2_io")]
        {
            // Read back what we just wrote to verify it matches
            let mut read_back = vec![0u8; cluster_data.len()];
            if let Ok(_) = self.graph_file.read_bytes(cluster_offset, &mut read_back) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                for byte in &read_back {
                    byte.hash(&mut hasher);
                }
                let readback_checksum32 = hasher.finish() as u32;

                let post_tx_id = self.graph_file.tx_state().tx_id;

                println!(
                    "[phase74] WRITE_POST: tx_id={}, node_id={}, direction={:?}, offset={}, size={}, checksum32=0x{:08x}",
                    post_tx_id,
                    node_id,
                    direction,
                    cluster_offset,
                    read_back.len(),
                    readback_checksum32
                );
            }
        }

        // PHASE 2 FIX: Advance header cluster offset by actual written bytes to prevent reuse
        let mut header = self.graph_file.header_mut();
        let written_bytes = cluster_data.len() as u64;

        if matches!(direction, super::v2::edge_cluster::Direction::Outgoing) {
            // CRITICAL: Advance outgoing_cluster_offset to next free position
            let next_outgoing_offset = cluster_offset + written_bytes;
            header.outgoing_cluster_offset = next_outgoing_offset;

            if std::env::var("V2_CLUSTER_AUDIT").is_ok() {
                println!(
                    "[V2_CLUSTER_AUDIT] {}:header_advance(): file:{} line={}, direction=Outgoing, old_offset={}, written_bytes={}, new_offset={}",
                    std::module_path!(),
                    file!(),
                    line!(),
                    cluster_offset,
                    written_bytes,
                    next_outgoing_offset
                );
            }
        } else {
            // CRITICAL: Advance incoming_cluster_offset to next free position
            let next_incoming_offset = cluster_offset + written_bytes;
            header.incoming_cluster_offset = next_incoming_offset;

            if std::env::var("V2_CLUSTER_AUDIT").is_ok() {
                println!(
                    "[V2_CLUSTER_AUDIT] {}:header_advance(): file:{} line={}, direction=Incoming, old_offset={}, written_bytes={}, new_offset={}",
                    std::module_path!(),
                    file!(),
                    line!(),
                    cluster_offset,
                    written_bytes,
                    next_incoming_offset
                );
            }
        }

        Ok(written_bytes) // Return actual bytes written
    }

    /// Update node cluster metadata after successful cluster writes
    fn update_node_cluster_metadata(&mut self, edge: &EdgeRecord) -> NativeResult<()> {
        use super::v2::node_record_v2::NodeRecordV2;

        // Get cluster offsets and calculate cluster sizes before creating node_store
        let (outgoing_offset, incoming_offset) = {
            let header = self.graph_file.header();
            (
                header.outgoing_cluster_offset,
                header.incoming_cluster_offset,
            )
        };

        // PHASE 5 FIX: Use realistic cluster size calculations instead of header offsets
        let (outgoing_cluster_size, incoming_cluster_size) = {
            // For now, use fixed estimates since we're writing minimal clusters
            // In a full implementation, we'd track actual sizes during cluster writing
            let outgoing_size = 50; // Approximate size of one edge cluster
            let incoming_size = 50; // Approximate size of one edge cluster

            (outgoing_size, incoming_size)
        };

        // Phase 75: Record that both nodes will have V2 cluster metadata modified
        self.graph_file
            .record_node_v2_cluster_modified(edge.from_id);
        self.graph_file.record_node_v2_cluster_modified(edge.to_id);

        let mut node_store = NodeStore::new(self.graph_file);

        // Update source node (outgoing) - use V2 node record
        // PHASE 5 DEBUG: Track node read sequence during edge insertion
        if std::env::var("V2_SLOT_DEBUG").is_ok() {
            println!(
                "[V2_SLOT_DEBUG] EDGE_UPDATE: about to read source node {} for metadata update",
                edge.from_id
            );
        }
        // V2-ONLY: Direct V2 node reading with no V1 fallback
        use super::v2::node_record_v2::NodeRecordV2Ext;

        // PHASE 5 DEBUG: Track exactly where corruption happens
        if std::env::var("V2_SLOT_DEBUG").is_ok() {
            println!(
                "[V2_SLOT_DEBUG] SOURCE_NODE_READ: attempting to read node {} as V2",
                edge.from_id
            );
        }

        let mut source_node_v2 = node_store.read_node_v2(edge.from_id)?;

        source_node_v2.outgoing_edge_count += 1;
        source_node_v2.outgoing_cluster_offset = outgoing_offset;
        source_node_v2.outgoing_cluster_size = outgoing_cluster_size;
        // Phase 75: Check for fault injection before writing node metadata
        #[cfg(feature = "trace_v2_io")]
        if std::env::var("PHASE75_FORCE_ROLLBACK").is_ok() {
            use crate::fault_injection::check_fault;
            if let Err(e) = check_fault(
                crate::fault_injection::FaultPoint::Phase75V2ClusterMetadataBeforeCommit,
            ) {
                #[cfg(feature = "trace_v2_io")]
                println!(
                    "[phase75] FAULT_INJECTED: Rolling back before source node metadata write for node {} (outgoing)",
                    edge.from_id
                );
                return Err(NativeBackendError::TransactionRolledBack(format!(
                    "Phase 75 fault injection: {}",
                    e
                )));
            }
        }

        node_store.write_node_v2(&source_node_v2)?;

        // Phase 75: Trace node metadata update
        #[cfg(feature = "trace_v2_io")]
        if std::env::var("PHASE75_INSTRUMENTATION").is_ok() {
            println!(
                "[phase75] NODE_METADATA_UPDATE: node_id={}, direction=outgoing, offset={}, size={}, count={}",
                edge.from_id,
                outgoing_offset,
                outgoing_cluster_size,
                source_node_v2.outgoing_edge_count
            );
        }

        // Update target node (incoming) - V2-ONLY: Direct V2 node reading
        let mut target_node_v2 = node_store.read_node_v2(edge.to_id)?;

        target_node_v2.incoming_edge_count += 1;
        target_node_v2.incoming_cluster_offset = incoming_offset;
        target_node_v2.incoming_cluster_size = incoming_cluster_size;

        // Phase 75: Trace node metadata update
        #[cfg(feature = "trace_v2_io")]
        if std::env::var("PHASE75_INSTRUMENTATION").is_ok() {
            println!(
                "[phase75] NODE_METADATA_UPDATE: node_id={}, direction=incoming, offset={}, size={}, count={}",
                edge.to_id,
                incoming_offset,
                incoming_cluster_size,
                target_node_v2.incoming_edge_count
            );
        }

        node_store.write_node_v2(&target_node_v2)?;

        Ok(())
    }

    /// Update node cluster metadata with ACTUAL sizes written to disk (PHASE 3 FIX)
    fn update_node_cluster_metadata_with_sizes(
        &mut self,
        edge: &EdgeRecord,
        actual_outgoing_size: u64,
        actual_incoming_size: u64,
    ) -> NativeResult<()> {
        use super::v2::node_record_v2::NodeRecordV2;

        // Phase 75: Record that both nodes will have V2 cluster metadata modified
        self.graph_file
            .record_node_v2_cluster_modified(edge.from_id);
        self.graph_file.record_node_v2_cluster_modified(edge.to_id);

        // Get cluster offsets from current header state (after advancement) BEFORE borrowing node_store
        let (outgoing_offset, incoming_offset) = {
            let header = self.graph_file.header();
            let current_outgoing = header.outgoing_cluster_offset;
            let current_incoming = header.incoming_cluster_offset;

            let actual_outgoing_offset =
                if actual_outgoing_size > 0 && actual_outgoing_size <= current_outgoing {
                    current_outgoing - actual_outgoing_size
                } else {
                    current_outgoing
                };

            let actual_incoming_offset =
                if actual_incoming_size > 0 && actual_incoming_size <= current_incoming {
                    current_incoming - actual_incoming_size
                } else {
                    current_incoming
                };

            (actual_outgoing_offset, actual_incoming_offset)
        };

        // HARD INVARIANT: Validate node existence before updating adjacency
        let node_data_offset = self.graph_file.persistent_header().node_data_offset;
        let source_slot_offset = node_slot_offset(node_data_offset, edge.from_id);
        let target_slot_offset = node_slot_offset(node_data_offset, edge.to_id);

        // Check source node existence by reading slot version directly
        let mut source_buffer = [0u8; 1];
        let source_exists = if self
            .graph_file
            .read_bytes(source_slot_offset, &mut source_buffer)
            .is_ok()
        {
            source_buffer[0] == 2u8 // V2 version byte
        } else {
            false
        };

        // Check target node existence by reading slot version directly
        let mut target_buffer = [0u8; 1];
        let target_exists = if self
            .graph_file
            .read_bytes(target_slot_offset, &mut target_buffer)
            .is_ok()
        {
            target_buffer[0] == 2u8 // V2 version byte
        } else {
            false
        };

        // ENFORCEMENT: Both nodes must exist before edge insertion
        if !source_exists {
            return Err(NativeBackendError::NodeNotFound {
                node_id: edge.from_id,
                operation: "edge insertion (source)".to_string(),
            });
        }

        if !target_exists {
            return Err(NativeBackendError::NodeNotFound {
                node_id: edge.to_id,
                operation: "edge insertion (target)".to_string(),
            });
        }

        // SLOT CORRUPTION DEBUG: Check node slots before reading
        if std::env::var("SLOT_CORRUPTION_DEBUG").is_ok() {
            // Check source node slot before reading
            let source_node_data_offset = self.graph_file.persistent_header().node_data_offset;
            let source_slot_offset = node_slot_offset(source_node_data_offset, edge.from_id);
            let mut source_buffer = [0u8; 1];
            if self
                .graph_file
                .read_bytes(source_slot_offset, &mut source_buffer)
                .is_ok()
            {
                println!(
                    "[SLOT_CORRUPTION] PRE_READ_SOURCE: node_id={}, slot_offset=0x{:x}, version={}",
                    edge.from_id, source_slot_offset, source_buffer[0]
                );
            }

            // Check target node slot before reading
            let target_node_data_offset = self.graph_file.persistent_header().node_data_offset;
            let target_slot_offset = node_slot_offset(target_node_data_offset, edge.to_id);
            let mut target_buffer = [0u8; 1];
            if self
                .graph_file
                .read_bytes(target_slot_offset, &mut target_buffer)
                .is_ok()
            {
                println!(
                    "[SLOT_CORRUPTION] PRE_READ_TARGET: node_id={}, slot_offset=0x{:x}, version={}",
                    edge.to_id, target_slot_offset, target_buffer[0]
                );
            }
        }

        let mut node_store = NodeStore::new(self.graph_file);

        // Update source node (outgoing) with ACTUAL size
        let mut source_node_v2 = node_store.read_node_v2(edge.from_id)?;
        source_node_v2.outgoing_edge_count += 1;
        source_node_v2.outgoing_cluster_offset = outgoing_offset;
        source_node_v2.outgoing_cluster_size = actual_outgoing_size as u32;

        // Phase 75: Check for fault injection before writing source node metadata
        #[cfg(feature = "trace_v2_io")]
        if std::env::var("PHASE75_FORCE_ROLLBACK").is_ok() {
            use crate::fault_injection::check_fault;
            if let Err(e) = check_fault(
                crate::fault_injection::FaultPoint::Phase75V2ClusterMetadataBeforeCommit,
            ) {
                #[cfg(feature = "trace_v2_io")]
                println!(
                    "[phase75] FAULT_INJECTED: Rolling back before source node metadata write for node {} (outgoing)",
                    edge.from_id
                );
                return Err(NativeBackendError::TransactionRolledBack(format!(
                    "Phase 75 fault injection: {}",
                    e
                )));
            }
        }

        node_store.write_node_v2(&source_node_v2)?;

        // Update target node (incoming) with ACTUAL size
        // SLOT CORRUPTION DEBUG: Check target node state before reading
        if std::env::var("SLOT_CORRUPTION_DEBUG").is_ok() && edge.to_id == 257 {
            println!(
                "[SLOT_CORRUPTION] ABOUT_TO_READ_TARGET: node_id={}, about_to_read_target_257",
                edge.to_id
            );
        }
        let mut target_node_v2 = node_store.read_node_v2(edge.to_id)?;
        target_node_v2.incoming_edge_count += 1;
        target_node_v2.incoming_cluster_offset = incoming_offset;
        target_node_v2.incoming_cluster_size = actual_incoming_size as u32;

        // Phase 75: Check for fault injection before writing target node metadata
        #[cfg(feature = "trace_v2_io")]
        if std::env::var("PHASE75_FORCE_ROLLBACK").is_ok() {
            use crate::fault_injection::check_fault;
            if let Err(e) = check_fault(
                crate::fault_injection::FaultPoint::Phase75V2ClusterMetadataBeforeCommit,
            ) {
                #[cfg(feature = "trace_v2_io")]
                println!(
                    "[phase75] FAULT_INJECTED: Rolling back before target node metadata write for node {} (incoming)",
                    edge.to_id
                );
                return Err(NativeBackendError::TransactionRolledBack(format!(
                    "Phase 75 fault injection: {}",
                    e
                )));
            }
        }

        node_store.write_node_v2(&target_node_v2)?;

        if std::env::var("V2_CLUSTER_AUDIT").is_ok() {
            println!(
                "[V2_CLUSTER_AUDIT] {}:metadata_fix(): file:{} line={}, node_id={}, actual_outgoing_size={}, actual_incoming_size={}",
                std::module_path!(),
                file!(),
                line!(),
                edge.from_id,
                actual_outgoing_size,
                actual_incoming_size
            );
        }

        Ok(())
    }

    /// Finalize header updates after successful cluster and node writes
    fn finalize_v2_header_updates(&mut self) -> NativeResult<()> {
        let mut header = self.graph_file.header_mut();

        // Update edge count
        header.edge_count += 1;

        // Note: Checksum will be updated by write_header()

        // Write updated header to file
        self.graph_file.write_header()?;

        // Ensure all writes are flushed to disk
        self.graph_file.sync()?;

        Ok(())
    }

    /// Validate edge record fields except for edge ID range (used when writing)
    fn validate_edge_fields(&self, edge: &EdgeRecord) -> NativeResult<()> {
        // Validate edge ID
        if edge.id <= 0 {
            return Err(NativeBackendError::InvalidEdgeId {
                id: edge.id,
                max_id: 0,
            });
        }

        // Validate node references against current node count
        let max_node_id = self.graph_file.persistent_header().node_count as NativeNodeId;
        if edge.from_id <= 0 || edge.from_id > max_node_id {
            return Err(NativeBackendError::InvalidNodeId {
                id: edge.from_id,
                max_id: max_node_id,
            });
        }

        if edge.to_id <= 0 || edge.to_id > max_node_id {
            return Err(NativeBackendError::InvalidNodeId {
                id: edge.to_id,
                max_id: max_node_id,
            });
        }

        if edge.edge_type.len() > super::constants::edge::MAX_STRING_LENGTH_U32 as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: edge.edge_type.len() as u32,
                max_size: super::constants::edge::MAX_STRING_LENGTH_U32,
            });
        }

        Ok(())
    }

    /// Read an edge record from the file
    pub fn read_edge(&mut self, edge_id: NativeEdgeId) -> NativeResult<EdgeRecord> {
        let header = self.graph_file.header();

        if edge_id <= 0 || edge_id > header.edge_count as NativeEdgeId {
            return Err(NativeBackendError::InvalidEdgeId {
                id: edge_id,
                max_id: header.edge_count as NativeEdgeId,
            });
        }

        // Calculate offset for this edge (fixed-size slot)
        let offset = self.edge_offset(edge_id);
        let fixed_slot_size = 256usize;

        // Read the entire fixed-size slot
        let mut buffer = vec![0u8; fixed_slot_size];
        self.graph_file.read_bytes(offset, &mut buffer)?;

        // Find the actual record size by looking for the end of valid data
        // Read just enough to get the header with length fields
        if buffer.len() < 33 {
            return Err(NativeBackendError::CorruptEdgeRecord {
                edge_id,
                reason: "Edge record too short".to_string(),
            });
        }

        // Check version
        if buffer[0] != 1 {
            return Err(NativeBackendError::CorruptEdgeRecord {
                edge_id,
                reason: "Invalid edge record version".to_string(),
            });
        }

        // Extract type_len and data_len from header
        let type_len = u16::from_be_bytes([buffer[27], buffer[28]]) as usize;
        let data_len =
            u32::from_be_bytes([buffer[29], buffer[30], buffer[31], buffer[32]]) as usize;

        // Calculate actual record size
        let actual_size = 1 + 2 + 8 + 8 + 8 + 2 + 4 + type_len + data_len;

        if actual_size > fixed_slot_size {
            return Err(NativeBackendError::CorruptEdgeRecord {
                edge_id,
                reason: "Edge record too large for fixed slot".to_string(),
            });
        }

        // Truncate buffer to actual size
        buffer.truncate(actual_size);

        // Deserialize edge record
        self.deserialize_edge(edge_id, &buffer)
    }

    /// Calculate file offset for an edge record
    fn edge_offset(&self, edge_id: NativeEdgeId) -> FileOffset {
        let base_offset = self.graph_file.persistent_header().edge_data_offset;
        // Use fixed-size edge records for simplicity: 256 bytes per edge
        // This ensures we have enough space for any edge and keeps offset calculation simple
        base_offset + ((edge_id - 1) as u64 * 256)
    }

    /// Serialize an edge record to bytes
    fn serialize_edge(&self, edge: &EdgeRecord) -> NativeResult<Vec<u8>> {
        let mut buffer = Vec::new();

        // Record header (version + flags)
        buffer.push(1); // Version 1
        buffer.extend_from_slice(&edge.flags.0.to_be_bytes()[..2]);

        // Edge ID (big-endian)
        buffer.extend_from_slice(&edge.id.to_be_bytes());

        // From node ID (big-endian)
        buffer.extend_from_slice(&edge.from_id.to_be_bytes());

        // To node ID (big-endian)
        buffer.extend_from_slice(&edge.to_id.to_be_bytes());

        // Edge type length (big-endian)
        let edge_type_bytes = edge.edge_type.as_bytes();
        if edge_type_bytes.len() > edge::MAX_STRING_LENGTH as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: edge_type_bytes.len() as u32,
                max_size: edge::MAX_STRING_LENGTH_U32,
            });
        }
        buffer.extend_from_slice(&(edge_type_bytes.len() as u16).to_be_bytes());

        // Data length (big-endian)
        // HOT PATH FIX: Only serialize edge data if it's non-empty/null
        let data_bytes = if edge.data == serde_json::Value::Null {
            Vec::new() // Empty bytes for null data (common case)
        } else {
            serde_json::to_vec(&edge.data)?
        };
        if data_bytes.len() > edge::MAX_DATA_LENGTH as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: data_bytes.len() as u32,
                max_size: edge::MAX_DATA_LENGTH,
            });
        }
        buffer.extend_from_slice(&(data_bytes.len() as u32).to_be_bytes());

        // Variable-length fields
        buffer.extend_from_slice(edge_type_bytes);
        buffer.extend_from_slice(&data_bytes);

        Ok(buffer)
    }

    /// Deserialize an edge record from bytes
    fn deserialize_edge(&self, edge_id: NativeEdgeId, buffer: &[u8]) -> NativeResult<EdgeRecord> {
        if buffer.len() < edge::FIXED_HEADER_SIZE {
            return Err(NativeBackendError::BufferTooSmall {
                size: buffer.len(),
                min_size: edge::FIXED_HEADER_SIZE,
            });
        }

        let mut offset = 0;

        // Skip record header (1 byte)
        offset += 1;

        // Read edge flags
        let flags_bytes = &buffer[offset..offset + 2];
        let flags = EdgeFlags(u16::from_be_bytes([flags_bytes[0], flags_bytes[1]]));
        offset += 2;

        // Read edge ID and validate
        let id_bytes = &buffer[offset..offset + edge::ID_SIZE];
        let id = i64::from_be_bytes([
            id_bytes[0],
            id_bytes[1],
            id_bytes[2],
            id_bytes[3],
            id_bytes[4],
            id_bytes[5],
            id_bytes[6],
            id_bytes[7],
        ]);
        offset += edge::ID_SIZE;

        if id != edge_id {
            return Err(NativeBackendError::CorruptEdgeRecord {
                edge_id,
                reason: format!("Expected edge ID {}, found {}", edge_id, id),
            });
        }

        // Read from node ID
        let from_bytes = &buffer[offset..offset + edge::FROM_ID_SIZE];
        let from_id = i64::from_be_bytes([
            from_bytes[0],
            from_bytes[1],
            from_bytes[2],
            from_bytes[3],
            from_bytes[4],
            from_bytes[5],
            from_bytes[6],
            from_bytes[7],
        ]);
        offset += edge::FROM_ID_SIZE;

        // Read to node ID
        let to_bytes = &buffer[offset..offset + edge::TO_ID_SIZE];
        let to_id = i64::from_be_bytes([
            to_bytes[0],
            to_bytes[1],
            to_bytes[2],
            to_bytes[3],
            to_bytes[4],
            to_bytes[5],
            to_bytes[6],
            to_bytes[7],
        ]);
        offset += edge::TO_ID_SIZE;

        // Read edge type length
        let type_len_bytes = &buffer[offset..offset + 2];
        let edge_type_len = u16::from_be_bytes([type_len_bytes[0], type_len_bytes[1]]) as usize;
        offset += 2;

        // Read data length
        let data_len_bytes = &buffer[offset..offset + 4];
        let data_len = u32::from_be_bytes([
            data_len_bytes[0],
            data_len_bytes[1],
            data_len_bytes[2],
            data_len_bytes[3],
        ]) as usize;
        offset += 4;

        // Validate we have enough bytes for remaining fields
        if buffer.len() < offset + edge_type_len + data_len {
            return Err(NativeBackendError::BufferTooSmall {
                size: buffer.len(),
                min_size: offset + edge_type_len + data_len,
            });
        }

        // Read edge type
        let edge_type_bytes = &buffer[offset..offset + edge_type_len];
        let edge_type = std::str::from_utf8(edge_type_bytes)?.to_string();
        offset += edge_type_len;

        // Read data
        let data_bytes = &buffer[offset..offset + data_len];
        let data = serde_json::from_slice(data_bytes)
            .map_err(|e| NativeBackendError::JsonError(e.into()))?;

        Ok(EdgeRecord {
            id,
            from_id,
            to_id,
            edge_type,
            flags,
            data,
        })
    }

    /// Get the maximum valid edge ID
    pub fn max_edge_id(&self) -> NativeEdgeId {
        self.graph_file.persistent_header().edge_count as NativeEdgeId
    }

    /// Allocate a new edge ID
    pub fn allocate_edge_id(&mut self) -> NativeEdgeId {
        let current_count = self.graph_file.persistent_header().edge_count;
        let new_id = current_count + 1;
        self.graph_file.persistent_header_mut().edge_count = new_id;
        new_id as NativeEdgeId
    }

    /// Allocate adjacency space for a node's outgoing edges
    pub fn allocate_outgoing_adjacency(
        &mut self,
        _node_id: NativeNodeId,
        count: u32,
    ) -> NativeResult<FileOffset> {
        if count == 0 {
            return Ok(0);
        }

        // For simplicity, allocate at the end of the file
        let file_size = self.graph_file.file_size()?;
        let offset = file_size.max(self.graph_file.persistent_header().edge_data_offset);

        // Ensure file is large enough for the edges
        let estimated_edge_size = 128; // Rough estimate per edge
        let required_space = count as u64 * estimated_edge_size;

        if file_size < offset + required_space {
            self.graph_file.grow(required_space)?;
        }

        Ok(offset)
    }

    /// Allocate adjacency space for a node's incoming edges
    pub fn allocate_incoming_adjacency(
        &mut self,
        _node_id: NativeNodeId,
        count: u32,
    ) -> NativeResult<FileOffset> {
        if count == 0 {
            return Ok(0);
        }

        // For simplicity, allocate at the end of the file after outgoing edges
        let file_size = self.graph_file.file_size()?;
        let offset = file_size.max(self.graph_file.persistent_header().edge_data_offset);

        // Ensure file is large enough for the edges
        let estimated_edge_size = 128; // Rough estimate per edge
        let required_space = count as u64 * estimated_edge_size;

        if file_size < offset + required_space {
            self.graph_file.grow(required_space)?;
        }

        Ok(offset)
    }

    /// Write edges to adjacency area
    pub fn write_adjacency_edges(
        &mut self,
        offset: FileOffset,
        edges: &[EdgeRecord],
    ) -> NativeResult<()> {
        let mut current_offset = offset;

        for edge in edges {
            let serialized = self.serialize_edge(edge)?;
            self.graph_file.write_bytes(current_offset, &serialized)?;
            current_offset += serialized.len() as u64;
        }

        Ok(())
    }

    /// Validate edge consistency across the file
    pub fn validate_consistency(&mut self) -> NativeResult<()> {
        let max_id = self.max_edge_id();
        let max_node_id = self.graph_file.persistent_header().node_count as NativeNodeId;

        for edge_id in 1..=max_id {
            match self.read_edge(edge_id) {
                Ok(edge) => {
                    // Validate node references
                    if edge.from_id <= 0 || edge.from_id > max_node_id {
                        return Err(NativeBackendError::InvalidNodeId {
                            id: edge.from_id,
                            max_id: max_node_id,
                        });
                    }

                    if edge.to_id <= 0 || edge.to_id > max_node_id {
                        return Err(NativeBackendError::InvalidNodeId {
                            id: edge.to_id,
                            max_id: max_node_id,
                        });
                    }

                    // Self-loops are allowed but should be flagged for consideration
                    if edge.from_id == edge.to_id {
                        // This is not an error, but could be logged in a real implementation
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    /// Read V2 clustered neighbors from the file (Phase 69 implementation)
    pub fn iter_neighbors(
        &mut self,
        cluster_offset: u64,
        cluster_size: u32,
        direction: crate::backend::native::v2::edge_cluster::Direction,
        node_id: NativeNodeId,
    ) -> NativeResult<Vec<NativeNodeId>> {
        use crate::backend::native::v2::edge_cluster::{EdgeCluster, TraceContext, TraceGuard};

        // Check if this is a V2 file with framed records
        let header = self.graph_file.header();
        let is_framed =
            (header.flags & crate::backend::native::constants::FLAG_V2_FRAMED_RECORDS) != 0;

        if !is_framed {
            return Err(NativeBackendError::CorruptEdgeRecord {
                edge_id: -1,
                reason: "iter_neighbors called on non-V2 framed file".to_string(),
            });
        }

        // Read cluster bytes from file
        let mut cluster_bytes = vec![0u8; cluster_size as usize];
        self.graph_file
            .read_bytes(cluster_offset, &mut cluster_bytes)?;

        // V2_CLUSTER_AUDIT: Log file read details
        if std::env::var("V2_CLUSTER_AUDIT").is_ok() {
            println!(
                "[V2_CLUSTER_AUDIT] {}:read_cluster(): file:{} line={}, node_id={}, direction={:?}, cluster_offset={}, cluster_size={}, actual_bytes_read={}",
                std::module_path!(),
                file!(),
                line!(),
                node_id,
                direction,
                cluster_offset,
                cluster_size,
                cluster_bytes.len()
            );
        }

        // PHASE 74 INSTRUMENTATION: Reader trace before deserialization
        #[cfg(feature = "trace_v2_io")]
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            for byte in &cluster_bytes {
                byte.hash(&mut hasher);
            }
            let checksum32 = hasher.finish() as u32;

            let first_32 = if cluster_bytes.len() >= 32 {
                &cluster_bytes[..32]
            } else {
                &cluster_bytes[..]
            };
            let last_32 = if cluster_bytes.len() >= 32 {
                &cluster_bytes[cluster_bytes.len() - 32..]
            } else {
                &cluster_bytes[..]
            };

            println!(
                "[phase74] READ_START: node_id={}, direction={:?}, offset={}, size={}, checksum32=0x{:08x}, first_32={:02x?}, last_32={:02x?}",
                node_id, direction, cluster_offset, cluster_size, checksum32, first_32, last_32
            );
        }

        // Set up trace context for Phase 69 debugging
        let trace_context = TraceContext {
            node_id: node_id as i64,
            direction,
            cluster_offset,
            payload_size: cluster_size,
            strict: true, // Phase 69: Always use strict mode for framed records
        };

        let _trace_guard = TraceGuard::new(trace_context);

        // Deserialize cluster using strict V2 framed mode
        let cluster = EdgeCluster::deserialize(&cluster_bytes)?;

        // Extract neighbor IDs from cluster
        let neighbors: Vec<NativeNodeId> = cluster.iter_neighbors().collect();

        Ok(neighbors)
    }
}

#[cfg(test)]
mod tests {
    use super::super::node_store::NodeStore;
    use super::*;
    use crate::backend::native::types::NodeRecord;
    use tempfile::NamedTempFile;

    fn create_test_graph_file() -> (GraphFile, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        let graph_file = GraphFile::create(path).unwrap();
        (graph_file, temp_file)
    }

    #[test]
    fn test_edge_roundtrip() {
        let (mut graph_file, _temp_file) = create_test_graph_file();

        // Create test nodes first so edge validation passes
        {
            let mut node_store = NodeStore::new(&mut graph_file);
            let node1 = NodeRecord::new(
                1,
                "Function".to_string(),
                "func1".to_string(),
                serde_json::json!({}),
            );
            let node2 = NodeRecord::new(
                2,
                "Function".to_string(),
                "func2".to_string(),
                serde_json::json!({}),
            );
            node_store.write_node(&node1).unwrap();
            node_store.write_node(&node2).unwrap();
        }

        let mut edge_store = EdgeStore::new(&mut graph_file);

        // Create test edge
        let test_data = serde_json::json!({
            "weight": 1.5,
            "label": "test"
        });

        let original_edge = EdgeRecord::new(1, 1, 2, "calls".to_string(), test_data);

        // Write edge
        edge_store.write_edge(&original_edge).unwrap();

        // Read edge back
        let read_edge = edge_store.read_edge(1).unwrap();

        assert_eq!(original_edge.id, read_edge.id);
        assert_eq!(original_edge.from_id, read_edge.from_id);
        assert_eq!(original_edge.to_id, read_edge.to_id);
        assert_eq!(original_edge.edge_type, read_edge.edge_type);
        assert_eq!(original_edge.data, read_edge.data);
    }

    #[test]
    fn test_edge_id_allocation() {
        let (mut graph_file, _temp_file) = create_test_graph_file();
        let mut edge_store = EdgeStore::new(&mut graph_file);

        let edge_id1 = edge_store.allocate_edge_id();
        let edge_id2 = edge_store.allocate_edge_id();

        assert_eq!(edge_id1, 1);
        assert_eq!(edge_id2, 2);
        assert_eq!(edge_store.max_edge_id(), 2);
    }

    #[test]
    fn test_invalid_edge_id() {
        let (mut graph_file, _temp_file) = create_test_graph_file();
        let mut edge_store = EdgeStore::new(&mut graph_file);

        let result = edge_store.read_edge(999);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NativeBackendError::InvalidEdgeId { .. }
        ));
    }

    #[test]
    fn test_edge_invalid_node_reference() {
        let (mut graph_file, _temp_file) = create_test_graph_file();
        let mut edge_store = EdgeStore::new(&mut graph_file);

        // Create edge with invalid node reference
        let invalid_edge = EdgeRecord::new(
            1,
            999, // Invalid node ID
            2,
            "calls".to_string(),
            serde_json::json!({}),
        );

        let result = edge_store.write_edge(&invalid_edge);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NativeBackendError::InvalidNodeId { .. }
        ));
    }
}

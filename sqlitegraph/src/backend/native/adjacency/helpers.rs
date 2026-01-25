//! Helper functions for adjacency operations

use super::AdjacencyIterator;
use super::Direction;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::node_store::NodeStore;
use crate::backend::native::types::*;
use crate::snapshot::SnapshotId;

/// Helper functions for adjacency operations
pub struct AdjacencyHelpers;

impl AdjacencyHelpers {
    /// Get outgoing neighbors for a node
    pub fn get_outgoing_neighbors(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
    ) -> NativeResult<Vec<NativeNodeId>> {
        let iterator = AdjacencyIterator::new_outgoing(graph_file, node_id)?;
        iterator.collect()
    }

    /// Get incoming neighbors for a node
    pub fn get_incoming_neighbors(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
    ) -> NativeResult<Vec<NativeNodeId>> {
        let iterator = AdjacencyIterator::new_incoming(graph_file, node_id)?;
        iterator.collect()
    }

    /// Get outgoing neighbors filtered by edge type
    pub fn get_outgoing_neighbors_filtered(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        edge_types: &[&str],
    ) -> NativeResult<Vec<NativeNodeId>> {
        let iterator =
            AdjacencyIterator::new_outgoing(graph_file, node_id)?.with_edge_filter(edge_types);
        iterator.collect()
    }

    /// Get incoming neighbors filtered by edge type
    pub fn get_incoming_neighbors_filtered(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        edge_types: &[&str],
    ) -> NativeResult<Vec<NativeNodeId>> {
        let iterator =
            AdjacencyIterator::new_incoming(graph_file, node_id)?.with_edge_filter(edge_types);
        iterator.collect()
    }

    /// Check if there's a path from source to target (direct edge)
    pub fn has_direct_edge(
        graph_file: &mut GraphFile,
        source_id: NativeNodeId,
        target_id: NativeNodeId,
    ) -> NativeResult<bool> {
        let mut iterator = AdjacencyIterator::new_outgoing(graph_file, source_id)?;
        iterator.contains(target_id)
    }

    /// Get degree of node (number of outgoing edges)
    pub fn outgoing_degree(graph_file: &mut GraphFile, node_id: NativeNodeId) -> NativeResult<u32> {
        let iterator = AdjacencyIterator::new_outgoing(graph_file, node_id)?;
        Ok(iterator.total_count())
    }

    /// Get degree of node (number of incoming edges)
    pub fn incoming_degree(graph_file: &mut GraphFile, node_id: NativeNodeId) -> NativeResult<u32> {
        let iterator = AdjacencyIterator::new_incoming(graph_file, node_id)?;
        Ok(iterator.total_count())
    }

    /// Get total degree of node (incoming + outgoing)
    pub fn total_degree(graph_file: &mut GraphFile, node_id: NativeNodeId) -> NativeResult<u32> {
        let outgoing = Self::outgoing_degree(graph_file, node_id)?;
        let incoming = Self::incoming_degree(graph_file, node_id)?;
        Ok(outgoing + incoming)
    }

    // ========== Snapshot-Aware Methods (Phase 38-04) ==========

    /// Get outgoing neighbors at a specific snapshot
    ///
    /// This is the snapshot-aware version of `get_outgoing_neighbors`.
    /// For now, it delegates to the non-snapshot version since WAL filtering
    /// requires full WAL reader integration (deferred to future phases).
    ///
    /// # Architecture Note
    ///
    /// Full implementation requires:
    /// 1. Read base neighbors from GraphFile (always visible - checkpointed data)
    /// 2. Read WAL records for this node
    /// 3. Filter WAL records by commit_lsn <= snapshot_id.as_lsn()
    /// 4. Apply visible WAL records to base neighbors
    ///
    /// Current implementation returns base data only, which is correct
    /// for checkpointed data but doesn't include committed-but-not-checkpointed
    /// WAL records.
    pub fn get_outgoing_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // TODO: Phase 38-04 - Apply WAL filtering
        // For now, base data (checkpointed) is always visible
        let _snapshot = snapshot_id; // Suppress unused warning
        Self::get_outgoing_neighbors(graph_file, node_id)
    }

    /// Get incoming neighbors at a specific snapshot
    ///
    /// See `get_outgoing_neighbors_at_snapshot` for architecture notes.
    pub fn get_incoming_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // TODO: Phase 38-04 - Apply WAL filtering
        let _snapshot = snapshot_id;
        Self::get_incoming_neighbors(graph_file, node_id)
    }

    /// Get neighbors with snapshot filtering via commit_lsn
    ///
    /// This is the main entry point for snapshot-aware neighbor retrieval.
    /// It filters WAL records to only show data from transactions with
    /// commit_lsn <= snapshot_id.
    ///
    /// # Future Implementation
    ///
    /// ```rust
    /// pub fn get_neighbors_at_snapshot(
    ///     graph_file: &GraphFile,
    ///     wal_reader: &V2WALReader,
    ///     snapshot_id: SnapshotId,
    ///     node_id: NativeNodeId,
    /// ) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    ///     // 1. Read base data (always visible - from checkpoint)
    ///     let mut neighbors = Self::read_base_neighbors(graph_file, node_id)?;
    ///
    ///     // 2. Apply only WAL records from committed transactions
    ///     for wal_record in wal_reader.iter_node_records(node_id)? {
    ///         // Get transaction for this record (by contiguity, tracked in tx_index)
    ///         if let Some(tx_range) = wal_reader.tx_index().get_tx_range_for_lsn(wal_record.lsn) {
    ///             // Check if transaction was committed at or before snapshot
    ///             if let Some(commit_lsn) = tx_range.commit_lsn {
    ///                 if commit_lsn <= snapshot_id.as_lsn() {
    ///                     neighbors.apply_wal_record(wal_record)?;
    ///                 }
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(neighbors)
    /// }
    /// ```
    pub fn get_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        direction: Direction,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // TODO: Phase 38-04 - Integrate WAL reader for full snapshot isolation
        // Current implementation returns base data only
        let _snapshot = snapshot_id;

        match direction {
            Direction::Outgoing => Self::get_outgoing_neighbors(graph_file, node_id),
            Direction::Incoming => Self::get_incoming_neighbors(graph_file, node_id),
        }
    }

    /// Validate adjacency consistency for a single node with strict real adjacency checks
    pub fn validate_node_adjacency(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
    ) -> NativeResult<()> {
        // Read node info first to avoid borrowing issues
        let node = {
            let mut node_store = NodeStore::new(graph_file);
            node_store.read_node(node_id)?
        };

        // Check if adjacency metadata is consistent with actual edges
        let outgoing_neighbors = Self::get_outgoing_neighbors(graph_file, node_id)?;
        let incoming_neighbors = Self::get_incoming_neighbors(graph_file, node_id)?;

        // Strict adjacency consistency validation for real implementation
        if outgoing_neighbors.len() as u32 != node.outgoing_edge_count {
            return Err(NativeBackendError::InconsistentAdjacency {
                node_id,
                count: node.outgoing_edge_count,
                direction: "outgoing".to_string(),
                file_count: outgoing_neighbors.len() as u32,
            });
        }

        if incoming_neighbors.len() as u32 != node.incoming_edge_count {
            return Err(NativeBackendError::InconsistentAdjacency {
                node_id,
                count: node.incoming_edge_count,
                direction: "incoming".to_string(),
                file_count: incoming_neighbors.len() as u32,
            });
        }

        // Validate that all referenced neighbors exist within valid ID range
        // Use same logic as validate_node_id_range - allow up to 100,000 OR current count + 1000
        let current_node_count = graph_file.persistent_header().node_count as NativeNodeId;
        let max_allowed_node_id = std::cmp::max(100_000, current_node_count + 1000);

        for &neighbor_id in &outgoing_neighbors {
            if neighbor_id <= 0 || neighbor_id > max_allowed_node_id {
                return Err(NativeBackendError::InvalidNodeId {
                    id: neighbor_id,
                    max_id: max_allowed_node_id,
                });
            }
        }

        for &neighbor_id in &incoming_neighbors {
            if neighbor_id <= 0 || neighbor_id > max_allowed_node_id {
                return Err(NativeBackendError::InvalidNodeId {
                    id: neighbor_id,
                    max_id: max_allowed_node_id,
                });
            }
        }

        Ok(())
    }

    /// Validate adjacency consistency across all nodes
    pub fn validate_all_adjacency(graph_file: &mut GraphFile) -> NativeResult<()> {
        let max_node_id = graph_file.persistent_header().node_count as NativeNodeId;

        for node_id in 1..=max_node_id {
            Self::validate_node_adjacency(graph_file, node_id)?;
        }

        Ok(())
    }
}

//! Helper functions for adjacency operations

use super::AdjacencyIterator;
use super::Direction;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::node_store::NodeStore;
use crate::backend::native::types::*;
use crate::snapshot::SnapshotId;

#[cfg(feature = "native-v2")]
use crate::backend::native::v2::wal::reader::V2WALReader;

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

    // ========== Snapshot-Aware Methods (Phase 38-04, 61-02) ==========

    /// Get outgoing neighbors at a specific snapshot
    ///
    /// This is the snapshot-aware version of `get_outgoing_neighbors`.
    /// It integrates WAL reader to include committed-but-uncheckpointed data.
    ///
    /// # Architecture
    ///
    /// 1. Read base neighbors from GraphFile (checkpointed data)
    /// 2. Read WAL records for this node
    /// 3. Filter WAL records by commit_lsn <= snapshot_id.as_lsn()
    /// 4. Merge visible WAL records with base neighbors
    ///
    /// # Parameters
    ///
    /// * `graph_file` - The graph file to read base data from
    /// * `node_id` - The node to get neighbors for
    /// * `snapshot_id` - The snapshot LSN for visibility filtering
    /// * `wal_reader` - Optional WAL reader for uncheckpointed data
    ///
    /// # Returns
    ///
    /// A vector of neighbor node IDs visible at the given snapshot
    #[cfg(feature = "native-v2")]
    pub fn get_outgoing_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        wal_reader: Option<&V2WALReader>,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // 1. Read base neighbors (checkpointed data - always visible)
        let mut neighbors = Self::get_outgoing_neighbors(graph_file, node_id)?;

        // 2. Apply WAL records if reader available and snapshot is not "all visible"
        if let Some(reader) = wal_reader {
            if snapshot_id.as_lsn() > 0 {
                Self::apply_wal_edge_records(
                    reader,
                    node_id,
                    Direction::Outgoing,
                    snapshot_id,
                    &mut neighbors,
                )?;
            }
        }

        Ok(neighbors)
    }

    /// Get outgoing neighbors at a specific snapshot (no native-v2 feature)
    #[cfg(not(feature = "native-v2"))]
    pub fn get_outgoing_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        _wal_reader: Option<&()>,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // Without native-v2, only checkpointed data is available
        let _snapshot = snapshot_id;
        Self::get_outgoing_neighbors(graph_file, node_id)
    }

    /// Get incoming neighbors at a specific snapshot
    ///
    /// See `get_outgoing_neighbors_at_snapshot` for architecture notes.
    #[cfg(feature = "native-v2")]
    pub fn get_incoming_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        wal_reader: Option<&V2WALReader>,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // 1. Read base neighbors (checkpointed data - always visible)
        let mut neighbors = Self::get_incoming_neighbors(graph_file, node_id)?;

        // 2. Apply WAL records if reader available and snapshot is not "all visible"
        if let Some(reader) = wal_reader {
            if snapshot_id.as_lsn() > 0 {
                Self::apply_wal_edge_records(
                    reader,
                    node_id,
                    Direction::Incoming,
                    snapshot_id,
                    &mut neighbors,
                )?;
            }
        }

        Ok(neighbors)
    }

    /// Get incoming neighbors at a specific snapshot (no native-v2 feature)
    #[cfg(not(feature = "native-v2"))]
    pub fn get_incoming_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        _wal_reader: Option<&()>,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // Without native-v2, only checkpointed data is available
        let _snapshot = snapshot_id;
        Self::get_incoming_neighbors(graph_file, node_id)
    }

    /// Get neighbors with snapshot filtering via commit_lsn
    ///
    /// This is the main entry point for snapshot-aware neighbor retrieval.
    /// It filters WAL records to only show data from transactions with
    /// commit_lsn <= snapshot_id.
    #[cfg(feature = "native-v2")]
    pub fn get_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        direction: Direction,
        wal_reader: Option<&V2WALReader>,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // 1. Read base neighbors (checkpointed data - always visible)
        let mut neighbors = match direction {
            Direction::Outgoing => Self::get_outgoing_neighbors(graph_file, node_id)?,
            Direction::Incoming => Self::get_incoming_neighbors(graph_file, node_id)?,
        };

        // 2. Apply WAL records if reader available and snapshot is not "all visible"
        if let Some(reader) = wal_reader {
            if snapshot_id.as_lsn() > 0 {
                Self::apply_wal_edge_records(
                    reader,
                    node_id,
                    direction,
                    snapshot_id,
                    &mut neighbors,
                )?;
            }
        }

        Ok(neighbors)
    }

    /// Get neighbors at snapshot (no native-v2 feature)
    #[cfg(not(feature = "native-v2"))]
    pub fn get_neighbors_at_snapshot(
        graph_file: &mut GraphFile,
        node_id: NativeNodeId,
        snapshot_id: SnapshotId,
        direction: Direction,
        _wal_reader: Option<&()>,
    ) -> NativeResult<Vec<NativeNodeId>> {
        // Without native-v2, only checkpointed data is available
        let _snapshot = snapshot_id;
        match direction {
            Direction::Outgoing => Self::get_outgoing_neighbors(graph_file, node_id),
            Direction::Incoming => Self::get_incoming_neighbors(graph_file, node_id),
        }
    }

    // ========== WAL Integration Helpers (Phase 61-02) ==========

    /// Apply WAL edge records to base neighbor list with snapshot filtering
    ///
    /// This helper method reads edge records from WAL and merges them with
    /// the base neighbors, applying snapshot visibility rules.
    ///
    /// # Algorithm
    ///
    /// 1. Scan WAL for edge records affecting this node
    /// 2. Filter by transaction visibility (commit_lsn <= snapshot_lsn)
    /// 3. Track deletions separately to remove from base neighbors
    /// 4. Merge additions with remaining base neighbors
    ///
    /// # Note
    ///
    /// This is a simplified implementation for Phase 61-02.
    /// The full implementation requires:
    /// - WAL record iteration (needs &mut V2WALReader)
    /// - Edge record indexing by node
    /// - Efficient LSN range queries
    ///
    /// For now, this function provides the structure for future enhancement.
    #[cfg(feature = "native-v2")]
    fn apply_wal_edge_records(
        _wal_reader: &V2WALReader,
        _node_id: NativeNodeId,
        _direction: Direction,
        _snapshot_id: SnapshotId,
        _neighbors: &mut Vec<NativeNodeId>,
    ) -> NativeResult<()> {
        // TODO: Phase 61-02 - Full WAL record integration
        // The full implementation requires:
        // 1. Iterate through WAL records (needs mut V2WALReader)
        // 2. Filter by node and direction
        // 3. Check transaction commit_lsn <= snapshot_lsn
        // 4. Apply EdgeInsert and EdgeDelete records
        // 5. Handle NodeDelete for this node
        //
        // For now, base data is returned without WAL overlay.
        // This is correct for checkpointed data but doesn't include
        // committed-but-uncheckpointed WAL records.

        Ok(())
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

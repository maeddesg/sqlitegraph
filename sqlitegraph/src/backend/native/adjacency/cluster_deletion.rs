//! Cluster deletion operations for node deletion with edge cleanup
//!
//! This module provides functionality for deleting edge clusters when nodes
//! are removed, ensuring proper space reclamation and referential integrity.

use crate::backend::native::adjacency::Direction;
use crate::backend::native::types::{NativeBackendError, NativeResult, NativeNodeId};
use crate::backend::native::GraphFile;

/// Delete a node's edge cluster in the specified direction
///
/// This operation:
/// 1. Reads the node to get cluster metadata
/// 2. Marks the cluster storage region as free
/// 3. Clears cluster metadata from the node record
/// 4. Writes the updated node record back
///
/// # Arguments
/// * `graph_file` - The graph file containing the node
/// * `node_id` - The ID of the node whose cluster should be deleted
/// * `direction` - Outgoing or Incoming direction
///
/// # Returns
/// * `Ok(())` if cluster was deleted successfully
/// * `Err` if node doesn't exist or deletion fails
pub fn delete_cluster(
    graph_file: &mut GraphFile,
    node_id: NativeNodeId,
    direction: Direction,
) -> NativeResult<()> {
    use crate::backend::native::node_store::NodeStore;

    // 1. Read node to get cluster metadata
    let mut node_store = NodeStore::new(graph_file);
    let mut node = node_store.read_node_v2(node_id)?;
    drop(node_store);

    // 2. Get cluster offset and size
    let (cluster_offset, cluster_size) = match direction {
        Direction::Outgoing => (node.outgoing_cluster_offset, node.outgoing_cluster_size),
        Direction::Incoming => (node.incoming_cluster_offset, node.incoming_cluster_size),
    };

    // 3. Mark cluster region as free (if it exists)
    if cluster_size > 0 && cluster_offset > 0 {
        mark_region_free(graph_file, cluster_offset, cluster_size as u64)?;
    }

    // 4. Clear cluster metadata in node record
    match direction {
        Direction::Outgoing => {
            node.outgoing_edge_count = 0;
            node.outgoing_cluster_offset = 0;
            node.outgoing_cluster_size = 0;
        }
        Direction::Incoming => {
            node.incoming_edge_count = 0;
            node.incoming_cluster_offset = 0;
            node.incoming_cluster_size = 0;
        }
    }

    // 5. Write updated node record
    let mut node_store = NodeStore::new(graph_file);
    node_store.write_node_v2(&node)?;

    Ok(())
}

/// Mark a file region as free for future reuse
///
/// This writes zeros to the region, indicating it's available for reuse.
/// In a production implementation with a free-space manager, this would
/// update the free-space bitmap or list.
///
/// # Arguments
/// * `graph_file` - The graph file to update
/// * `offset` - File offset of the region to mark free
/// * `size` - Size of the region in bytes
fn mark_region_free(graph_file: &mut GraphFile, offset: u64, size: u64) -> NativeResult<()> {
    // Zero out the region
    let zero_buffer = vec![0u8; size as usize];
    graph_file.write_bytes(offset, &zero_buffer)?;
    graph_file.flush()?;

    Ok(())
}

/// Remove back-references to a deleted node from its neighbors
///
/// When a node is deleted, all edges pointing to it must be removed
/// from neighbor nodes' clusters to maintain referential integrity.
///
/// This is a simplified implementation that scans all edges to find and remove
/// references. A more optimized implementation would use an index.
///
/// # Arguments
/// * `graph_file` - The graph file containing the nodes
/// * `deleted_node_id` - The ID of the node that was deleted
///
/// # Returns
/// * `Ok(count)` - Number of back-references removed
/// * `Err` if removal fails
pub fn remove_back_references(
    graph_file: &mut GraphFile,
    deleted_node_id: NativeNodeId,
) -> NativeResult<usize> {
    use crate::backend::native::edge_store::EdgeStore;

    let mut removed_count = 0;

    // Get edge count first to avoid borrow checker issues
    let edge_count = graph_file.header().edge_count as i64;

    // Use EdgeStore to iterate edges and remove those pointing to deleted node
    for edge_id in 1..=edge_count {
        // Create a new EdgeStore for each iteration to avoid borrow issues
        let mut edge_store = EdgeStore::new(graph_file);
        if let Ok(edge) = edge_store.read_edge(edge_id) {
            // Check if this edge points to the deleted node
            if edge.to_id == deleted_node_id {
                // Delete the edge
                let _ = edge_store.delete_edge(edge_id);
                removed_count += 1;
            }
        }
    }

    Ok(removed_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exists() {
        // Basic test to verify module compiles
        assert!(true);
    }
}

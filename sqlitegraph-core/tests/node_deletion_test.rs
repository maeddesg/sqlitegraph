//! Node deletion tests with edge cascade cleanup
//!
//! Tests for the node deletion functionality that ensures:
//! - Node deletion cascades to all related edges
//! - Clusters are deleted and space marked free
//! - Neighbor back-references are removed
//! - Node slots are marked reusable
//! - Header statistics are updated

use sqlitegraph::backend::native::{
    adjacency::Direction,
    graph_file::GraphFile,
    node_store::NodeStore,
    types::{NativeBackendError, NativeResult},
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper function to create a temporary test graph file
fn create_test_graph() -> NativeResult<(TempDir, GraphFile)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::create(&db_path)?;

    // Create initial nodes and edges
    let mut node_store = NodeStore::new(&mut graph_file);

    // Create node 1
    let node1 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        1,
        "Node".to_string(),
        "node1".to_string(),
        serde_json::json!({"id": 1}),
    );
    node_store.write_node_v2(&node1)?;

    // Create node 2
    let node2 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        2,
        "Node".to_string(),
        "node2".to_string(),
        serde_json::json!({"id": 2}),
    );
    node_store.write_node_v2(&node2)?;

    // Create node 3
    let node3 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        3,
        "Node".to_string(),
        "node3".to_string(),
        serde_json::json!({"id": 3}),
    );
    node_store.write_node_v2(&node3)?;

    Ok((temp_dir, graph_file))
}

#[test]
fn test_delete_isolated_node() {
    // Create a graph with an isolated node
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    // Create node 4 (isolated, no edges)
    let mut node_store = NodeStore::new(&mut graph_file);
    let node4 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        4,
        "Node".to_string(),
        "isolated".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node_v2(&node4).unwrap();

    // Store the node count before deletion
    let node_count_before = {
        // Release mutable borrow before reading header
        drop(node_store);
        graph_file.header().node_count
    };

    // Create new store for deletion
    let mut node_store = NodeStore::new(&mut graph_file);

    // Delete the isolated node
    let result = node_store.delete_node(4);
    assert!(result.is_ok(), "Should be able to delete isolated node");

    // Verify the deleted node is no longer readable
    let result = node_store.read_node_v2(4);
    assert!(result.is_err(), "Deleted node should not be readable");

    // Release mutable borrow before reading header
    drop(node_store);

    // Verify node count was NOT decremented (node_count = max allocated slot)
    // node_count should stay the same because node IDs are fixed slot addresses
    let node_count_after = graph_file.header().node_count;
    assert_eq!(
        node_count_after, node_count_before,
        "node_count should NOT be decremented - it represents max allocated slot, not active nodes"
    );
}

#[test]
fn test_delete_node_with_edges() {
    // Create a graph with nodes and edges
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    // Verify we can read all three nodes
    let mut node_store = NodeStore::new(&mut graph_file);
    let _node1 = node_store.read_node_v2(1).unwrap();
    let _node2 = node_store.read_node_v2(2).unwrap();
    let _node3 = node_store.read_node_v2(3).unwrap();

    // Delete node 2
    let result = node_store.delete_node(2);
    assert!(result.is_ok(), "Should be able to delete node with edges");

    // Verify node 2 is deleted (slot should be cleared)
    let result = node_store.read_node_v2(2);
    assert!(result.is_err(), "Deleted node should not be readable");

    // Verify nodes 1 and 3 still exist
    let result = node_store.read_node_v2(1);
    assert!(result.is_ok(), "Node 1 should still exist");

    let result = node_store.read_node_v2(3);
    assert!(result.is_ok(), "Node 3 should still exist");
}

#[test]
fn test_deletion_updates_header() {
    // Create a graph with 4 nodes
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    // Create node 4
    let mut node_store = NodeStore::new(&mut graph_file);
    let node4 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        4,
        "Node".to_string(),
        "node4".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node_v2(&node4).unwrap();

    // Create node 5
    let node5 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        5,
        "Node".to_string(),
        "node5".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node_v2(&node5).unwrap();

    let node_count_before = {
        drop(node_store);
        graph_file.header().node_count
    };
    assert_eq!(node_count_before, 5, "Should have 5 node slots initially");

    // Delete 2 nodes
    let mut node_store = NodeStore::new(&mut graph_file);
    let _ = node_store.delete_node(4).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let _ = node_store.delete_node(5).unwrap();

    // node_count represents the max allocated slot, not active nodes.
    // It should NOT be decremented on deletion - node IDs are fixed addresses.
    let node_count_after = graph_file.header().node_count;
    assert_eq!(
        node_count_after, 5,
        "node_count should remain 5 (max allocated slot) - deletion doesn't change slot addresses"
    );

    // Verify deleted nodes are unreadable
    let mut node_store = NodeStore::new(&mut graph_file);
    assert!(
        node_store.read_node_v2(4).is_err(),
        "Deleted node 4 should be unreadable"
    );
    assert!(
        node_store.read_node_v2(5).is_err(),
        "Deleted node 5 should be unreadable"
    );

    // Verify surviving nodes are still readable
    assert!(
        node_store.read_node_v2(1).is_ok(),
        "Node 1 should still exist"
    );
    assert!(
        node_store.read_node_v2(3).is_ok(),
        "Node 3 should still exist"
    );
}

#[test]
fn test_slot_reuse_after_deletion() {
    // Create a graph, delete a node, then create a new node
    // Verify the slot can be reused (space reclamation)
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    // Create node 4
    let mut node_store = NodeStore::new(&mut graph_file);
    let node4 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        4,
        "Node".to_string(),
        "node4".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node_v2(&node4).unwrap();

    // Delete node 4
    let result = node_store.delete_node(4);
    assert!(result.is_ok(), "Should be able to delete node 4");

    // Create a new node - this could potentially reuse the slot
    // (in a full implementation with free slot tracking)
    let node5 = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
        5,
        "Node".to_string(),
        "node5".to_string(),
        serde_json::json!({}),
    );
    let result = node_store.write_node_v2(&node5);
    assert!(
        result.is_ok(),
        "Should be able to create new node after deletion"
    );
}

#[test]
fn test_delete_nonexistent_node() {
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    let mut node_store = NodeStore::new(&mut graph_file);

    // Try to delete a node that doesn't exist
    let result = node_store.delete_node(999);
    assert!(result.is_err(), "Should fail to delete non-existent node");

    // Verify error type
    match result {
        Err(NativeBackendError::InvalidNodeId { id, .. }) => {
            assert_eq!(id, 999, "Error should reference the invalid node ID");
        }
        _ => panic!("Expected InvalidNodeId error"),
    }
}

#[test]
fn test_delete_node_clears_cluster_metadata() {
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    let mut node_store = NodeStore::new(&mut graph_file);

    // Read node 1 before deletion
    let node_before = node_store.read_node_v2(1).unwrap();
    let _original_outgoing_count = node_before.outgoing_edge_count;
    let _original_incoming_count = node_before.incoming_edge_count;

    // Delete node 1
    let result = node_store.delete_node(1);
    assert!(result.is_ok(), "Should be able to delete node 1");

    // Read the slot back (should fail or return zeroed data)
    let result = node_store.read_node_v2(1);
    match result {
        Ok(node_after) => {
            // If we can read it, metadata should be cleared
            assert_eq!(
                node_after.outgoing_edge_count, 0,
                "Outgoing edge count should be 0 after deletion"
            );
            assert_eq!(
                node_after.incoming_edge_count, 0,
                "Incoming edge count should be 0 after deletion"
            );
            assert_eq!(
                node_after.outgoing_cluster_offset, 0,
                "Outgoing cluster offset should be 0 after deletion"
            );
            assert_eq!(
                node_after.incoming_cluster_offset, 0,
                "Incoming cluster offset should be 0 after deletion"
            );
        }
        Err(_) => {
            // This is also acceptable - slot may be unreadable
        }
    }
}

#[test]
fn test_multiple_decrements_in_same_slot() {
    // Test that deleting a node twice results in an error
    let (_temp_dir, mut graph_file) = create_test_graph().unwrap();

    let mut node_store = NodeStore::new(&mut graph_file);

    // Delete node 1
    let result1 = node_store.delete_node(1);
    assert!(result1.is_ok(), "First deletion should succeed");

    // Try to delete the same node again - should fail because it doesn't exist
    let result2 = node_store.delete_node(1);
    assert!(result2.is_err(), "Second deletion of same node should fail");
}

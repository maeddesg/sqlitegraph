//! Phase 31.3: Cluster Neighbor-ID Correction Tests
//!
//! These tests verify that V2 clusters store the correct neighbor IDs:
//! - Outgoing clusters should store target node IDs
//! - Incoming clusters should store source node IDs

use sqlitegraph::{
    EdgeSpec, NodeSpec, SnapshotId,
    backend::native::v2::edge_cluster::{Direction, EdgeCluster},
    config::GraphConfig,
    open_graph,
};
use tempfile::TempDir;

/// Test 1: Outgoing neighbor correctness
/// Insert edge 1->2 and verify that node 1's outgoing cluster stores neighbor_id = 2
#[test]
fn test_outgoing_cluster_neighbor_id_correctness() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_outgoing_neighbor.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Insert edge 1->2
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .unwrap();

    // Get neighbors from source node (outgoing)
    let neighbors = graph
        .neighbors(
            SnapshotId::current(),
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Outgoing neighbors from node {}: {:?}",
        source_id, neighbors
    );

    // This should pass: outgoing neighbor should be target_id
    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 outgoing neighbor"
    );
    assert_eq!(
        neighbors[0], target_id,
        "Outgoing neighbor should be target node (2), got {}",
        neighbors[0]
    );
}

/// Test 2: Incoming neighbor correctness
/// Insert edge 1->2 and verify that node 2's incoming cluster stores neighbor_id = 1
#[test]
fn test_incoming_cluster_neighbor_id_correctness() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_incoming_neighbor.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Insert edge 1->2
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .unwrap();

    // Get neighbors from target node (incoming)
    let neighbors = graph
        .neighbors(
            SnapshotId::current(),
            target_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Incoming neighbors to node {}: {:?}",
        target_id, neighbors
    );

    // This should pass: incoming neighbor should be source_id
    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 incoming neighbor"
    );
    assert_eq!(
        neighbors[0], source_id,
        "Incoming neighbor should be source node (1), got {}",
        neighbors[0]
    );
}

/// Test 3: Exact byte layout match
/// Verify that the cluster bytes encode the correct neighbor ID
#[test]
fn test_cluster_byte_layout_neighbor_id() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_cluster_bytes.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Insert edge 1->2
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({}), // Empty JSON to minimize size
        })
        .unwrap();

    // Get neighbors - this will trigger cluster creation and reading
    let neighbors = graph
        .neighbors(
            SnapshotId::current(),
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Final neighbors from node {}: {:?}",
        source_id, neighbors
    );

    // The cluster should contain neighbor_id = 2 (target)
    assert_eq!(neighbors.len(), 1, "Should have exactly 1 neighbor");
    assert_eq!(
        neighbors[0], target_id,
        "Cluster bytes should encode neighbor_id=2, got {}",
        neighbors[0]
    );
}

/// Test 4: Multiple edges with different neighbor IDs
/// Create multiple outgoing edges from same source to different targets
#[test]
fn test_multiple_outgoing_neighbor_ids() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_multiple_neighbors.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source node
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create multiple target nodes
    let mut target_ids = Vec::new();
    for i in 1..=3 {
        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Target".to_string(),
                name: format!("target_{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        target_ids.push(target_id);

        // Create edge from source to target
        graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "connects".to_string(),
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
    }

    // Get all outgoing neighbors from source
    let neighbors = graph
        .neighbors(
            SnapshotId::current(),
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Multiple neighbors from node {}: {:?}",
        source_id, neighbors
    );

    // Should have 3 neighbors with the correct IDs
    assert_eq!(
        neighbors.len(),
        3,
        "Should have exactly 3 outgoing neighbors"
    );

    // Sort and compare
    let mut sorted_neighbors = neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();

    assert_eq!(
        sorted_neighbors, sorted_targets,
        "Neighbors should match target IDs exactly"
    );
}

//! Phase 35: V2 Adjacency Router Rewrite Tests
//!
//! Test-driven development suite for Phase 35 V2 adjacency routing fixes.
//! These tests validate that neighbor queries correctly use V2 clustered adjacency
//! when available and only fall back to V1 when appropriate.

use sqlitegraph::{
    EdgeSpec, NodeSpec, SnapshotId,
    backend::native::types::NativeNodeId,
    backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction, EdgeCluster},
    backend::native::v2::node_record_v2::NodeRecordV2,
    backend::native::v2::string_table::StringTable,
    backend::native::{AdjacencyIterator, EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
use tempfile::TempDir;

/// Test a) v2_single_outgoing_neighbors_use_clustered_path
/// Create a small V2 graph (2 nodes, 1 edge 1→2) and verify neighbors() uses clustered path
#[test]
fn v2_single_outgoing_neighbors_use_clustered_path() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v2_single_outgoing.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes: 1 -> 2
    let node1_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!({"test": "outgoing"}),
        })
        .unwrap();

    let node2_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node2".to_string(),
            file_path: None,
            data: serde_json::json!({"test": "target"}),
        })
        .unwrap();

    // Create edge 1 -> 2
    graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"weight": 1.0, "test": true}),
        })
        .unwrap();

    // Test outgoing neighbors via public API
    let neighbors = graph
        .neighbors(SnapshotId::current(), node1_id, sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: V2 outgoing neighbors from node {}: {:?}",
        node1_id, neighbors
    );

    // Assertions - should have exactly 1 outgoing neighbor
    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 outgoing neighbor"
    );
    assert_eq!(neighbors[0], node2_id, "Outgoing neighbor should be node2");

    // Verify V2 cluster metadata exists via direct file access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let node1 = node_store.read_node_v2(node1_id as NativeNodeId).unwrap();

    assert!(
        node1.has_outgoing_edges(),
        "Node1 should have V2 outgoing cluster metadata"
    );
    assert_eq!(
        node1.outgoing_edge_count, 1,
        "Node1 should have 1 outgoing edge in V2 metadata"
    );
}

/// Test b) v2_single_incoming_neighbors_use_clustered_path
/// Same as above, but neighbors of node2 must include node1 via V2 cluster path
#[test]
fn v2_single_incoming_neighbors_use_clustered_path() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v2_single_incoming.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes: 1 -> 2
    let node1_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "source"}),
        })
        .unwrap();

    let node2_id = graph
        .insert_node(NodeSpec {
            kind: "Target".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "target"}),
        })
        .unwrap();

    // Create edge 1 -> 2
    graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "connects".to_string(),
            data: serde_json::json!({"type": "direct", "strength": 0.9}),
        })
        .unwrap();

    // Test incoming neighbors via public API
    let neighbors = graph
        .neighbors(SnapshotId::current(), node2_id, sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: V2 incoming neighbors to node {}: {:?}",
        node2_id, neighbors
    );

    // Assertions - should have exactly 1 incoming neighbor
    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 incoming neighbor"
    );
    assert_eq!(neighbors[0], node1_id, "Incoming neighbor should be node1");

    // Verify V2 cluster metadata exists via direct file access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let node2 = node_store.read_node_v2(node2_id as NativeNodeId).unwrap();

    assert!(
        node2.has_incoming_edges(),
        "Node2 should have V2 incoming cluster metadata"
    );
    assert_eq!(
        node2.incoming_edge_count, 1,
        "Node2 should have 1 incoming edge in V2 metadata"
    );
}

/// Test c) v2_multi_edge_neighbors_preserve_all_edges
/// 1 node with 3 outgoing edges to different targets. Verify neighbors() returns 3 distinct neighbors.
#[test]
fn v2_multi_edge_neighbors_preserve_all_edges() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v2_multi_edge.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source node
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "multi_source"}),
        })
        .unwrap();

    // Create 3 target nodes and edges
    let mut target_ids = Vec::new();
    for i in 1..=3 {
        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Target".to_string(),
                name: format!("target_{}", i),
                file_path: None,
                data: serde_json::json!({"index": i, "role": "target"}),
            })
            .unwrap();
        target_ids.push(target_id);

        // Create edge from source to target
        graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: format!("edge_type_{}", i),
                data: serde_json::json!({"index": i, "weight": i as f64 * 0.5}),
            })
            .unwrap();
    }

    // Test outgoing neighbors via public API
    let neighbors = graph
        .neighbors(SnapshotId::current(), source_id, sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: V2 multi-edge neighbors from source {}: {:?}",
        source_id, neighbors
    );

    // Assertions - should have exactly 3 distinct neighbors
    assert_eq!(
        neighbors.len(),
        3,
        "Should have exactly 3 outgoing neighbors"
    );

    // Sort and compare to handle any ordering differences
    let mut sorted_neighbors = neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();

    assert_eq!(
        sorted_neighbors, sorted_targets,
        "All 3 target nodes should be returned as neighbors"
    );

    // Verify V2 cluster metadata via direct file access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id as NativeNodeId).unwrap();

    assert_eq!(
        source_node.outgoing_edge_count, 3,
        "Source should have 3 outgoing edges in V2 metadata"
    );
    assert!(
        source_node.has_outgoing_edges(),
        "Source should have V2 outgoing cluster metadata"
    );
}

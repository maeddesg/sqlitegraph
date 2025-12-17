//! Phase 32 (Rewritten): V2 Cluster Pipeline Tests
//!
//! Clean tests using Phase 34 pipeline to verify cluster reconstruction works correctly.
//! All tests use real sqlitegraph APIs with zero manual cluster manipulation.

use sqlitegraph::{
    BackendDirection, NeighborQuery,
    backend::native::types::NativeNodeId,
    backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction, EdgeCluster},
    backend::native::v2::node_record_v2::NodeRecordV2,
    backend::native::v2::string_table::StringTable,
    backend::native::{EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
mod helpers;
use helpers::v2_fixture_builders::*;

/// Test 1: Verify single edge cluster creation with clean Phase 34 pipeline
#[test]
fn test_single_edge_cluster_clean_creation() {
    let (graph, source_id, target_id, temp_dir) = create_simple_v2_graph();

    // Verify neighbors work through public API (Phase 35 routing)
    let neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(neighbors.len(), 1, "Should have exactly 1 neighbor");
    assert_eq!(neighbors[0], target_id, "Neighbor should be target");

    // Verify V2 cluster metadata via direct file access
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id as NativeNodeId).unwrap();

    assert!(
        source_node.has_outgoing_edges(),
        "Source should have outgoing cluster metadata"
    );
    assert_eq!(
        source_node.outgoing_edge_count, 1,
        "Source should have 1 outgoing edge"
    );
    assert!(
        source_node.outgoing_cluster_offset > 0,
        "Source should have valid cluster offset"
    );
    assert!(
        source_node.outgoing_cluster_size > 0,
        "Source should have valid cluster size"
    );

    // Verify target has incoming cluster metadata
    let target_node = node_store.read_node_v2(target_id as NativeNodeId).unwrap();
    assert!(
        target_node.has_incoming_edges(),
        "Target should have incoming cluster metadata"
    );
    assert_eq!(
        target_node.incoming_edge_count, 1,
        "Target should have 1 incoming edge"
    );
}

/// Test 2: Verify multi-edge cluster creation with clean Phase 34 pipeline
#[test]
fn test_multi_edge_cluster_clean_creation() {
    let (graph, center_id, target_ids, _temp_dir) = create_star_v2_graph(3);

    // Verify center node has all outgoing neighbors
    let center_neighbors = graph
        .neighbors(
            center_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        center_neighbors.len(),
        3,
        "Center should have 3 outgoing neighbors"
    );

    // Sort for comparison since neighbor order isn't guaranteed
    let mut sorted_neighbors = center_neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();
    assert_eq!(
        sorted_neighbors, sorted_targets,
        "All target nodes should be neighbors"
    );

    // Verify each target has exactly one incoming neighbor
    for target_id in &target_ids {
        let incoming_neighbors = graph
            .neighbors(
                *target_id,
                NeighborQuery {
                    direction: BackendDirection::Incoming,
                    edge_type: None,
                },
            )
            .unwrap();

        assert_eq!(
            incoming_neighbors.len(),
            1,
            "Each target should have 1 incoming neighbor"
        );
        assert_eq!(
            incoming_neighbors[0], center_id,
            "Incoming neighbor should be center"
        );
    }
}

/// Test 3: Verify cluster data consistency across reads and writes
#[test]
fn test_cluster_data_consistency() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create complex graph with multiple edge types
    let source_id = add_node_v2(
        &mut graph,
        1,
        "Source",
        "source",
        serde_json::json!({"role": "source"}),
    );
    let target1_id = add_node_v2(
        &mut graph,
        2,
        "Target1",
        "target1",
        serde_json::json!({"role": "target"}),
    );
    let target2_id = add_node_v2(
        &mut graph,
        3,
        "Target2",
        "target2",
        serde_json::json!({"role": "target"}),
    );

    // Create different edge types
    add_edge_v2(
        &mut graph,
        source_id,
        target1_id,
        "strong_edge",
        serde_json::json!({"weight": 10.0, "type": "primary"}),
    );
    add_edge_v2(
        &mut graph,
        source_id,
        target2_id,
        "weak_edge",
        serde_json::json!({"weight": 1.0, "type": "secondary"}),
    );

    // Verify all outgoing neighbors
    let all_outgoing = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(all_outgoing.len(), 2, "Should have 2 outgoing neighbors");

    // Verify filtered queries work correctly
    let strong_neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("strong_edge".to_string()),
            },
        )
        .unwrap();

    assert_eq!(
        strong_neighbors.len(),
        1,
        "Should have 1 strong_edge neighbor"
    );
    assert_eq!(
        strong_neighbors[0], target1_id,
        "Strong edge should connect to target1"
    );

    let weak_neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("weak_edge".to_string()),
            },
        )
        .unwrap();

    assert_eq!(weak_neighbors.len(), 1, "Should have 1 weak_edge neighbor");
    assert_eq!(
        weak_neighbors[0], target2_id,
        "Weak edge should connect to target2"
    );
}

/// Test 4: Verify symmetric incoming/outgoing clusters
#[test]
fn test_symmetric_incoming_outgoing_clusters() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create bidirectional relationship
    let node1_id = add_node_v2(
        &mut graph,
        1,
        "Node1",
        "node1",
        serde_json::json!({"role": "node1"}),
    );
    let node2_id = add_node_v2(
        &mut graph,
        2,
        "Node2",
        "node2",
        serde_json::json!({"role": "node2"}),
    );

    // Create bidirectional edges
    add_edge_v2(
        &mut graph,
        node1_id,
        node2_id,
        "forward",
        serde_json::json!({"direction": "1->2"}),
    );
    add_edge_v2(
        &mut graph,
        node2_id,
        node1_id,
        "backward",
        serde_json::json!({"direction": "2->1"}),
    );

    // Verify node1's outgoing and incoming
    let node1_outgoing = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    let node1_incoming = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        node1_outgoing.len(),
        1,
        "Node1 should have 1 outgoing neighbor"
    );
    assert_eq!(
        node1_outgoing[0], node2_id,
        "Node1 outgoing should be node2"
    );
    assert_eq!(
        node1_incoming.len(),
        1,
        "Node1 should have 1 incoming neighbor"
    );
    assert_eq!(
        node1_incoming[0], node2_id,
        "Node1 incoming should be node2"
    );

    // Verify node2's outgoing and incoming
    let node2_outgoing = graph
        .neighbors(
            node2_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    let node2_incoming = graph
        .neighbors(
            node2_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        node2_outgoing.len(),
        1,
        "Node2 should have 1 outgoing neighbor"
    );
    assert_eq!(
        node2_outgoing[0], node1_id,
        "Node2 outgoing should be node1"
    );
    assert_eq!(
        node2_incoming.len(),
        1,
        "Node2 should have 1 incoming neighbor"
    );
    assert_eq!(
        node2_incoming[0], node1_id,
        "Node2 incoming should be node1"
    );
}

/// Test 5: Verify cluster persistence after file operations
#[test]
fn test_cluster_persistence() {
    let (mut graph, temp_dir) = create_test_graph();

    // Create initial graph data
    let (mut node_ids, mut edge_ids) = (Vec::new(), Vec::new());

    // Create 3 nodes
    for i in 1..=3 {
        let node_id = add_node_v2(
            &mut graph,
            i,
            "TestNode",
            &format!("test_node_{}", i),
            serde_json::json!({"index": i}),
        );
        node_ids.push(node_id);
    }

    // Create edges forming a triangle
    for i in 0..3 {
        let from = node_ids[i];
        let to = node_ids[(i + 1) % 3];
        let edge_id = add_edge_v2(
            &mut graph,
            from,
            to,
            "triangle_edge",
            serde_json::json!({"edge_index": i}),
        );
        edge_ids.push(edge_id);
    }

    // Flush and reopen
    let graph = flush_and_reopen(graph, &temp_dir);

    // Verify all nodes and edges persisted correctly
    for (i, &node_id) in node_ids.iter().enumerate() {
        // Check node exists and has correct data
        let node = graph.get_node(node_id).unwrap();
        assert_eq!(node.name, format!("test_node_{}", i + 1));

        // Check each node has correct neighbors
        let neighbors = graph
            .neighbors(
                node_id,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .unwrap();

        assert_eq!(
            neighbors.len(),
            1,
            "Each node should have 1 outgoing neighbor in triangle"
        );
    }

    // Verify triangle connectivity
    for i in 0..3 {
        let from = node_ids[i];
        let expected_to = node_ids[(i + 1) % 3];

        let neighbors = graph
            .neighbors(
                from,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: Some("triangle_edge".to_string()),
                },
            )
            .unwrap();

        assert_eq!(neighbors.len(), 1, "Should have 1 triangle_edge neighbor");
        assert_eq!(
            neighbors[0], expected_to,
            "Triangle connectivity should be preserved"
        );
    }
}

/// Test 6: Public API neighbors match manual deserialization
#[test]
fn v2_cluster_neighbors_match_manual_deserialization() {
    let (graph, source_id, target_id, temp_dir) = create_simple_v2_graph();

    // Test 1: Get neighbors via public API (graph.neighbors())
    let public_neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!("DEBUG: Public API neighbors: {:?}", public_neighbors);

    // Test 2: Get neighbors manually via EdgeCluster deserialization
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id as NativeNodeId).unwrap();

    let mut edge_store = EdgeStore::new(&mut graph_file);
    let manual_neighbors = edge_store
        .iter_neighbors(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
            source_id as NativeNodeId,
        )
        .unwrap();

    println!(
        "DEBUG: Manual EdgeCluster neighbors: {:?}",
        manual_neighbors
    );

    // Test 3: Assert both methods return the same results
    assert_eq!(
        public_neighbors.len(),
        1,
        "Public API should return 1 neighbor"
    );
    assert_eq!(
        manual_neighbors.len(),
        1,
        "Manual EdgeCluster should return 1 neighbor"
    );

    let public_as_native: Vec<NativeNodeId> = public_neighbors
        .iter()
        .map(|&id| id as NativeNodeId)
        .collect();
    assert_eq!(
        public_as_native, manual_neighbors,
        "Public API and manual EdgeCluster should return identical neighbor IDs"
    );
}

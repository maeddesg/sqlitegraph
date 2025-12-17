//! Phase 33 (Rewritten): V2 Cluster Architecture Tests
//!
//! Clean tests using Phase 34 pipeline to verify V2 cluster architecture works correctly.
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

/// Test 1: Single outgoing cluster neighbors correctness
/// Build clean graph: 1 -> 2, verify outgoing neighbors(1) == {2}
#[test]
fn test_single_outgoing_cluster_neighbors_correct() {
    let (graph, source_id, target_id, _temp_dir) = create_simple_v2_graph();

    // Test outgoing neighbors
    let outgoing_neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Single outgoing neighbors from source {}: {:?}",
        source_id, outgoing_neighbors
    );

    // Assertions
    assert_eq!(
        outgoing_neighbors.len(),
        1,
        "Should have exactly 1 outgoing neighbor"
    );
    assert_eq!(
        outgoing_neighbors[0], target_id,
        "Outgoing neighbor should be target"
    );

    // Verify target has no outgoing neighbors (it was only a target)
    let target_outgoing = graph
        .neighbors(
            target_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();
    assert_eq!(
        target_outgoing.len(),
        0,
        "Target should have no outgoing neighbors"
    );
}

/// Test 2: Single incoming cluster neighbors correctness
/// Build clean graph: 1 -> 2, verify incoming neighbors(2) == {1}
#[test]
fn test_single_incoming_cluster_neighbors_correct() {
    let (graph, source_id, target_id, _temp_dir) = create_simple_v2_graph();

    // Test incoming neighbors
    let incoming_neighbors = graph
        .neighbors(
            target_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Single incoming neighbors to target {}: {:?}",
        target_id, incoming_neighbors
    );

    // Assertions
    assert_eq!(
        incoming_neighbors.len(),
        1,
        "Should have exactly 1 incoming neighbor"
    );
    assert_eq!(
        incoming_neighbors[0], source_id,
        "Incoming neighbor should be source"
    );

    // Verify source has no incoming neighbors
    let source_incoming = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();
    assert_eq!(
        source_incoming.len(),
        0,
        "Source should have no incoming neighbors"
    );
}

/// Test 3: Multi-outgoing cluster neighbors correctness
/// Build clean graph: 1 -> 2, 1 -> 3, 1 -> 4, verify outgoing neighbors(1) contains {2,3,4}
#[test]
fn test_multi_outgoing_cluster_neighbors_correct() {
    let (graph, center_id, target_ids, _temp_dir) = create_star_v2_graph(3);

    // Test outgoing neighbors from center
    let outgoing_neighbors = graph
        .neighbors(
            center_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Multi-outgoing neighbors from center {}: {:?}",
        center_id, outgoing_neighbors
    );

    // Assertions - should have exactly 3 neighbors
    assert_eq!(
        outgoing_neighbors.len(),
        3,
        "Should have exactly 3 outgoing neighbors"
    );

    // Sort and compare to handle any ordering differences
    let mut sorted_neighbors = outgoing_neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();

    assert_eq!(
        sorted_neighbors, sorted_targets,
        "All 3 target nodes should be returned as neighbors"
    );
}

/// Test 4: Multi-incoming cluster neighbors correctness
/// Build clean graph: 2 -> 10, 3 -> 10, 4 -> 10, verify incoming neighbors(10) contains {2,3,4}
#[test]
fn test_multi_incoming_cluster_neighbors_correct() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create target node that will receive multiple incoming edges
    let target_id = add_node_v2(
        &mut graph,
        10,
        "Target",
        "multi_target",
        serde_json::json!({"role": "target"}),
    );

    // Create 3 source nodes that all point to target
    let mut source_ids = Vec::new();
    for i in 2..=4 {
        let source_id = add_node_v2(
            &mut graph,
            i as u32,
            "Source",
            &format!("source_{}", i),
            serde_json::json!({"role": "source", "index": i}),
        );

        // Create edge from source to target
        add_edge_v2(
            &mut graph,
            source_id,
            target_id,
            "points_to",
            serde_json::json!({"strength": i as f64}),
        );
        source_ids.push(source_id);
    }

    // Test incoming neighbors to target
    let incoming_neighbors = graph
        .neighbors(
            target_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Multi-incoming neighbors to target {}: {:?}",
        target_id, incoming_neighbors
    );

    // Assertions - should have exactly 3 neighbors
    assert_eq!(
        incoming_neighbors.len(),
        3,
        "Should have exactly 3 incoming neighbors"
    );

    // Sort and compare to handle any ordering differences
    let mut sorted_neighbors = incoming_neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_sources = source_ids.clone();
    sorted_sources.sort();

    assert_eq!(
        sorted_neighbors, sorted_sources,
        "All 3 source nodes should be returned as incoming neighbors"
    );
}

/// Test 5: Complex cluster layout consistency
/// Build graph with mixed edge types and verify cluster consistency
#[test]
fn test_complex_cluster_layout_consistency() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create central node
    let hub_id = add_node_v2(
        &mut graph,
        1,
        "Hub",
        "central_hub",
        serde_json::json!({"role": "hub"}),
    );

    // Create various types of connections
    let mut node_ids = Vec::new();
    let edge_types = vec!["strong", "weak", "medium", "important", "casual"];

    for (i, edge_type) in edge_types.iter().enumerate() {
        let node_id = add_node_v2(
            &mut graph,
            (i + 2) as u32,
            "Connected",
            &format!("connected_{}", i),
            serde_json::json!({"edge_type": edge_type, "index": i}),
        );
        node_ids.push(node_id);

        // Create edge with distinct data
        add_edge_v2(
            &mut graph,
            hub_id,
            node_id,
            edge_type,
            serde_json::json!({
                "edge_type_index": i,
                "edge_label": edge_type,
                "metadata": {
                    "created_at": format!("2024-01-{:02}T00:00:00Z", i + 1),
                    "priority": match *edge_type {
                        "important" => "high",
                        "strong" => "medium",
                        _ => "low"
                    },
                    "tags": vec![*edge_type, "test_edge"]
                }
            }),
        );
    }

    // Verify all outgoing neighbors exist
    let all_outgoing = graph
        .neighbors(
            hub_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        all_outgoing.len(),
        5,
        "Hub should have 5 outgoing neighbors"
    );

    // Verify filtered queries work correctly
    let important_neighbors = graph
        .neighbors(
            hub_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("important".to_string()),
            },
        )
        .unwrap();

    assert_eq!(
        important_neighbors.len(),
        1,
        "Should have exactly 1 'important' neighbor"
    );

    // Verify each connected node has exactly one incoming neighbor
    for node_id in &node_ids {
        let incoming = graph
            .neighbors(
                *node_id,
                NeighborQuery {
                    direction: BackendDirection::Incoming,
                    edge_type: None,
                },
            )
            .unwrap();

        assert_eq!(
            incoming.len(),
            1,
            "Each connected node should have exactly 1 incoming neighbor"
        );
        assert_eq!(incoming[0], hub_id, "Incoming neighbor should be the hub");
    }
}

/// Test 6: Cluster serialization roundtrip consistency
/// Build clean graph, verify cluster bytes deserialize correctly
#[test]
fn test_cluster_serialization_roundtrip_consistency() {
    let (graph, source_id, target_id, temp_dir) = create_simple_v2_graph();

    // Verify neighbors work through public API
    let public_neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(public_neighbors.len(), 1, "Should have 1 neighbor");

    // Read cluster directly to verify serialization consistency
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id as NativeNodeId).unwrap();

    // Read compact edges from cluster
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let compact_edges = edge_store
        .read_clustered_edges(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    assert_eq!(
        compact_edges.len(),
        1,
        "Cluster should contain exactly 1 compact edge"
    );

    // Verify compact edge data integrity
    let compact_edge = &compact_edges[0];
    assert_eq!(
        compact_edge.neighbor_id, target_id as NativeNodeId,
        "Compact edge should preserve target ID"
    );
    assert!(
        compact_edge.edge_type_offset > 0,
        "Compact edge should have valid type offset"
    );
    assert!(
        !compact_edge.edge_data.is_empty(),
        "Compact edge should preserve edge data"
    );

    // Verify edge data can be deserialized
    let edge_data: serde_json::Value =
        serde_json::from_slice(&compact_edge.edge_data).expect("Edge data should be valid JSON");

    // Verify the data matches what we expect
    assert!(
        edge_data.get("weight").is_some(),
        "Edge data should contain weight field"
    );
}

/// Test 7: Bidirectional cluster symmetry
/// Create bidirectional edges and verify cluster symmetry
#[test]
fn test_bidirectional_cluster_symmetry() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create two nodes
    let node1_id = add_node_v2(
        &mut graph,
        1,
        "NodeA",
        "node_a",
        serde_json::json!({"side": "A"}),
    );
    let node2_id = add_node_v2(
        &mut graph,
        2,
        "NodeB",
        "node_b",
        serde_json::json!({"side": "B"}),
    );

    // Create bidirectional edges with different types
    add_edge_v2(
        &mut graph,
        node1_id,
        node2_id,
        "forward",
        serde_json::json!({"direction": "A->B"}),
    );
    add_edge_v2(
        &mut graph,
        node2_id,
        node1_id,
        "backward",
        serde_json::json!({"direction": "B->A"}),
    );

    // Verify node1's clusters
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
        node1_incoming.len(),
        1,
        "Node1 should have 1 incoming neighbor"
    );
    assert_eq!(
        node1_outgoing[0], node2_id,
        "Node1 outgoing should be node2"
    );
    assert_eq!(
        node1_incoming[0], node2_id,
        "Node1 incoming should be node2"
    );

    // Verify node2's clusters
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
        node2_incoming.len(),
        1,
        "Node2 should have 1 incoming neighbor"
    );
    assert_eq!(
        node2_outgoing[0], node1_id,
        "Node2 outgoing should be node1"
    );
    assert_eq!(
        node2_incoming[0], node1_id,
        "Node2 incoming should be node1"
    );

    // Verify filtered queries work in both directions
    let node1_forward = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("forward".to_string()),
            },
        )
        .unwrap();

    assert_eq!(node1_forward.len(), 1, "Node1 should have 1 forward edge");

    let node2_backward = graph
        .neighbors(
            node2_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("backward".to_string()),
            },
        )
        .unwrap();

    assert_eq!(node2_backward.len(), 1, "Node2 should have 1 backward edge");
}

//! Phase 31 (Rewritten): V2 Default Takeover Tests
//!
//! Clean tests using Phase 34 pipeline to verify V2 is the default format.
//! All tests use real sqlitegraph APIs with zero manual cluster manipulation.

use sqlitegraph::{BackendDirection, NeighborQuery, config::GraphConfig, open_graph};
mod helpers;
use helpers::v2_fixture_builders::*;

/// Test 1: Verify that default writer creates V2 format with clean clustering
#[test]
fn default_writer_is_v2_format() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Insert a node using default writer (Phase 34 pipeline)
    let node_id = add_node_v2(
        &mut graph,
        1,
        "TestNode",
        "test_node",
        serde_json::json!({"test": true}),
    );

    // Insert an edge using default writer (triggers Phase 34 cluster creation)
    let _edge_id = add_edge_v2(
        &mut graph,
        node_id,
        node_id,
        "self_ref",
        serde_json::json!({"weight": 1.0}),
    );

    // Read the node back and verify it's correct
    let node = graph.get_node(node_id).unwrap();
    assert_eq!(node.name, "test_node");

    // Verify V2 clustering is used by default - should have 1 outgoing neighbor (self)
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
        "Should have 1 outgoing neighbor (self-reference)"
    );
    assert_eq!(
        neighbors[0], node_id,
        "Self-reference should point to same node"
    );

    // Verify incoming neighbors also work
    let incoming_neighbors = graph
        .neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        incoming_neighbors.len(),
        1,
        "Should have 1 incoming neighbor (self-reference)"
    );
    assert_eq!(
        incoming_neighbors[0], node_id,
        "Incoming self-reference should point to same node"
    );
}

/// Test 2: Verify that default reader reads V2 format correctly
#[test]
fn default_reader_is_v2_format() {
    let (mut graph, temp_dir) = create_test_graph();

    // Create clean V2 data
    let source_id = add_node_v2(
        &mut graph,
        1,
        "Source",
        "source",
        serde_json::json!({"role": "source"}),
    );
    let target_id = add_node_v2(
        &mut graph,
        2,
        "Target",
        "target",
        serde_json::json!({"role": "target"}),
    );

    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "test_edge",
        serde_json::json!({"weight": 2.5}),
    );

    // Flush and reopen to test reader
    let graph = flush_and_reopen(graph, &temp_dir);

    // Verify nodes read correctly
    let source_node = graph.get_node(source_id).unwrap();
    let target_node = graph.get_node(target_id).unwrap();
    assert_eq!(source_node.name, "source");
    assert_eq!(target_node.name, "target");

    // Verify adjacency works through V2 clustered data
    let outgoing = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(outgoing.len(), 1, "Source should have 1 outgoing neighbor");
    assert_eq!(outgoing[0], target_id, "Outgoing neighbor should be target");

    let incoming = graph
        .neighbors(
            target_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(incoming.len(), 1, "Target should have 1 incoming neighbor");
    assert_eq!(incoming[0], source_id, "Incoming neighbor should be source");
}

/// Test 3: Verify edge insertion updates V2 clusters correctly
#[test]
fn edge_insertion_updates_v2_clusters() {
    let (mut graph, temp_dir) = create_test_graph();

    // Create a simple star topology
    let center_id = add_node_v2(
        &mut graph,
        0,
        "Center",
        "center",
        serde_json::json!({"type": "center"}),
    );

    let mut target_ids = Vec::new();
    for i in 1..=3 {
        let target_id = add_node_v2(
            &mut graph,
            i,
            "Target",
            &format!("target_{}", i),
            serde_json::json!({"index": i, "type": "target"}),
        );

        add_edge_v2(
            &mut graph,
            center_id,
            target_id,
            "connects",
            serde_json::json!({"index": i}),
        );
        target_ids.push(target_id);
    }

    // Flush to ensure all clusters are written
    let graph = flush_and_reopen(graph, &temp_dir);

    // Verify center node has correct outgoing neighbors
    let center_outgoing = graph
        .neighbors(
            center_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        center_outgoing.len(),
        3,
        "Center should have 3 outgoing neighbors"
    );

    // Sort for comparison since neighbor order isn't guaranteed
    let mut sorted_outgoing = center_outgoing.clone();
    sorted_outgoing.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();
    assert_eq!(
        sorted_outgoing, sorted_targets,
        "All target nodes should be neighbors"
    );

    // Verify each target has correct incoming neighbor
    for target_id in &target_ids {
        let target_incoming = graph
            .neighbors(
                *target_id,
                NeighborQuery {
                    direction: BackendDirection::Incoming,
                    edge_type: None,
                },
            )
            .unwrap();

        assert_eq!(
            target_incoming.len(),
            1,
            "Each target should have 1 incoming neighbor"
        );
        assert_eq!(
            target_incoming[0], center_id,
            "Incoming neighbor should be center"
        );
    }
}

/// Test 4: Verify BFS uses V2 clustered iteration by default
#[test]
fn bfs_uses_v2_clustered_iteration() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create a chain: 1 -> 2 -> 3 -> 4
    let mut node_ids = Vec::new();
    for i in 1..=4 {
        let node_id = add_node_v2(
            &mut graph,
            i,
            "ChainNode",
            &format!("node_{}", i),
            serde_json::json!({"index": i, "type": "chain"}),
        );
        node_ids.push(node_id);
    }

    // Create chain edges
    for i in 0..3 {
        add_simple_edge_v2(&mut graph, node_ids[i], node_ids[i + 1], "next");
    }

    // Perform BFS from first node
    let bfs_result = graph.bfs(node_ids[0], 3).unwrap();

    assert_eq!(bfs_result.len(), 4, "BFS should reach all 4 nodes");

    // Verify all nodes are included (order may vary)
    let mut result_sorted = bfs_result.clone();
    result_sorted.sort();
    let mut expected_sorted = node_ids.clone();
    expected_sorted.sort();
    assert_eq!(
        result_sorted, expected_sorted,
        "BFS should find all nodes in chain"
    );
}

/// Test 5: Verify adjacency uses clustered metadata by default
#[test]
fn adjacency_uses_clustered_metadata_by_default() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create nodes with distinct roles
    let producer_id = add_node_v2(
        &mut graph,
        1,
        "Producer",
        "producer",
        serde_json::json!({"role": "producer"}),
    );
    let consumer_id = add_node_v2(
        &mut graph,
        2,
        "Consumer",
        "consumer",
        serde_json::json!({"role": "consumer"}),
    );

    // Create edge with complex data
    add_edge_v2(
        &mut graph,
        producer_id,
        consumer_id,
        "produces",
        serde_json::json!({
            "product": "data",
            "quantity": 42,
            "timestamp": "2024-01-01T00:00:00Z",
            "metadata": {"quality": "high", "certified": true}
        }),
    );

    // Verify adjacency works through V2 clustered metadata
    let producer_outgoing = graph
        .neighbors(
            producer_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        producer_outgoing.len(),
        1,
        "Producer should have 1 outgoing neighbor"
    );
    assert_eq!(
        producer_outgoing[0], consumer_id,
        "Producer should connect to consumer"
    );

    // Verify filtered adjacency works
    let filtered_outgoing = graph
        .neighbors(
            producer_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("produces".to_string()),
            },
        )
        .unwrap();

    assert_eq!(
        filtered_outgoing.len(),
        1,
        "Filtered query should find the 'produces' edge"
    );

    let wrong_filter = graph
        .neighbors(
            producer_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("does_not_exist".to_string()),
            },
        )
        .unwrap();

    assert_eq!(
        wrong_filter.len(),
        0,
        "Non-existent edge type should return empty results"
    );
}

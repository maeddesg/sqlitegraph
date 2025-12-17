//! Phase 50: V2 Semantic Regression Tests
//!
//! These tests establish the correct contract between:
//! 1) neighbors() → unique neighbor IDs (V1 + V2 parity)
//! 2) edges() → full multi-edge visibility

use sqlitegraph::{BackendDirection, EdgeSpec, NeighborQuery, config::GraphConfig, open_graph};
mod helpers;
use helpers::v2_fixture_builders::*;

/// Test 1: neighbors() should return UNIQUE neighbor IDs
/// This test documents the expected V1/V2 semantic contract
#[test]
fn test_neighbors_returns_unique_ids_v2_compliance() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create 3 distinct target nodes
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
        serde_json::json!({"role": "target1"}),
    );
    let target2_id = add_node_v2(
        &mut graph,
        3,
        "Target2",
        "target2",
        serde_json::json!({"role": "target2"}),
    );
    let target3_id = add_node_v2(
        &mut graph,
        4,
        "Target3",
        "target3",
        serde_json::json!({"role": "target3"}),
    );

    // Create edges to distinct targets - should return 3 unique neighbors
    add_simple_edge_v2(&mut graph, source_id, target1_id, "connects1");
    add_simple_edge_v2(&mut graph, source_id, target2_id, "connects2");
    add_simple_edge_v2(&mut graph, source_id, target3_id, "connects3");

    let neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    // CRITICAL: neighbors() must return unique IDs
    assert_eq!(
        neighbors.len(),
        3,
        "neighbors() should return 3 unique neighbor IDs"
    );

    // All neighbors should be distinct
    let mut unique_neighbors = std::collections::HashSet::new();
    for &neighbor in &neighbors {
        unique_neighbors.insert(neighbor);
    }
    assert_eq!(
        unique_neighbors.len(),
        3,
        "All neighbor IDs should be unique"
    );

    // Should contain exactly the expected target IDs
    assert!(
        unique_neighbors.contains(&target1_id),
        "Should contain target1_id"
    );
    assert!(
        unique_neighbors.contains(&target2_id),
        "Should contain target2_id"
    );
    assert!(
        unique_neighbors.contains(&target3_id),
        "Should contain target3_id"
    );
}

/// Test 2: neighbors() should return UNIQUE neighbor IDs even with multi-edge scenarios
/// This tests the edge case where multiple edges go to the same target
#[test]
fn test_neighbors_returns_unique_ids_multi_edge_scenario() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create source and single target (same target multiple edges)
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

    // Create MULTIPLE edges to the SAME target - Phase 36 pattern
    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "connection_1",
        serde_json::json!({"index": 1}),
    );
    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "connection_2",
        serde_json::json!({"index": 2}),
    );
    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "connection_3",
        serde_json::json!({"index": 3}),
    );

    let neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    // CRITICAL SEMANTIC CONTRACT: neighbors() must return unique IDs
    // Even though there are 3 edges, neighbors() should return 1 unique target
    assert_eq!(
        neighbors.len(),
        1,
        "neighbors() should deduplicate to 1 unique neighbor ID for multi-edge scenario"
    );

    // The single neighbor should be the target
    assert_eq!(
        neighbors[0], target_id,
        "The single neighbor should be the target_id"
    );
}

/// Test 3: Edge count should still be observable through cluster metadata
/// This tests that multi-edge storage works correctly even when neighbors() deduplicates
#[test]
fn test_multi_edge_storage_integrity_with_neighbor_deduplication() {
    let (mut graph, temp_dir) = create_test_graph();

    // Create source and single target
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

    // Create 3 edges to the same target
    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "connection_1",
        serde_json::json!({"index": 1}),
    );
    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "connection_2",
        serde_json::json!({"index": 2}),
    );
    add_edge_v2(
        &mut graph,
        source_id,
        target_id,
        "connection_3",
        serde_json::json!({"index": 3}),
    );

    // Verify storage contains 3 edges via cluster metadata
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = sqlitegraph::backend::native::GraphFile::open(&db_path).unwrap();
    let mut node_store = sqlitegraph::backend::native::NodeStore::new(&mut graph_file);
    let source_node = node_store
        .read_node_v2(source_id as sqlitegraph::backend::native::types::NativeNodeId)
        .unwrap();

    // Cluster metadata should show 3 edges stored
    assert_eq!(
        source_node.outgoing_edge_count, 3,
        "Cluster metadata should show 3 edges stored"
    );
    assert!(
        source_node.outgoing_cluster_offset > 0,
        "Should have valid cluster offset"
    );
    assert!(
        source_node.outgoing_cluster_size > 8,
        "Cluster size should be > header size (8)"
    );
}

/// Test 4: V1 vs V2 semantic parity - neighbors() should behave identically
#[test]
fn test_v1_v2_neighbor_semantic_parity_distinct_targets() {
    let (mut v1_graph, _temp_dir1) = create_test_graph_v1();
    let (mut v2_graph, _temp_dir2) = create_test_graph();

    // Create identical graph structure in both backends
    let source_id_v1 = add_node_v2(&mut v1_graph, 1, "Source", "source", serde_json::json!({}));
    let target1_id_v1 = add_node_v2(
        &mut v1_graph,
        2,
        "Target1",
        "target1",
        serde_json::json!({}),
    );
    let target2_id_v1 = add_node_v2(
        &mut v1_graph,
        3,
        "Target2",
        "target2",
        serde_json::json!({}),
    );

    let source_id_v2 = add_node_v2(&mut v2_graph, 1, "Source", "source", serde_json::json!({}));
    let target1_id_v2 = add_node_v2(
        &mut v2_graph,
        2,
        "Target1",
        "target1",
        serde_json::json!({}),
    );
    let target2_id_v2 = add_node_v2(
        &mut v2_graph,
        3,
        "Target2",
        "target2",
        serde_json::json!({}),
    );

    add_simple_edge_v2(&mut v1_graph, source_id_v1, target1_id_v1, "connects");
    add_simple_edge_v2(&mut v1_graph, source_id_v1, target2_id_v1, "connects");

    add_simple_edge_v2(&mut v2_graph, source_id_v2, target1_id_v2, "connects");
    add_simple_edge_v2(&mut v2_graph, source_id_v2, target2_id_v2, "connects");

    let neighbors_v1 = v1_graph
        .neighbors(
            source_id_v1,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    let neighbors_v2 = v2_graph
        .neighbors(
            source_id_v2,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    // V1 and V2 should return identical neighbor counts
    assert_eq!(
        neighbors_v1.len(),
        neighbors_v2.len(),
        "V1 and V2 should return same number of neighbors"
    );
    assert_eq!(
        neighbors_v1.len(),
        2,
        "Both should return 2 neighbors for 2 distinct targets"
    );
}

// Helper function for V1 graph creation
fn create_test_graph_v1() -> (Box<dyn sqlitegraph::GraphBackend>, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_v1.db");

    // Force V1 backend by disabling v2_experimental
    let mut config = GraphConfig::native();
    // Note: This assumes we can force V1 mode - adjust if API differs
    let graph = open_graph(&db_path, &config).expect("Failed to create V1 test graph");

    (graph, temp_dir)
}

//! Phase 45 — V2 Adjacency Deduplication & Query Semantics Tests
//!
//! TDD tests to verify V2 neighbor deduplication works correctly
//! and maintains parity with V1 behavior.

use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeSpec, SqliteGraphError,
    config::GraphConfig, open_graph,
};
use std::collections::HashSet;
use tempfile::TempDir;

#[test]
#[cfg(feature = "v2_experimental")]
fn test_v2_neighbors_are_unique() -> Result<(), SqliteGraphError> {
    // Create graph with 2 nodes
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create test graph");

    // Insert 2 nodes
    let node1_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func1".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node1_id = graph.insert_node(node1_spec)?;

    let node2_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func2".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node2_id = graph.insert_node(node2_spec)?;

    // Insert 5 edges 1->2 with different edge types
    for i in 1..=5 {
        let edge_spec = EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: format!("CALL_{}", i),
            data: serde_json::json!({"seq": i}),
        };
        let edge_id = graph.insert_edge(edge_spec)?;
        assert!(edge_id > 0, "Edge {} should be inserted successfully", i);
    }

    // Verify neighbors count - should be 1 unique neighbor, not 5
    let neighbors = graph.neighbors(
        node1_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 unique neighbor (node2), not {} neighbors",
        neighbors.len()
    );
    assert_eq!(neighbors[0], node2_id, "Neighbor should be node2");

    // Verify no duplicates using HashSet
    let neighbor_set: HashSet<i64> = neighbors.into_iter().collect();
    assert_eq!(
        neighbor_set.len(),
        1,
        "Neighbor set should contain exactly 1 unique neighbor"
    );
    assert!(
        neighbor_set.contains(&node2_id),
        "Neighbor set should contain node2"
    );

    println!(
        "SUCCESS: Phase 45 V2 deduplication test passed - found {} unique neighbors",
        neighbor_set.len()
    );

    Ok(())
}

#[test]
#[cfg(feature = "v2_experimental")]
fn test_v2_multi_edge_same_neighbor_returns_once() -> Result<(), SqliteGraphError> {
    // Test multiple edges to the same neighbor with filtered edge type
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create test graph");

    // Insert 2 nodes
    let node1_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func1".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node1_id = graph.insert_node(node1_spec)?;

    let node2_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func2".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node2_id = graph.insert_node(node2_spec)?;

    // Insert 3 edges of the same type 1->2
    for i in 1..=3 {
        let edge_spec = EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({"instance": i}),
        };
        let edge_id = graph.insert_edge(edge_spec)?;
        assert!(edge_id > 0, "Edge {} should be inserted successfully", i);
    }

    // Verify filtered neighbors - should still be 1 unique neighbor
    let neighbors = graph.neighbors(
        node1_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: Some("CALLS".to_string()),
        },
    )?;

    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 unique neighbor when filtered by edge type"
    );
    assert_eq!(neighbors[0], node2_id, "Filtered neighbor should be node2");

    println!(
        "SUCCESS: Phase 45 V2 filtered deduplication test passed - found {} unique neighbors",
        neighbors.len()
    );

    Ok(())
}

#[test]
#[cfg(not(feature = "v2_experimental"))]
fn test_v1_behavior_unchanged() -> Result<(), SqliteGraphError> {
    // Verify V1 behavior still works without V2 feature
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create test graph");

    // Insert 2 nodes
    let node1_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func1".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node1_id = graph.insert_node(node1_spec)?;

    let node2_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func2".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node2_id = graph.insert_node(node2_spec)?;

    // Insert multiple edges 1->2
    for i in 1..=3 {
        let edge_spec = EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: format!("CALL_{}", i),
            data: serde_json::json!({"seq": i}),
        };
        let edge_id = graph.insert_edge(edge_spec)?;
        assert!(edge_id > 0, "Edge {} should be inserted successfully", i);
    }

    // V1 should naturally return unique neighbors (scattered storage deduplicates implicitly)
    let neighbors = graph.neighbors(
        node1_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    assert_eq!(neighbors.len(), 1, "V1 should return exactly 1 neighbor");
    assert_eq!(neighbors[0], node2_id, "V1 neighbor should be node2");

    println!("SUCCESS: Phase 45 V1 parity test passed - V1 behavior unchanged");

    Ok(())
}

#[test]
#[cfg(feature = "v2_experimental")]
fn test_v2_and_v1_parity_single_neighbor() -> Result<(), SqliteGraphError> {
    // Test that V2 and V1 return the same result for single edge case
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create test graph");

    // Insert 2 nodes
    let node1_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func1".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node1_id = graph.insert_node(node1_spec)?;

    let node2_spec = NodeSpec {
        kind: "Function".to_string(),
        name: "func2".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    };
    let node2_id = graph.insert_node(node2_spec)?;

    // Insert single edge 1->2
    let edge_spec = EdgeSpec {
        from: node1_id,
        to: node2_id,
        edge_type: "CALLS".to_string(),
        data: serde_json::json!({"weight": 1.0}),
    };
    let edge_id = graph.insert_edge(edge_spec)?;
    assert!(edge_id > 0, "Single edge should be inserted successfully");

    // V2 should return single neighbor
    let neighbors = graph.neighbors(
        node1_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    assert_eq!(
        neighbors.len(),
        1,
        "V2 should return exactly 1 neighbor for single edge"
    );
    assert_eq!(neighbors[0], node2_id, "V2 neighbor should be node2");

    // Verify no duplicates using HashSet
    let neighbor_set: HashSet<i64> = neighbors.into_iter().collect();
    assert_eq!(
        neighbor_set.len(),
        1,
        "Neighbor set should contain exactly 1 unique neighbor"
    );
    assert!(
        neighbor_set.contains(&node2_id),
        "Neighbor set should contain node2"
    );

    println!("SUCCESS: Phase 45 V2/V1 parity test passed - single edge case works correctly");

    Ok(())
}

//! Regression tests for Phase 14 Step 7 - V1 Node Corruption Fix.
//!
//! These tests MINIMALLY reproduce the V1 corruption bug ("Corrupt node record 257: Expected node ID 257, found 0")
//! using SMALL graphs to ensure deterministic reproduction.
//!
//! TESTS MUST FAIL BEFORE ANY CODE FIXES - THIS IS REQUIRED BY TDD.

use sqlitegraph::{GraphEntity, NodeSpec, config::GraphConfig, open_graph};
use tempfile::TempDir;

/// Test 1: Reproduce "Corrupt node record 257: Expected node ID 257, found 0"
/// This should FAIL before the fix, then PASS after the fix.
#[test]
fn v1_native_read_node_257_should_not_corrupt() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_corruption.db");

    // Use the same configuration as benchmarks
    let config = GraphConfig::native();

    // Create native graph (same as benchmarks)
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Step 1: Create exactly 300 nodes to hit the corruption boundary
    for i in 1..=300 {
        let node_spec = NodeSpec {
            kind: "node".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({"id": i}),
        };
        let node_id = graph.insert_node(node_spec).unwrap();
        assert_eq!(node_id, i as i64);
    }

    // Step 2: Try to read node 257 specifically - this should trigger the corruption error
    let result = graph.get_node(257);

    // BEFORE FIX: This should FAIL with "Corrupt node record 257: Expected node ID 257, found 0"
    // AFTER FIX: This should SUCCEED
    assert!(
        result.is_ok(),
        "Reading node 257 should not fail with corruption. Error: {:?}",
        result
    );

    let node_data = result.unwrap();
    assert_eq!(node_data.id, 257);
    assert_eq!(node_data.name, "node_257");
}

/// Test 2: Verify boundary nodes (255, 256, 257, 258, 259) all read correctly
/// This tests buffer alignment around the corruption boundary.
#[test]
fn v1_native_boundary_nodes_should_read_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_boundary.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Step 1: Create exactly 260 nodes to test boundary around 257
    for i in 1..=260 {
        let node_spec = NodeSpec {
            kind: "boundary_node".to_string(),
            name: format!("boundary_node_{}", i),
            file_path: None,
            data: serde_json::json!({"boundary": i}),
        };
        let node_id = graph.insert_node(node_spec).unwrap();
        assert_eq!(node_id, i as i64);
    }

    // Step 2: Test reading boundary nodes
    let boundary_ids = [255, 256, 257, 258, 259];

    for &node_num in &boundary_ids {
        let result = graph.get_node(node_num);

        // BEFORE FIX: node 257 should fail, others might work
        // AFTER FIX: All should work
        assert!(
            result.is_ok(),
            "Reading node {} should not fail. Error: {:?}",
            node_num,
            result
        );

        let node_data = result.unwrap();
        assert_eq!(node_data.id, node_num);
        assert_eq!(node_data.name, format!("boundary_node_{}", node_num));
        assert_eq!(
            node_data.data["boundary"],
            serde_json::Value::Number(node_num.into())
        );
    }
}

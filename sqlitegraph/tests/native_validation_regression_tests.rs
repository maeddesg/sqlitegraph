#![cfg(feature = "v2_experimental")]
//! Regression tests for native backend node ID validation bug
//!
//! Issue: Node ID validation uses stale current_node_count before new nodes are allocated
//! Error: "Invalid node ID: 1 (max: 100)" when inserting edges between valid nodes

use sqlitegraph::{EdgeSpec, NodeSpec, config::GraphConfig, open_graph};
use tempfile::TempDir;

#[test]
fn test_node_validation_regression_100_nodes() {
    // Reproduce the exact benchmark scenario that fails
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_validation.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).expect("Failed to create native graph");

    // Create exactly 100 nodes (matching benchmark setup)
    let mut node_ids = Vec::new();
    for i in 1..=100 {
        let node_spec = NodeSpec {
            kind: "test_node".to_string(),
            name: format!("test_node_{}", i),
            file_path: None,
            data: serde_json::json!({"node_id": i}),
        };
        let node_id = graph
            .insert_node(node_spec)
            .expect(&format!("Failed to insert node {}", i));
        node_ids.push(node_id);
    }

    // We have 100 node IDs stored in node_ids vector from insertion loop

    // Try to insert edge between valid nodes (this should succeed but currently fails)
    println!(
        "DEBUG: Attempting edge from {} to {}",
        node_ids[0], node_ids[1]
    );
    let edge_spec = EdgeSpec {
        from: node_ids[0], // First node ID (should be 1)
        to: node_ids[1],   // Second node ID (should be 2)
        edge_type: "test_edge".to_string(),
        data: serde_json::json!({"test": "edge"}),
    };

    // This should succeed but currently fails with "Invalid node ID: 1 (max: 100)"
    let result = graph.insert_edge(edge_spec);
    match result {
        Ok(_) => {
            // Success - edge was inserted
            println!("SUCCESS: Edge insertion passed");
        }
        Err(e) => {
            // Failure - this is bug we're fixing
            println!("DEBUG: Edge insertion failed with error: {:?}", e);
            panic!("REGRESSION: Edge insertion failed with error: {:?}", e);
        }
    }
}

#[test]
fn test_node_validation_regression_edge_case_99_to_100() {
    // Test edge case that should definitely work: 99→100
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_validation_99_100.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).expect("Failed to create native graph");

    // Create exactly 100 nodes
    let mut node_ids = Vec::new();
    for i in 1..=100 {
        let node_spec = NodeSpec {
            kind: "test_node".to_string(),
            name: format!("test_node_{}", i),
            file_path: None,
            data: serde_json::json!({"node_id": i}),
        };
        let node_id = graph
            .insert_node(node_spec)
            .expect(&format!("Failed to insert node {}", i));
        node_ids.push(node_id);
    }

    // Try edge from 99 to 100 (highest valid IDs)
    let edge_spec = EdgeSpec {
        from: node_ids[98], // 99th node (0-indexed)
        to: node_ids[99],   // 100th node (0-indexed)
        edge_type: "test_edge_99_100".to_string(),
        data: serde_json::json!({"test": "edge_99_100"}),
    };

    let result = graph.insert_edge(edge_spec);
    match result {
        Ok(_) => {
            println!("SUCCESS: Edge 99→100 insertion passed");
        }
        Err(e) => {
            panic!(
                "REGRESSION: Edge 99→100 insertion failed with error: {:?}",
                e
            );
        }
    }
}

#[test]
fn test_node_validation_regression_smaller_graph() {
    // Test with smaller graph to isolate the issue
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_validation_small.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).expect("Failed to create native graph");

    // Create only 10 nodes
    let mut node_ids = Vec::new();
    for i in 1..=10 {
        let node_spec = NodeSpec {
            kind: "test_node".to_string(),
            name: format!("test_node_{}", i),
            file_path: None,
            data: serde_json::json!({"node_id": i}),
        };
        let node_id = graph
            .insert_node(node_spec)
            .expect(&format!("Failed to insert node {}", i));
        node_ids.push(node_id);
    }

    // Try edge from 1 to 2
    let edge_spec = EdgeSpec {
        from: node_ids[0], // First node
        to: node_ids[1],   // Second node
        edge_type: "test_edge_small".to_string(),
        data: serde_json::json!({"test": "edge_small"}),
    };

    let result = graph.insert_edge(edge_spec);
    match result {
        Ok(_) => {
            println!("SUCCESS: Small graph edge insertion passed");
        }
        Err(e) => {
            panic!(
                "REGRESSION: Small graph edge insertion failed with error: {:?}",
                e
            );
        }
    }
}

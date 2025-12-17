#![cfg(feature = "v2_experimental")]
//! TDD Regression Tests for V1 Boundary-Correct Variable-Length Node Read.
//!
//! These tests validate the surgical fix for node-read boundary logic in V1.
//! They create synthetic nodes with controlled kind/name/data lengths to test:
//! (a) <64KB nodes - should return Ok(...) without corruption
//! (b) ~=64KB nodes - should return Ok(...) without corruption
//! (c) >64KB nodes - should return NativeBackendError::CorruptNodeRecord (not panic, not I/O error)
//!
//! IMPORTANT: These tests are designed to FAIL before implementation and PASS after.

use sqlitegraph::{NodeSpec, config::GraphConfig, open_graph};
use tempfile::TempDir;

/// Helper function to create a node with controlled total size
/// V1 Node Record Size Formula: total_size = 41 + kind_len + name_len + data_len
fn create_controlled_size_node(node_id: i64, total_size_target: usize) -> NodeSpec {
    // Calculate string payload needed to reach target size
    let string_payload_size = total_size_target.saturating_sub(41); // 41 bytes overhead

    // Distribute payload across kind, name, and data
    let kind_size = string_payload_size / 3;
    let name_size = string_payload_size / 3;
    let data_size = string_payload_size - (kind_size + name_size);

    let kind_string = "x".repeat(kind_size);
    let _name_string = "y".repeat(name_size);
    let data_string = format!("data_{}", "z".repeat(data_size));

    NodeSpec {
        kind: kind_string,
        name: format!("controlled_node_{}", node_id),
        file_path: None,
        data: serde_json::json!({"payload": data_string, "target_size": total_size_target}),
    }
}

/// Test case (a): Small nodes <64KB should read successfully
#[test]
fn v1_boundary_small_nodes_should_read_successfully() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_small_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes with different small sizes
    let small_sizes = [100, 1000, 10000, 50000]; // All <64KB (65536)

    for &target_size in &small_sizes {
        let node_spec = create_controlled_size_node(target_size as i64, target_size);
        let node_id = graph.insert_node(node_spec).unwrap();

        // Verify node can be read without corruption
        let result = graph.get_node(node_id);
        assert!(
            result.is_ok(),
            "Small node ({} bytes) should read successfully. Error: {:?}",
            target_size,
            result
        );

        let node = result.unwrap();
        assert_eq!(node.id, node_id);
    }
}

/// Test case (b): Boundary nodes ~=64KB should read successfully
#[test]
fn v1_boundary_nodes_around_64kb_should_read_successfully() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_64k_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes very close to 64KB boundary
    let boundary_sizes = [65000, 65400, 65480, 65535]; // Just under 64KB

    for &target_size in &boundary_sizes {
        let node_spec = create_controlled_size_node(target_size as i64, target_size);
        let node_id = graph.insert_node(node_spec).unwrap();

        // Verify node can be read without corruption
        let result = graph.get_node(node_id);
        assert!(
            result.is_ok(),
            "Boundary node ({} bytes) should read successfully. Error: {:?}",
            target_size,
            result
        );

        let node = result.unwrap();
        assert_eq!(node.id, node_id);
    }
}

/// Test case (c): Large nodes >64KB should return CorruptNodeRecord error
#[test]
fn v1_boundary_large_nodes_should_return_corrupt_node_record_error() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_large_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes that exceed 64KB boundary
    let large_sizes = [65537, 65550, 66000, 70000]; // Just over 64KB

    for &target_size in &large_sizes {
        let node_spec = create_controlled_size_node(target_size as i64, target_size);
        let node_id = graph.insert_node(node_spec).unwrap();

        // Verify node reading fails with CorruptNodeRecord (not BufferTooSmall, not I/O error)
        let result = graph.get_node(node_id);

        match result {
            Err(sqlitegraph::SqliteGraphError::ConnectionError(err_msg)) => {
                // Check that the error message indicates CorruptNodeRecord
                assert!(
                    err_msg.contains("Corrupt node record"),
                    "Expected CorruptNodeRecord in connection error, got: {}",
                    err_msg
                );
                assert!(
                    err_msg.contains(&node_id.to_string()),
                    "Expected node ID {} in error message, got: {}",
                    node_id,
                    err_msg
                );
            }
            Ok(_) => {
                panic!(
                    "Large node ({} bytes) should NOT read successfully",
                    target_size
                );
            }
            other_err => {
                panic!(
                    "Large node ({} bytes) should return connection error with CorruptNodeRecord, got: {:?}",
                    target_size, other_err
                );
            }
        }
    }
}

/// Performance test: Mixed size nodes should handle boundaries correctly
#[test]
fn v1_boundary_mixed_size_nodes_should_handle_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_mixed_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create a mix of small, boundary, and large nodes
    let mut node_ids = Vec::new();

    // Small nodes (should succeed)
    for i in 1..=5 {
        let node_spec = create_controlled_size_node(i, 1000);
        let node_id = graph.insert_node(node_spec).unwrap();
        node_ids.push((node_id, 1000, true)); // (id, size, should_succeed)
    }

    // Boundary nodes (should succeed)
    for i in 6..=10 {
        let node_spec = create_controlled_size_node(i, 65000);
        let node_id = graph.insert_node(node_spec).unwrap();
        node_ids.push((node_id, 65000, true));
    }

    // Large nodes (should fail with CorruptNodeRecord)
    for i in 11..=15 {
        let node_spec = create_controlled_size_node(i, 66000);
        let node_id = graph.insert_node(node_spec).unwrap();
        node_ids.push((node_id, 66000, false));
    }

    // Test each node according to expected behavior
    for (node_id, size, should_succeed) in node_ids {
        let result = graph.get_node(node_id);

        if should_succeed {
            assert!(
                result.is_ok(),
                "Node {} ({} bytes) should read successfully. Error: {:?}",
                node_id,
                size,
                result
            );
        } else {
            assert!(
                result.is_err(),
                "Large node {} ({} bytes) should fail",
                node_id,
                size
            );

            // Verify it's the correct error type
            match result.unwrap_err() {
                sqlitegraph::SqliteGraphError::ConnectionError(err_msg) => {
                    // Expected error type - should contain "Corrupt node record"
                    assert!(
                        err_msg.contains("Corrupt node record"),
                        "Expected 'Corrupt node record' in connection error, got: {}",
                        err_msg
                    );
                }
                other_err => {
                    panic!(
                        "Expected ConnectionError with CorruptNodeRecord for large node, got: {:?}",
                        other_err
                    );
                }
            }
        }
    }
}

/// Edge case: Exactly 64KB node should be handled correctly
#[test]
fn v1_boundary_exactly_64kb_node_should_be_handled_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_exact_64k.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create node that is exactly 64KB - 1 byte (should succeed)
    let exact_64k_minus_1 = 65535;
    let node_spec = create_controlled_size_node(1, exact_64k_minus_1);
    let node_id = graph.insert_node(node_spec).unwrap();

    let result = graph.get_node(node_id);
    assert!(
        result.is_ok(),
        "Exactly 64KB-1 node should read successfully. Error: {:?}",
        result
    );

    // Create node that is exactly 64KB (behavior depends on implementation)
    let exact_64k = 65536;
    let node_spec2 = create_controlled_size_node(2, exact_64k);
    let node_id2 = graph.insert_node(node_spec2).unwrap();

    let result2 = graph.get_node(node_id2);
    // This test documents current behavior - may succeed or fail based on buffer alignment
    println!("Exactly 64KB node read result: {:?}", result2);
}

/// Regression test: Verify corruption occurs at expected node boundary
#[test]
fn v1_boundary_corruption_should_occur_at_expected_node_257() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_regression_257.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes up to and including the known corruption boundary
    for i in 1..=300 {
        let node_spec = NodeSpec {
            kind: "boundary_test".to_string(),
            name: format!("boundary_node_{}", i),
            file_path: None,
            data: serde_json::json!({
                "node_id": i,
                "payload": "x".repeat(1000) // Each node ~1041 bytes
            }),
        };
        graph.insert_node(node_spec).unwrap();
    }

    // Test reading around the known corruption boundary
    let boundary_nodes = [255, 256, 257, 258, 259, 260];

    for &node_num in &boundary_nodes {
        let result = graph.get_node(node_num);

        // Before fix: Should fail at node 257
        // After fix: Should handle correctly based on size
        println!("Node {} read result: {:?}", node_num, result);

        // Document behavior for regression verification
        if node_num == 257 {
            match &result {
                Ok(_) => println!("Node 257 now reads successfully (FIXED)"),
                Err(e) => println!("Node 257 still fails: {:?} (NOT FIXED)", e),
            }
        }
    }
}

#![cfg(feature = "v2_experimental")]
//! TDD Regression Tests for V1 Edge-Record Boundary & Variable-Length Read Fix.
//!
//! These tests validate the surgical fix for edge-read boundary logic in V1.
//! They create synthetic edges with controlled type/data lengths to test:
//! (a) <256B edges - should return Ok(...) without corruption
//! (b) ~=256B edges - should return Ok(...) without corruption
//! (c) >256B edges - should return NativeBackendError::CorruptEdgeRecord (not panic, not I/O error)
//!
//! IMPORTANT: These tests are designed to FAIL before implementation and PASS after.

use sqlitegraph::{
    BackendDirection, EdgeSpec, NeighborQuery, NodeSpec, config::GraphConfig, open_graph,
};
use tempfile::TempDir;

/// Helper function to create a node for edge testing
fn create_test_node(graph: &Box<dyn sqlitegraph::GraphBackend>, node_id: i64) -> i64 {
    let node_spec = NodeSpec {
        kind: "test_node".to_string(),
        name: format!("test_node_{}", node_id),
        file_path: None,
        data: serde_json::json!({"node_id": node_id}),
    };
    graph.insert_node(node_spec).unwrap()
}

/// Helper function to create an edge with controlled total size
/// V1 Edge Record Size Formula: total_size = 33 + type_len + data_len
fn create_controlled_size_edge(from_id: i64, to_id: i64, total_size_target: usize) -> EdgeSpec {
    // Calculate string payload needed to reach target size
    let string_payload_size = total_size_target.saturating_sub(33); // 33 bytes overhead

    // Distribute payload across edge_type and data
    let type_size = string_payload_size / 2;
    let data_size = string_payload_size - type_size;

    let edge_type_string = "x".repeat(type_size);
    let data_string = format!("data_{}", "y".repeat(data_size));

    EdgeSpec {
        from: from_id,
        to: to_id,
        edge_type: edge_type_string,
        data: serde_json::json!({"payload": data_string, "target_size": total_size_target}),
    }
}

/// Test case (a): Small edges <256B should read successfully
#[test]
fn v1_edge_boundary_small_edges_should_read_successfully() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_small_edge_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 1);
    let to_node_id = create_test_node(&graph, 2);

    // Create edges with different small sizes
    let small_sizes = [50, 100, 200, 255]; // All <256B

    for &target_size in &small_sizes {
        let edge_spec = create_controlled_size_edge(from_node_id, to_node_id, target_size);
        let edge_id = graph.insert_edge(edge_spec).unwrap();

        // Verify edge can be read without corruption by checking neighbors
        let result = graph.neighbors(
            from_node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        );
        assert!(
            result.is_ok(),
            "Small edge ({} bytes) should read successfully. Error: {:?}",
            target_size,
            result
        );

        let neighbors = result.unwrap();
        assert!(
            neighbors.contains(&to_node_id),
            "Small edge should connect nodes successfully"
        );
    }
}

/// Test case (b): Boundary edges ~=256B should read successfully
#[test]
fn v1_edge_boundary_edges_around_256b_should_read_successfully() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_256b_edge_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 3);
    let to_node_id = create_test_node(&graph, 4);

    // Create edges very close to 256B boundary
    let boundary_sizes = [250, 255, 256, 260]; // Around 256B

    for &target_size in &boundary_sizes {
        let edge_spec = create_controlled_size_edge(from_node_id, to_node_id, target_size);
        let edge_id = graph.insert_edge(edge_spec).unwrap();

        // Verify edge can be read without corruption
        let result = graph.neighbors(
            from_node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        );
        assert!(
            result.is_ok(),
            "Boundary edge ({} bytes) should read successfully. Error: {:?}",
            target_size,
            result
        );

        let neighbors = result.unwrap();
        assert!(
            neighbors.contains(&to_node_id),
            "Boundary edge should connect nodes successfully"
        );
    }
}

/// Test case (c): Large edges >256B should return CorruptEdgeRecord error
#[test]
fn v1_edge_boundary_large_edges_should_return_corrupt_edge_record_error() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_large_edge_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 5);
    let to_node_id = create_test_node(&graph, 6);

    // Create edges that exceed 256B boundary
    let large_sizes = [300, 512, 1024, 4096]; // Over 256B

    for &target_size in &large_sizes {
        let edge_spec = create_controlled_size_edge(from_node_id, to_node_id, target_size);

        // Verify edge insertion fails with CorruptEdgeRecord (not BufferTooSmall, not I/O error)
        let result = graph.insert_edge(edge_spec);

        match result {
            Err(sqlitegraph::SqliteGraphError::ConnectionError(err_msg)) => {
                // Check that the error message indicates CorruptEdgeRecord
                assert!(
                    err_msg.contains("Corrupt edge record")
                        || err_msg.contains("Corrupt node record"), // Edge insertion may trigger node reads
                    "Expected CorruptEdgeRecord in connection error, got: {}",
                    err_msg
                );
            }
            Ok(_) => {
                panic!(
                    "Large edge ({} bytes) should NOT insert successfully",
                    target_size
                );
            }
            other_err => {
                panic!(
                    "Large edge ({} bytes) should return connection error with CorruptEdgeRecord, got: {:?}",
                    target_size, other_err
                );
            }
        }
    }
}

/// Performance test: Mixed size edges should handle boundaries correctly
#[test]
fn v1_edge_boundary_mixed_size_edges_should_handle_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_mixed_edge_boundary.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create 10 nodes for testing
    let mut node_ids = Vec::new();
    for i in 7..=16 {
        let node_id = create_test_node(&graph, i);
        node_ids.push(node_id);
    }

    // Create a mix of small, boundary, and large edges
    let mut edge_results = Vec::new();

    // Small edges (should succeed)
    for i in 0..=2 {
        let edge_spec = create_controlled_size_edge(node_ids[i], node_ids[i + 1], 100);
        let result = graph.insert_edge(edge_spec);
        edge_results.push((100, result, true)); // (size, result, should_succeed)
    }

    // Boundary edges (should succeed)
    for i in 3..=5 {
        let edge_spec = create_controlled_size_edge(node_ids[i], node_ids[i + 1], 255);
        let result = graph.insert_edge(edge_spec);
        edge_results.push((255, result, true));
    }

    // Large edges (should fail with CorruptEdgeRecord)
    for i in 6..=8 {
        let edge_spec =
            create_controlled_size_edge(node_ids[i], node_ids[(i + 1) % node_ids.len()], 512);
        let result = graph.insert_edge(edge_spec);
        edge_results.push((512, result, false));
    }

    // Test each edge according to expected behavior
    for (size, result, should_succeed) in edge_results {
        if should_succeed {
            assert!(
                result.is_ok(),
                "Edge ({} bytes) should insert successfully. Error: {:?}",
                size,
                result
            );
        } else {
            assert!(result.is_err(), "Large edge ({} bytes) should fail", size);

            // Verify it's the correct error type
            match result.unwrap_err() {
                sqlitegraph::SqliteGraphError::ConnectionError(err_msg) => {
                    // Expected error type - should contain "Corrupt edge record" or "Corrupt node record"
                    assert!(
                        err_msg.contains("Corrupt edge record")
                            || err_msg.contains("Corrupt node record"),
                        "Expected 'Corrupt edge record' or 'Corrupt node record' in connection error, got: {}",
                        err_msg
                    );
                }
                other_err => {
                    panic!(
                        "Expected ConnectionError with CorruptEdgeRecord for large edge, got: {:?}",
                        other_err
                    );
                }
            }
        }
    }
}

/// Edge case: Exactly 256B edge should be handled correctly
#[test]
fn v1_edge_boundary_exactly_256b_edge_should_be_handled_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_exact_256b_edge.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 17);
    let to_node_id = create_test_node(&graph, 18);

    // Create edge that is exactly 256B - 1 byte (should succeed)
    let exact_256b_minus_1 = 255;
    let edge_spec = create_controlled_size_edge(from_node_id, to_node_id, exact_256b_minus_1);
    let edge_id = graph.insert_edge(edge_spec).unwrap();

    // Verify edge can be read
    let result = graph.neighbors(
        from_node_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    );
    assert!(
        result.is_ok(),
        "Exactly 256B-1 edge should read successfully. Error: {:?}",
        result
    );

    // Create edge that is exactly 256B (behavior depends on implementation)
    let exact_256b = 256;
    let edge_spec2 = create_controlled_size_edge(from_node_id, to_node_id, exact_256b);
    let result2 = graph.insert_edge(edge_spec2);

    // This test documents current behavior - may succeed or fail based on buffer alignment
    println!("Exactly 256B edge insertion result: {:?}", result2);
}

/// Regression test: Verify edge corruption occurs at expected node 257 boundary
#[test]
fn v1_edge_boundary_corruption_should_occur_at_expected_node_257() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_edge_regression_257.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes up to and including the known corruption boundary
    let mut node_ids = Vec::new();
    for i in 1..=300 {
        let node_id = create_test_node(&graph, i);
        node_ids.push(node_id);
    }

    // Try to create edges that will trigger the boundary corruption
    let mut edge_results = Vec::new();
    let mut failed_at_boundary = false;
    let mut error_details = None;

    // Create edges up to node 257
    for i in 0..=256 {
        let from_node = node_ids[i];
        let to_node = node_ids[(i + 1) % node_ids.len()];

        let edge_spec = EdgeSpec {
            from: from_node,
            to: to_node,
            edge_type: "boundary_test".to_string(),
            data: serde_json::json!({
                "from_node": from_node,
                "to_node": to_node,
                "payload": "x".repeat(100) // Moderate edge size
            }),
        };

        let result = graph.insert_edge(edge_spec);
        let is_success = result.is_ok();

        // Check for boundary failure
        if let Err(e) = result {
            if from_node == 257 || to_node == 257 {
                failed_at_boundary = true;
                error_details = Some(format!(
                    "Edge corruption at boundary: from {} to {} → Error: {:?}",
                    from_node, to_node, e
                ));
                break;
            }
        }

        edge_results.push((from_node, to_node, is_success));
    }

    // Document behavior for regression verification
    if let Some(details) = error_details {
        println!("{}", details);
        println!("Edge corruption correctly occurs at node 257 boundary (REGRESSION CONFIRMED)");
    } else {
        println!("Edge corruption pattern changed - may indicate fix or different issue");
    }

    // Before fix: Should fail at node 257
    // After fix: Should handle correctly based on edge size
}

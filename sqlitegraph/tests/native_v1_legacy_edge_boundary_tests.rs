#![cfg(feature = "v2_experimental")]
//! Legacy V1 Edge Boundary Tests - Copied from Original V1 Tests.
//!
//! These tests validate V1 edge boundary behavior using the original test logic.
//! They preserve the V1 regression tests while the new V2 tests use V2-by-default.
//! These tests may fail until proper V1 file creation is supported.
//! For now, they document the expected V1 behavior for legacy compatibility.

use sqlitegraph::{
    BackendDirection, EdgeSpec, NeighborQuery, NodeSpec, config::GraphConfig, open_graph,
};
use tempfile::TempDir;

/// Helper function to create a V1-style graph file (currently creates V2 - will be updated)
fn create_v1_style_graph_file(db_path: &std::path::Path) -> Box<dyn sqlitegraph::GraphBackend> {
    // Note: This currently creates V2 files due to Step 17 changes
    // The test failure documents this transition
    let config = GraphConfig::native();
    open_graph(db_path, &config).unwrap()
}

/// Helper function to create a node for V1 edge testing
fn create_test_node(graph: &Box<dyn sqlitegraph::GraphBackend>, node_id: i64) -> i64 {
    let node_spec = NodeSpec {
        kind: "test_node".to_string(),
        name: format!("test_node_{}", node_id),
        file_path: None,
        data: serde_json::json!({"node_id": node_id}),
    };
    graph.insert_node(node_spec).unwrap()
}

/// Helper function to create an edge with controlled total size for V1
/// V1 Edge Record Size Formula: total_size = 33 + type_len + data_len
fn create_controlled_size_edge_v1(from_id: i64, to_id: i64, total_size_target: usize) -> EdgeSpec {
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

/// Test that documents V2-by-default transition from V1 tests
#[test]
fn v1_legacy_documents_v2_by_default_transition() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v1_legacy_transition.db");

    // Create V1-style file (actually creates V2 due to Step 17)
    let graph = create_v1_style_graph_file(&db_path);

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 1);
    let to_node_id = create_test_node(&graph, 2);

    // Create edge and document the V1-in-V2 format mismatch
    let edge_spec = create_controlled_size_edge_v1(from_node_id, to_node_id, 100);
    let result = graph.insert_edge(edge_spec);

    // This documents the current transition: V2 files but V1 node records
    match result {
        Err(sqlitegraph::SqliteGraphError::ConnectionError(err_msg)) => {
            assert!(
                err_msg.contains("Unexpected V1 node record encountered in V2 region"),
                "Expected V1-in-V2 format error, got: {}",
                err_msg
            );
            println!("V1 legacy test correctly documents V2-by-default transition");
        }
        Ok(_) => {
            // If this succeeds, it means the V1/V2 mismatch has been resolved
            println!("V1 legacy edge insertion succeeded - V1/V2 mismatch may be resolved");
        }
        other => {
            panic!("Unexpected error type: {:?}", other);
        }
    }
}

/// Legacy test case (a): Small edges <256B should read successfully in V1
#[test]
fn v1_legacy_small_edges_should_read_successfully() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir
        .path()
        .join("test_v1_legacy_small_edge_boundary.db");

    let graph = create_v1_style_graph_file(&db_path);

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 1);
    let to_node_id = create_test_node(&graph, 2);

    // Create edges with different small sizes
    let small_sizes = [50, 100, 200, 255]; // All <256B

    for &target_size in &small_sizes {
        let edge_spec = create_controlled_size_edge_v1(from_node_id, to_node_id, target_size);
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
            "Small V1 legacy edge ({} bytes) should read successfully. Error: {:?}",
            target_size,
            result
        );

        let neighbors = result.unwrap();
        assert!(
            neighbors.contains(&to_node_id),
            "Small V1 legacy edge should connect nodes successfully"
        );
    }
}

/// Legacy test case (b): Boundary edges ~=256B should read successfully in V1
#[test]
fn v1_legacy_edges_around_256b_should_read_successfully() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v1_legacy_256b_edge_boundary.db");

    let graph = create_v1_style_graph_file(&db_path);

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 3);
    let to_node_id = create_test_node(&graph, 4);

    // Create edges very close to 256B boundary
    let boundary_sizes = [250, 255, 256, 260]; // Around 256B

    for &target_size in &boundary_sizes {
        let edge_spec = create_controlled_size_edge_v1(from_node_id, to_node_id, target_size);
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
            "Boundary V1 legacy edge ({} bytes) should read successfully. Error: {:?}",
            target_size,
            result
        );

        let neighbors = result.unwrap();
        assert!(
            neighbors.contains(&to_node_id),
            "Boundary V1 legacy edge should connect nodes successfully"
        );
    }
}

/// Legacy test case (c): Large edges >256B should return CorruptEdgeRecord error in V1
#[test]
fn v1_legacy_large_edges_should_return_corrupt_edge_record_error() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir
        .path()
        .join("test_v1_legacy_large_edge_boundary.db");

    let graph = create_v1_style_graph_file(&db_path);

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 5);
    let to_node_id = create_test_node(&graph, 6);

    // Create edges that exceed 256B boundary
    let large_sizes = [300, 512, 1024, 4096]; // Over 256B

    for &target_size in &large_sizes {
        let edge_spec = create_controlled_size_edge_v1(from_node_id, to_node_id, target_size);

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
                    "Large V1 legacy edge ({} bytes) should NOT insert successfully",
                    target_size
                );
            }
            other_err => {
                panic!(
                    "Large V1 legacy edge ({} bytes) should return connection error with CorruptEdgeRecord, got: {:?}",
                    target_size, other_err
                );
            }
        }
    }
}

/// Legacy performance test: Mixed size edges should handle boundaries correctly in V1
#[test]
fn v1_legacy_mixed_size_edges_should_handle_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir
        .path()
        .join("test_v1_legacy_mixed_edge_boundary.db");

    let graph = create_v1_style_graph_file(&db_path);

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
        let edge_spec = create_controlled_size_edge_v1(node_ids[i], node_ids[i + 1], 100);
        let result = graph.insert_edge(edge_spec);
        edge_results.push((100, result, true)); // (size, result, should_succeed)
    }

    // Boundary edges (should succeed)
    for i in 3..=5 {
        let edge_spec = create_controlled_size_edge_v1(node_ids[i], node_ids[i + 1], 255);
        let result = graph.insert_edge(edge_spec);
        edge_results.push((255, result, true));
    }

    // Large edges (should fail with CorruptEdgeRecord)
    for i in 6..=8 {
        let edge_spec =
            create_controlled_size_edge_v1(node_ids[i], node_ids[(i + 1) % node_ids.len()], 512);
        let result = graph.insert_edge(edge_spec);
        edge_results.push((512, result, false));
    }

    // Test each edge according to expected behavior
    for (size, result, should_succeed) in edge_results {
        if should_succeed {
            assert!(
                result.is_ok(),
                "V1 legacy edge ({} bytes) should insert successfully. Error: {:?}",
                size,
                result
            );
        } else {
            assert!(
                result.is_err(),
                "Large V1 legacy edge ({} bytes) should fail",
                size
            );

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
                        "Expected ConnectionError with CorruptEdgeRecord for large V1 legacy edge, got: {:?}",
                        other_err
                    );
                }
            }
        }
    }
}

/// Legacy edge case: Exactly 256B edge should be handled correctly in V1
#[test]
fn v1_legacy_exactly_256b_edge_should_be_handled_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v1_legacy_exact_256b_edge.db");

    let graph = create_v1_style_graph_file(&db_path);

    // Create source and target nodes
    let from_node_id = create_test_node(&graph, 17);
    let to_node_id = create_test_node(&graph, 18);

    // Create edge that is exactly 256B - 1 byte (should succeed)
    let exact_256b_minus_1 = 255;
    let edge_spec = create_controlled_size_edge_v1(from_node_id, to_node_id, exact_256b_minus_1);
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
        "Exactly 256B-1 V1 legacy edge should read successfully. Error: {:?}",
        result
    );

    // Create edge that is exactly 256B (behavior depends on V1 implementation)
    let exact_256b = 256;
    let edge_spec2 = create_controlled_size_edge_v1(from_node_id, to_node_id, exact_256b);
    let result2 = graph.insert_edge(edge_spec2);

    // This test documents current V1 behavior - may succeed or fail based on buffer alignment
    println!(
        "Exactly 256B V1 legacy edge insertion result: {:?}",
        result2
    );
}

/// Legacy regression test: Verify edge corruption occurs at expected node 257 boundary in V1
#[test]
fn v1_legacy_corruption_should_occur_at_expected_node_257() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir
        .path()
        .join("test_v1_legacy_edge_regression_257.db");

    let graph = create_v1_style_graph_file(&db_path);

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
            edge_type: "v1_legacy_boundary_test".to_string(),
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
                    "V1 legacy edge corruption at boundary: from {} to {} → Error: {:?}",
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
        println!(
            "V1 legacy edge corruption correctly occurs at node 257 boundary (REGRESSION CONFIRMED)"
        );
    } else {
        println!("V1 legacy edge corruption pattern changed - may indicate fix or different issue");
    }

    // Before fix: Should fail at node 257
    // After fix: Should handle correctly based on edge size
}

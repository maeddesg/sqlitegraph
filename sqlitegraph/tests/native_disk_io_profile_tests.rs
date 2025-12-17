#![cfg(feature = "v2_experimental")]
//! TDD I/O Regression Harness for V1 Native Disk I/O Profiling.
//!
//! These tests reproduce the specific I/O performance characteristics and corruption
//! issues identified in Phase 14 analysis. They validate:
//! (a) "good" V1 workloads (small/medium graphs)
//! (b) Large graphs that expose slow/random I/O patterns
//! (c) Corruption boundary conditions at node/edge 257
//! (d) Performance characteristics vs. SQLite backend

use sqlitegraph::{BackendDirection, EdgeSpec, NodeSpec, config::GraphConfig, open_graph};
use std::time::Instant;
use tempfile::TempDir;

/// Test good V1 workload: Small graph sequential access
/// Expected: Fast performance with no corruption
#[test]
fn v1_small_graph_sequential_access_should_perform_well() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_small.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create small chain graph (50 nodes)
    for i in 1..=50 {
        let node_spec = NodeSpec {
            kind: "test_node".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({"id": i}),
        };
        graph.insert_node(node_spec).unwrap();
    }

    // Create edges for chain topology
    for i in 1..50 {
        let edge_spec = EdgeSpec {
            from: i as i64,
            to: (i + 1) as i64,
            edge_type: "chain".to_string(),
            data: serde_json::json!({"from": i, "to": i + 1}),
        };
        graph.insert_edge(edge_spec).unwrap();
    }

    // Measure sequential access performance (1-hop from each node)
    let start = Instant::now();
    for i in 1..=50 {
        let neighbors = graph.k_hop(i, 1, BackendDirection::Outgoing).unwrap();
        assert!(
            neighbors.len() <= 1,
            "Chain node should have at most 1 neighbor"
        );
    }
    let duration = start.elapsed();

    // Performance assertion: Should complete within reasonable time
    assert!(
        duration.as_millis() < 100,
        "Sequential access to 50 nodes should complete quickly, took {:?}",
        duration
    );

    // Corruption check: All nodes should be readable
    for i in 1..=50 {
        let node = graph.get_node(i).expect("Node should be readable");
        assert_eq!(node.id, i);
    }
}

/// Test good V1 workload: Medium graph hub access pattern
/// Expected: Reasonable performance with cached hub node
#[test]
fn v1_medium_graph_star_topology_should_perform_reasonably() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_star.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create star graph: 1 center + 99 leaf nodes (100 total)
    let center_spec = NodeSpec {
        kind: "center".to_string(),
        name: "center".to_string(),
        file_path: None,
        data: serde_json::json!({"role": "center"}),
    };
    let center_id = graph.insert_node(center_spec).unwrap();

    for i in 1..=99 {
        let leaf_spec = NodeSpec {
            kind: "leaf".to_string(),
            name: format!("leaf_{}", i),
            file_path: None,
            data: serde_json::json!({"leaf_id": i}),
        };
        let leaf_id = graph.insert_node(leaf_spec).unwrap();

        // Connect leaf to center
        let edge_spec = EdgeSpec {
            from: center_id,
            to: leaf_id,
            edge_type: "star".to_string(),
            data: serde_json::json!({"leaf": i}),
        };
        graph.insert_edge(edge_spec).unwrap();
    }

    // Measure hub access performance (k-hop from center)
    let start = Instant::now();
    let neighbors = graph
        .k_hop(center_id, 1, BackendDirection::Outgoing)
        .unwrap();
    let duration = start.elapsed();

    // Performance assertion: Should complete within reasonable time
    assert!(
        duration.as_millis() < 50,
        "Hub access should complete quickly, took {:?}",
        duration
    );

    // Correctness check: Should reach all 99 leaf nodes
    assert_eq!(neighbors.len(), 99, "Center should reach all 99 leaves");

    // Corruption check: All nodes should be readable
    assert!(
        graph.get_node(center_id).is_ok(),
        "Center node should be readable"
    );
    for i in 1..=99 {
        let leaf_node = graph
            .get_node(center_id + i as i64)
            .expect("Leaf node should be readable");
        assert_eq!(leaf_node.kind, "leaf");
    }
}

/// Test corruption boundary: Node 257 corruption reproduction
/// Expected: Should FAIL before fix, then PASS after fix
#[test]
fn v1_node_257_boundary_should_not_corrupt() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_boundary_257.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create exactly 300 nodes to cross the 257 boundary
    for i in 1..=300 {
        let node_spec = NodeSpec {
            kind: "boundary_test".to_string(),
            name: format!("boundary_node_{}", i),
            file_path: None,
            data: serde_json::json!({"boundary_id": i}),
        };
        let node_id = graph.insert_node(node_spec).unwrap();
        assert_eq!(node_id, i as i64);
    }

    // Test reading boundary nodes around 257
    let boundary_nodes = [255, 256, 257, 258, 259];

    for &node_num in &boundary_nodes {
        let result = graph.get_node(node_num);

        // BEFORE FIX: node 257 should fail with corruption error
        // AFTER FIX: All boundary nodes should be readable
        assert!(
            result.is_ok(),
            "Reading boundary node {} should not fail with corruption. Error: {:?}",
            node_num,
            result
        );

        let node_data = result.unwrap();
        assert_eq!(node_data.id, node_num);
        assert_eq!(node_data.name, format!("boundary_node_{}", node_num));
    }
}

/// Test corruption boundary: Edge insertion corruption at node 257
/// Expected: Should FAIL before fix, then PASS after fix
#[test]
fn v1_edge_insertion_257_boundary_should_not_corrupt() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_edge_boundary_257.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create 300 nodes to ensure we cross the 257 boundary
    for i in 1..=300 {
        let node_spec = NodeSpec {
            kind: "edge_boundary_test".to_string(),
            name: format!("edge_node_{}", i),
            file_path: None,
            data: serde_json::json!({"node_id": i}),
        };
        graph.insert_node(node_spec).unwrap();
    }

    // Create edges that will hit the corruption boundary
    // Specifically connect node 257 to trigger the "Buffer too small" error
    let edge_spec = EdgeSpec {
        from: 257,
        to: 258,
        edge_type: "boundary_test".to_string(),
        data: serde_json::json!({"boundary": "test"}),
    };

    // BEFORE FIX: This should fail with "Buffer too small: 65536 bytes (need at least 65581 bytes)"
    // AFTER FIX: This should succeed
    let result = graph.insert_edge(edge_spec);
    assert!(
        result.is_ok(),
        "Edge insertion at node 257 boundary should not fail with corruption. Error: {:?}",
        result
    );
}

/// Test I/O amplification: Large random access pattern
/// Expected: Should demonstrate poor performance characteristics
#[test]
fn v1_large_random_access_should_show_io_amplification() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_random_io.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create large graph (500 nodes)
    for i in 1..=500 {
        let node_spec = NodeSpec {
            kind: "random_test".to_string(),
            name: format!("random_node_{}", i),
            file_path: None,
            data: serde_json::json!({"random_id": i}),
        };
        graph.insert_node(node_spec).unwrap();
    }

    // Create random edges to ensure adjacency access
    for i in 1..=400 {
        let from_id = (i % 500) + 1; // Ensure valid node IDs
        let to_id = ((i * 7) % 500) + 1; // Pseudo-random distribution

        if from_id != to_id {
            // Avoid self-loops
            let edge_spec = EdgeSpec {
                from: from_id as i64,
                to: to_id as i64,
                edge_type: "random".to_string(),
                data: serde_json::json!({"edge_id": i}),
            };
            graph.insert_edge(edge_spec).unwrap();
        }
    }

    // Measure random access performance (k-hop from random nodes)
    let random_nodes = [1, 50, 100, 150, 200, 250, 300, 350, 400, 450];
    let start = Instant::now();

    for &node_id in &random_nodes {
        let _neighbors = graph.k_hop(node_id, 1, BackendDirection::Outgoing).unwrap();
    }

    let duration = start.elapsed();

    // Performance assertion: Should show significant I/O amplification
    // This test documents the current poor performance, doesn't enforce a specific time
    println!(
        "Random access to 10 nodes in 500-node graph took: {:?}",
        duration
    );

    // Sanity check: Should complete without corruption
    for &node_id in &random_nodes {
        assert!(
            graph.get_node(node_id).is_ok(),
            "Random node access should not corrupt"
        );
    }
}

/// Test cache thrashing: More than 100 unique nodes to exceed thread-local cache
/// Expected: Should demonstrate cache thrashing behavior
#[test]
fn v1_cache_thrashing_should_occur_with_many_unique_nodes() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_cache_thrash.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create 150 nodes to exceed 100-entry thread-local cache
    for i in 1..=150 {
        let node_spec = NodeSpec {
            kind: "cache_test".to_string(),
            name: format!("cache_node_{}", i),
            file_path: None,
            data: serde_json::json!({"cache_id": i}),
        };
        graph.insert_node(node_spec).unwrap();
    }

    // Access nodes in order to fill cache, then access more to cause thrashing
    let start = Instant::now();

    // First, access 100 nodes to fill cache (should be fast)
    for i in 1..=100 {
        let _node = graph.get_node(i).unwrap();
    }

    let fill_time = start.elapsed();

    // Then access 50 more nodes to cause cache thrashing (should be slower)
    let thrash_start = Instant::now();
    for i in 101..=150 {
        let _node = graph.get_node(i).unwrap();
    }
    let thrash_time = thrash_start.elapsed();

    // Document cache behavior
    println!("Cache fill (100 nodes): {:?}", fill_time);
    println!("Cache thrash (50 nodes): {:?}", thrash_time);

    // Sanity check: All nodes should be readable despite cache thrashing
    for i in 1..=150 {
        assert!(
            graph.get_node(i).is_ok(),
            "All nodes should remain readable after cache thrashing"
        );
    }
}

/// Performance comparison test: Native vs SQLite for small graphs
/// Expected: Native should be competitive for small workloads
#[test]
fn v1_vs_sqlite_small_graph_performance_comparison() {
    // Test with Native backend
    let native_temp = TempDir::new().unwrap();
    let native_path = native_temp.path().join("test_native.db");
    let native_config = GraphConfig::native();
    let mut native_graph = open_graph(&native_path, &native_config).unwrap();

    // Create identical small graph in both backends
    for i in 1..=50 {
        let node_spec = NodeSpec {
            kind: "comparison".to_string(),
            name: format!("comp_node_{}", i),
            file_path: None,
            data: serde_json::json!({"comp_id": i}),
        };
        native_graph.insert_node(node_spec).unwrap();
    }

    // Measure native performance
    let native_start = Instant::now();
    for i in 1..=50 {
        let _node = native_graph.get_node(i).unwrap();
    }
    let native_time = native_start.elapsed();

    // Test with SQLite backend
    let sqlite_temp = TempDir::new().unwrap();
    let sqlite_path = sqlite_temp.path().join("test_sqlite.db");
    let sqlite_config = GraphConfig::sqlite();
    let mut sqlite_graph = open_graph(&sqlite_path, &sqlite_config).unwrap();

    for i in 1..=50 {
        let node_spec = NodeSpec {
            kind: "comparison".to_string(),
            name: format!("comp_node_{}", i),
            file_path: None,
            data: serde_json::json!({"comp_id": i}),
        };
        sqlite_graph.insert_node(node_spec).unwrap();
    }

    // Measure SQLite performance
    let sqlite_start = Instant::now();
    for i in 1..=50 {
        let _node = sqlite_graph.get_node(i).unwrap();
    }
    let sqlite_time = sqlite_start.elapsed();

    // Document performance comparison
    println!("Native backend (50 nodes): {:?}", native_time);
    println!("SQLite backend (50 nodes): {:?}", sqlite_time);

    // Both should complete without errors
    for i in 1..=50 {
        assert!(
            native_graph.get_node(i).is_ok(),
            "Native graph should remain readable"
        );
        assert!(
            sqlite_graph.get_node(i).is_ok(),
            "SQLite graph should remain readable"
        );
    }
}

/// Test file size efficiency: V1 vs expected compact storage
/// Expected: V1 should show significant space overhead
#[test]
fn v1_should_show_significant_space_overhead() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_space_overhead.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).unwrap();

    // Create 100 nodes
    let node_count = 100;
    for i in 1..=node_count {
        let node_spec = NodeSpec {
            kind: "space_test".to_string(),
            name: format!("space_node_{}", i),
            file_path: None,
            data: serde_json::json!({"space_id": i}),
        };
        graph.insert_node(node_spec).unwrap();
    }

    // Force file to disk
    drop(graph);

    // Check file size
    let file_size = std::fs::metadata(&db_path).unwrap().len();

    // Expected: V1 should use ~4KB per node = 400KB for 100 nodes
    // Actual node data is ~41 bytes, so expected overhead is ~100x
    let expected_minimum = node_count * 4096; // 4KB per node
    let actual_data_size = node_count * 41; // ~41 bytes per node record

    println!("V1 file size for {} nodes: {} bytes", node_count, file_size);
    println!("Expected minimum (4KB slots): {} bytes", expected_minimum);
    println!("Actual data needed (41B each): {} bytes", actual_data_size);
    println!(
        "Space overhead factor: {:.1}x",
        file_size as f64 / actual_data_size as f64
    );

    // Sanity check: File should be at least the expected minimum
    assert!(
        file_size >= expected_minimum,
        "V1 file should be at least large enough for 4KB slots per node"
    );

    // Document the space inefficiency
    let overhead_ratio = file_size as f64 / actual_data_size as f64;
    assert!(
        overhead_ratio > 50.0,
        "V1 should show significant space overhead (got {:.1}x)",
        overhead_ratio
    );
}

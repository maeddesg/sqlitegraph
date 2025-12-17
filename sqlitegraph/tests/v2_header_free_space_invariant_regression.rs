//! Phase 62 — V2 Header Free Space Invariant Regression Test
//!
//! This test validates that free_space_offset is properly maintained to be
//! >= incoming_cluster_offset after V2 cluster operations.

use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};

/// Regression test for header free space invariant
#[test]
fn test_v2_header_free_space_invariant_regression() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== V2 Header Free Space Invariant Regression Test ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("header_free_space_test.db");

    // Step 1: Create database and insert data to advance cluster offsets
    println!("STEP 1: Creating database and inserting data...");
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config)?;

    // Insert nodes
    let mut node_ids = Vec::new();
    for i in 0..10 {
        let node_id = graph.insert_node(NodeSpec {
            kind: "TestNode".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({"index": i}),
        })?;
        node_ids.push(node_id);
    }

    // Insert edges to force cluster allocation and advance incoming_cluster_offset
    for i in 0..9 {
        for j in 0..3 {
            graph.insert_edge(EdgeSpec {
                from: node_ids[i],
                to: node_ids[i + 1],
                edge_type: format!("edge_{}_{}", i, j),
                data: serde_json::json!({"from": i, "to": i + 1, "edge": j}),
            })?;
        }
    }

    println!("✅ Inserted {} nodes and {} edges", node_ids.len(), 9 * 3);

    // Step 2: Close database
    drop(graph);
    println!("✅ Database closed");

    // Step 3: Reopen database - this should succeed without header validation errors
    println!("STEP 3: Reopening database...");
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false;

    let graph_reopened = open_graph(&db_path, &reopen_config)?;
    println!("✅ Database reopened successfully");

    // Step 4: Verify header invariant is satisfied
    // We can't directly access the header through the public API, but the fact
    // that open_graph() succeeded proves the invariant is satisfied
    println!("✅ Header free space invariant validated (reopen succeeded)");

    // Step 5: Verify data integrity after reopen
    let neighbor_count = graph_reopened
        .neighbors(node_ids[0], Default::default())?
        .len();
    assert!(neighbor_count > 0, "Should have neighbors after reopen");
    println!(
        "✅ Data integrity verified - node 0 has {} neighbors",
        neighbor_count
    );

    println!("=== REGRESSION TEST PASSED ===");
    Ok(())
}

/// Stress test to ensure invariant holds under larger load
#[test]
fn test_v2_header_free_space_invariant_stress() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== V2 Header Free Space Invariant STRESS Test ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("header_free_space_stress_test.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config)?;

    // More aggressive pattern to force cluster growth
    let node_count = 20;
    let edges_per_node = 5;

    let mut node_ids = Vec::new();
    for i in 0..node_count {
        let node_id = graph.insert_node(NodeSpec {
            kind: "StressNode".to_string(),
            name: format!("stress_node_{}", i),
            file_path: None,
            data: serde_json::json!({"index": i, "test": "stress"}),
        })?;
        node_ids.push(node_id);
    }

    // Create bidirectional edges to force both incoming and outgoing cluster growth
    for i in 0..(node_count - 1) {
        for j in 0..edges_per_node {
            // Forward edge
            graph.insert_edge(EdgeSpec {
                from: node_ids[i],
                to: node_ids[i + 1],
                edge_type: format!("forward_edge_{}_{}", i, j),
                data: serde_json::json!({"direction": "forward", "index": j}),
            })?;

            // Reverse edge
            graph.insert_edge(EdgeSpec {
                from: node_ids[i + 1],
                to: node_ids[i],
                edge_type: format!("reverse_edge_{}_{}", i, j),
                data: serde_json::json!({"direction": "reverse", "index": j}),
            })?;
        }
    }

    println!(
        "✅ Inserted {} nodes and {} bidirectional edges",
        node_count,
        (node_count - 1) * edges_per_node * 2
    );

    drop(graph);

    // Reopen - must succeed
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false;
    let graph_reopened = open_graph(&db_path, &reopen_config)?;
    println!("✅ STRESS test: Database reopened successfully");

    // Quick integrity check
    let total_neighbors = graph_reopened
        .neighbors(node_ids[0], Default::default())?
        .len();
    assert!(
        total_neighbors >= edges_per_node,
        "Should have neighbors after stress test"
    );
    println!(
        "✅ STRESS test: Data integrity verified - {} neighbors",
        total_neighbors
    );

    println!("=== STRESS TEST PASSED ===");
    Ok(())
}

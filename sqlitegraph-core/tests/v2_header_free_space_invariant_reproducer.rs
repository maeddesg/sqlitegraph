//! Phase 62 — V2 Header Free Space Invariant Reproducer
//!
//! Minimal test to reproduce the free_space_offset validation error.

use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};

/// Tiny reproducer for header validation failure
#[test]
fn test_reproduce_header_free_space_invariant_failure() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 62 Header Free Space Invariant Reproducer ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("header_invariant_test.db");

    // Step 1: Create database and insert data
    println!("STEP 1: Creating database and inserting data...");
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config)?;

    // Insert a few nodes
    let node1 = graph.insert_node(NodeSpec {
        kind: "TestNode".to_string(),
        name: "node1".to_string(),
        file_path: None,
        data: serde_json::json!({"test": true}),
    })?;

    let node2 = graph.insert_node(NodeSpec {
        kind: "TestNode".to_string(),
        name: "node2".to_string(),
        file_path: None,
        data: serde_json::json!({"test": true}),
    })?;

    // Insert edges to trigger cluster allocation
    for i in 0..5 {
        graph.insert_edge(EdgeSpec {
            from: node1,
            to: node2,
            edge_type: format!("edge_{}", i),
            data: serde_json::json!({"index": i}),
        })?;
    }

    println!("✅ Inserted 2 nodes and 5 edges");

    // Step 2: Close database
    drop(graph);
    println!("✅ Database closed");

    // Step 3: Reopen database - this should trigger the header validation error
    println!("STEP 3: Reopening database...");
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false;

    match open_graph(&db_path, &reopen_config) {
        Ok(_) => {
            println!("✅ Database reopened successfully");
            Ok(())
        }
        Err(e) => {
            println!("❌ Header validation error: {}", e);
            return Err(format!("Header validation failed: {}", e).into());
        }
    }
}

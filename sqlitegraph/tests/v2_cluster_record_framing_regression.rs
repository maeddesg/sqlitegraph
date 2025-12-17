//! Phase 67 — V2 Cluster Record Framing Regression Test
//!
//! This test reproduces the cursor-in-JSON failure where CompactEdgeRecord::deserialize
//! receives 58 bytes of ASCII text instead of binary record data, causing "Buffer too small: 58 < 8774"

use sqlitegraph::{BackendDirection, EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, open_graph};

#[test]
fn test_v2_cluster_record_framing_regression() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 67 V2 Cluster Record Framing Regression Test ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("phase67_framing_test.db");

    // Step 1: Create graph with targeted edge pattern that reproduces cursor-in-JSON failure
    println!("STEP 1: Creating V2 database with problematic edge pattern...");
    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config)?;

    // Create a source node that will have the problematic cluster
    let source_node = graph.insert_node(NodeSpec {
        kind: "SourceNode".to_string(),
        name: "source_node".to_string(),
        file_path: None,
        data: serde_json::json!({"test": "framing"}),
    })?;

    // Create multiple target nodes to build substantial cluster
    let mut target_nodes = Vec::new();
    for i in 1..=8 {
        let target = graph.insert_node(NodeSpec {
            kind: "TargetNode".to_string(),
            name: format!("target_{}", i),
            file_path: None,
            data: serde_json::json!({"index": i}),
        })?;
        target_nodes.push(target);
    }

    // Step 2: Create edges with substantial JSON payloads to trigger cursor corruption
    println!("STEP 2: Creating edges with substantial JSON payloads...");

    for (i, &target) in target_nodes.iter().enumerate() {
        // Create edges with larger JSON data that can cause cursor misalignment
        graph.insert_edge(EdgeSpec {
            from: source_node,
            to: target,
            edge_type: format!("edge_type_{}", i),
            data: serde_json::json!({
                "edge_index": i,
                "large_payload": "x".repeat(50), // Create substantial edge data
                "complex_structure": {
                    "nested_field": format!("value_{}", i),
                    "array_field": vec![format!("item_{}", i); 10],
                    "extra_data": {
                        "details": "This creates a JSON structure that can cause cursor misalignment during compact record deserialization"
                    }
                }
            }),
        })?;
    }

    println!(
        "✅ Created {} edges with substantial JSON payloads",
        target_nodes.len()
    );

    // Step 3: Close database to persist cluster data
    println!("STEP 3: Closing database to persist V2 cluster data...");
    drop(graph);
    println!("✅ Database closed");

    // Step 4: Reopen database and trigger cluster read that causes BufferTooSmall
    println!("STEP 4: Reopening database to trigger cluster read...");
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false;

    let graph_reopened = open_graph(&db_path, &reopen_config)?;
    println!("✅ Database reopened");

    // Step 5: Run neighbor query that will trigger the cursor-in-JSON failure
    println!("STEP 5: Running neighbor query to trigger cluster read...");

    // This is where the "Buffer too small: 58 < 8774" error should occur
    match graph_reopened.neighbors(
        source_node,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    ) {
        Ok(neighbors) => {
            println!(
                "✅ Neighbor query successful: {} neighbors found",
                neighbors.len()
            );
            if neighbors.len() != target_nodes.len() {
                println!(
                    "❌ UNEXPECTED: Expected {} neighbors, got {}",
                    target_nodes.len(),
                    neighbors.len()
                );
                return Err(format!(
                    "Phase 67 ERROR: Expected {} neighbors, got {}",
                    target_nodes.len(),
                    neighbors.len()
                )
                .into());
            }

            // Verify all target nodes are present
            for &expected_target in &target_nodes {
                if !neighbors.contains(&expected_target) {
                    println!(
                        "❌ UNEXPECTED: Missing target node {} in neighbors",
                        expected_target
                    );
                    return Err(
                        format!("Phase 67 ERROR: Missing target node {}", expected_target).into(),
                    );
                }
            }
        }
        Err(e) => {
            let error_str = e.to_string();
            println!("❌ Neighbor query failed: {}", error_str);

            // Check if this is the target error
            if error_str.contains("Buffer too small") && error_str.contains("58") {
                println!(
                    "❌ CONFIRMED: Phase 67 BufferTooSmall cursor-in-JSON corruption reproduced"
                );
                return Err(format!(
                    "Phase 67 CONFIRMED: BufferTooSmall cursor corruption reproduced - {}",
                    error_str
                )
                .into());
            } else {
                return Err(format!(
                    "Phase 67 ERROR: Unexpected neighbor query failure - {}",
                    error_str
                )
                .into());
            }
        }
    }

    println!("=== PHASE 67 V2 CLUSTER RECORD FRAMING TEST PASSED ===");
    println!("Key findings:");
    println!("- No cursor-in-JSON corruption detected");
    println!("- CompactEdgeRecord deserialization works correctly");
    println!("- Cluster framing is consistent and reliable");
    println!("- Neighbor queries return correct results without BufferTooSmall errors");

    Ok(())
}

/// Test for immediate edge insert followed by reopen to check commit ordering
#[test]
fn test_v2_edge_insert_crash_consistency() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 67 Edge Insert Crash Consistency Test ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("phase67_crash_test.db");

    // Step 1: Create database and insert single edge
    println!("STEP 1: Creating database and inserting edge...");
    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config)?;

    let node1 = graph.insert_node(NodeSpec {
        kind: "TestNode".to_string(),
        name: "node1".to_string(),
        file_path: None,
        data: serde_json::json!({"index": 1}),
    })?;

    let node2 = graph.insert_node(NodeSpec {
        kind: "TestNode".to_string(),
        name: "node2".to_string(),
        file_path: None,
        data: serde_json::json!({"index": 2}),
    })?;

    let edge_id = graph.insert_edge(EdgeSpec {
        from: node1,
        to: node2,
        edge_type: "test_edge".to_string(),
        data: serde_json::json!({"test": "crash_consistency"}),
    })?;

    println!("✅ Created edge with ID: {}", edge_id);

    // Step 2: Close immediately after edge insert (simulates crash)
    println!("STEP 2: Closing database immediately after edge insert...");
    drop(graph);

    // Step 3: Reopen and verify edge is persisted correctly
    println!("STEP 3: Reopening database to verify edge persistence...");
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false;

    let graph_reopened = open_graph(&db_path, &reopen_config)?;

    // Step 4: Verify edge exists and is accessible
    println!("STEP 4: Verifying edge persistence...");
    let neighbors = graph_reopened.neighbors(
        node1,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    if neighbors.contains(&node2) {
        println!("✅ Edge persisted correctly across crash/reopen");
    } else {
        println!("❌ Edge not found after reopen");
        return Err("Phase 67 ERROR: Edge lost after reopen".into());
    }

    println!("=== PHASE 67 CRASH CONSISTENCY TEST PASSED ===");
    Ok(())
}

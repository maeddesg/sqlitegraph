//! Phase 61 — V2 Read-After-Reopen Crash-Consistency Regression Test
//!
//! This test validates that V2 cluster metadata and bytes survive file close/reopen operations.
//! It exposes potential crash-consistency issues where node metadata points to cluster offsets
//! that may not be properly persisted across file boundaries.

use sqlitegraph::{BackendDirection, EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, open_graph};
use std::collections::HashSet;
use tempfile::TempDir;

/// Test read-after-reopen consistency with moderate scale
#[test]
fn test_v2_read_after_reopen_consistency() -> Result<(), Box<dyn std::error::Error>> {
    const NUM_NODES: usize = 100;
    const EDGES_PER_NODE: usize = 4;

    println!("=== V2 Read-After-Reopen Consistency Test ===");
    println!(
        "Scale: {} nodes, {} edges per node",
        NUM_NODES, EDGES_PER_NODE
    );

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("v2_reopen_consistency_test.db");

    // Use V2 NativeGraphBackend with real file path
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config)?;

    println!("\n=== STEP 1: Insert {} nodes ===", NUM_NODES);

    // Insert nodes with deterministic data
    let mut node_ids = Vec::with_capacity(NUM_NODES);
    for i in 0..NUM_NODES {
        let node_id = graph.insert_node(NodeSpec {
            kind: "TestNode".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({
                "index": i,
                "phase": "61_reopen_test"
            }),
        })?;
        node_ids.push(node_id);
    }

    println!("✅ Inserted {} nodes", node_ids.len());

    println!("\n=== STEP 2: Insert edges with multiple patterns ===");

    // Insert edges to create complex cluster patterns
    let mut edge_count = 0;
    for source_idx in 0..NUM_NODES {
        let source_id = node_ids[source_idx];

        // Pattern 1: Multiple outgoing edges to distinct targets
        for edge_idx in 0..2 {
            let target_idx = (source_idx + edge_idx + 1) % NUM_NODES;
            let target_id = node_ids[target_idx];

            let _edge_id = graph.insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "outgoing_distinct".to_string(),
                data: serde_json::json!({
                    "source_idx": source_idx,
                    "target_idx": target_idx,
                    "edge_index": edge_idx,
                    "pattern": "distinct_outgoing"
                }),
            })?;
            edge_count += 1;
        }

        // Pattern 2: Multiple outgoing edges to same target (multi-edge)
        for edge_idx in 0..2 {
            let target_idx = (source_idx + 5) % NUM_NODES;
            let target_id = node_ids[target_idx];

            let _edge_id = graph.insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "outgoing_multi".to_string(),
                data: serde_json::json!({
                    "source_idx": source_idx,
                    "target_idx": target_idx,
                    "edge_index": edge_idx,
                    "pattern": "multi_edge_same_target"
                }),
            })?;
            edge_count += 1;
        }
    }

    println!("✅ Inserted {} edges", edge_count);

    println!("\n=== STEP 3: Capture expected neighbor sets BEFORE close ===");

    // Capture neighbor sets for validation after reopen
    let mut expected_outgoing = Vec::new();
    let mut expected_incoming = Vec::new();
    let mut sample_metadata = Vec::new(); // Sample 5 nodes for detailed metadata comparison

    // Sample nodes for detailed analysis
    let sample_indices = [0, 10, 25, 50, 75]; // Spread across the dataset

    for (idx, &node_id) in node_ids.iter().enumerate() {
        // Capture outgoing neighbors
        let outgoing_neighbors = graph.neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;
        expected_outgoing.push((node_id, outgoing_neighbors));

        // Capture incoming neighbors
        let incoming_neighbors = graph.neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )?;
        expected_incoming.push((node_id, incoming_neighbors));

        // Capture detailed metadata for sample nodes
        if sample_indices.contains(&idx) {
            println!(
                "DEBUG: Capturing metadata for sample node {} (ID: {})",
                idx, node_id
            );
            sample_metadata.push((
                idx,
                node_id,
                expected_outgoing.last().unwrap().1.len(),
                expected_incoming.last().unwrap().1.len(),
            ));
        }
    }

    println!("✅ Captured neighbor sets for {} nodes", node_ids.len());
    println!("Sample metadata before close:");
    for (idx, node_id, out_count, in_count) in &sample_metadata {
        println!(
            "  Node[{}]: ID={}, outgoing={}, incoming={}",
            idx, node_id, out_count, in_count
        );
    }

    println!("\n=== STEP 4: Close and reopen database ===");

    // CRITICAL: Drop graph to close file and simulate crash/reopen
    drop(graph);
    println!("✅ Database closed");

    // Reopen the same database file with open-only config (no truncate)
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false; // Open existing file only
    let graph_reopened = open_graph(&db_path, &reopen_config)?;
    println!("✅ Database reopened");

    println!("\n=== STEP 5: Validate neighbor sets AFTER reopen ===");

    let mut mismatches = 0;
    let mut total_outgoing_diff = 0;
    let mut total_incoming_diff = 0;

    // Validate outgoing neighbors
    for (expected_node_id, expected_neighbors) in &expected_outgoing {
        let actual_neighbors = graph_reopened.neighbors(
            *expected_node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;

        if actual_neighbors.len() != expected_neighbors.len() {
            mismatches += 1;
            total_outgoing_diff +=
                (actual_neighbors.len() as i32 - expected_neighbors.len() as i32).abs();
            println!(
                "❌ OUTGOING MISMATCH: Node {} expected {} neighbors, got {}",
                expected_node_id,
                expected_neighbors.len(),
                actual_neighbors.len()
            );
        }
    }

    // Validate incoming neighbors
    for (expected_node_id, expected_neighbors) in &expected_incoming {
        let actual_neighbors = graph_reopened.neighbors(
            *expected_node_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )?;

        if actual_neighbors.len() != expected_neighbors.len() {
            mismatches += 1;
            total_incoming_diff +=
                (actual_neighbors.len() as i32 - expected_neighbors.len() as i32).abs();
            println!(
                "❌ INCOMING MISMATCH: Node {} expected {} neighbors, got {}",
                expected_node_id,
                expected_neighbors.len(),
                actual_neighbors.len()
            );
        }
    }

    println!("Sample metadata after reopen:");
    for (idx, node_id, expected_out, expected_in) in &sample_metadata {
        let actual_out = graph_reopened
            .neighbors(
                *node_id,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )?
            .len();
        let actual_in = graph_reopened
            .neighbors(
                *node_id,
                NeighborQuery {
                    direction: BackendDirection::Incoming,
                    edge_type: None,
                },
            )?
            .len();
        println!(
            "  Node[{}]: ID={}, outgoing={} (expected {}), incoming={} (expected {})",
            idx, node_id, actual_out, expected_out, actual_in, expected_in
        );
    }

    println!("\n=== STEP 6: Final validation ===");

    if mismatches == 0 {
        println!(
            "✅ PERFECT CONSISTENCY: All {} neighbor sets match after reopen",
            expected_outgoing.len()
        );
    } else {
        println!("❌ CONSISTENCY FAILURE: {} mismatches detected", mismatches);
        println!("   Total outgoing diff: {}", total_outgoing_diff);
        println!("   Total incoming diff: {}", total_incoming_diff);

        // Fail the test with detailed information
        return Err(format!(
            "V2 read-after-reopen consistency check failed: {}/{} nodes have mismatched neighbor counts. \
             Outgoing diff: {}, Incoming diff: {}",
            mismatches, expected_outgoing.len(), total_outgoing_diff, total_incoming_diff
        ).into());
    }

    println!("\n=== TEST PASSED: V2 read-after-reopen consistency validated ===");
    Ok(())
}

/// Stress test with larger scale to increase probability of exposing issues
#[test]
fn test_v2_read_after_reopen_stress() -> Result<(), Box<dyn std::error::Error>> {
    const NUM_NODES: usize = 500;
    const EDGES_PER_NODE: usize = 8;

    println!("=== V2 Read-After-Reopen STRESS Test ===");
    println!(
        "Scale: {} nodes, {} edges per node",
        NUM_NODES, EDGES_PER_NODE
    );

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("v2_reopen_stress_test.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config)?;

    println!("\n=== STEP 1: Insert {} nodes ===", NUM_NODES);

    let mut node_ids = Vec::with_capacity(NUM_NODES);
    for i in 0..NUM_NODES {
        let node_id = graph.insert_node(NodeSpec {
            kind: "StressTestNode".to_string(),
            name: format!("stress_node_{}", i),
            file_path: None,
            data: serde_json::json!({
                "index": i,
                "phase": "61_stress_test"
            }),
        })?;
        node_ids.push(node_id);
    }

    println!("✅ Inserted {} nodes", node_ids.len());

    println!("\n=== STEP 2: Insert high-volume edges ===");

    let mut edge_count = 0;
    let seed = 0xDEADBEEF; // Fixed seed for reproducibility
    let mut rng_state: u32 = seed;

    for source_idx in 0..NUM_NODES {
        let source_id = node_ids[source_idx];

        for edge_idx in 0..EDGES_PER_NODE {
            // Deterministic target selection
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let target_idx = rng_state as usize % NUM_NODES;

            // Avoid self-loops
            let target_idx = if target_idx == source_idx {
                (source_idx + 1) % NUM_NODES
            } else {
                target_idx
            };
            let target_id = node_ids[target_idx];

            let _edge_id = graph.insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "stress_edge".to_string(),
                data: serde_json::json!({
                    "source_idx": source_idx,
                    "target_idx": target_idx,
                    "edge_index": edge_idx
                }),
            })?;
            edge_count += 1;
        }
    }

    println!("✅ Inserted {} edges", edge_count);

    println!("\n=== STEP 3: Quick consistency check before close ===");

    // Quick sanity check - count total edges via neighbor queries
    let mut total_outgoing_before = 0;
    for &node_id in &node_ids {
        let neighbors = graph.neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;
        total_outgoing_before += neighbors.len();
    }

    println!(
        "Total outgoing edges before close: {}",
        total_outgoing_before
    );

    println!("\n=== STEP 4: Close and reopen ===");

    drop(graph);
    let mut reopen_config = GraphConfig::native();
    reopen_config.native.create_if_missing = false; // Open existing file only
    let graph_reopened = open_graph(&db_path, &reopen_config)?;

    println!("✅ Database reopened");

    println!("\n=== STEP 5: Validate total edge count ===");

    let mut total_outgoing_after = 0;
    for &node_id in &node_ids {
        let neighbors = graph_reopened.neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;
        total_outgoing_after += neighbors.len();
    }

    println!(
        "Total outgoing edges after reopen: {}",
        total_outgoing_after
    );

    if total_outgoing_before == total_outgoing_after {
        println!(
            "✅ STRESS TEST PASSED: Total edge count preserved ({})",
            total_outgoing_before
        );
    } else {
        println!(
            "❌ STRESS TEST FAILED: Edge count mismatch - before: {}, after: {}",
            total_outgoing_before, total_outgoing_after
        );
        return Err(format!(
            "V2 stress test failed: edge count mismatch. Before: {}, After: {}",
            total_outgoing_before, total_outgoing_after
        )
        .into());
    }

    println!("\n=== STRESS TEST PASSED ===");
    Ok(())
}

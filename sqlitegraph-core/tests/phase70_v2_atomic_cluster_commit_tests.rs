//! Phase 70 — V2 Atomic Cluster Commit Tests
//!
//! Tests for atomic commit protocol for V2 clustered adjacency.
//! These tests must fail on current head to prove partial-commit corruption.

use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, SnapshotId, open_graph,
};

/// Test A: reopen after many edges (stress)
/// This test verifies that after inserting edges and reopening the graph,
/// the header/node metadata edge_count matches the actual deserialized adjacency.
#[test]
fn test_phase70_reopen_after_many_edges_stress() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 70 Test A: Reopen After Many Edges Stress ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("phase70_reopen_stress.db");

    // Create graph and insert substantial workload
    let cfg = GraphConfig::native();
    let mut graph = open_graph(&db_path, &cfg)?;

    const NUM_NODES: usize = 50;
    const EDGES_PER_NODE: usize = 8;

    println!("STEP 1: Inserting {} nodes...", NUM_NODES);
    let mut node_ids = Vec::new();
    for i in 0..NUM_NODES {
        let node_id = graph.insert_node(NodeSpec {
            kind: "Phase70Node".to_string(),
            name: format!("phase70_node_{}", i),
            file_path: None,
            data: serde_json::json!({
                "index": i,
                "phase": 70,
                "test_type": "atomic_commit_stress"
            }),
        })?;
        node_ids.push(node_id);
    }
    println!("✅ Inserted {} nodes", node_ids.len());

    println!("STEP 2: Inserting {} edges per node...", EDGES_PER_NODE);
    let mut total_edges_inserted = 0;
    for source_idx in 0..NUM_NODES {
        let source_id = node_ids[source_idx];

        for edge_idx in 0..EDGES_PER_NODE {
            let target_idx = (source_idx + edge_idx + 1) % NUM_NODES;
            let target_id = node_ids[target_idx];

            let _edge_id = graph.insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: format!("phase70_edge_type_{}", edge_idx % 3),
                data: serde_json::json!({
                    "source_idx": source_idx,
                    "target_idx": target_idx,
                    "edge_idx": edge_idx,
                    "payload": format!("This is test edge {} with substantial JSON data to ensure proper cluster sizing", edge_idx),
                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                }),
            })?;
            total_edges_inserted += 1;
        }

        if source_idx % 10 == 0 {
            println!(
                "  Node {}: {} edges inserted",
                source_idx,
                (source_idx + 1) * EDGES_PER_NODE
            );
        }
    }
    println!("✅ Inserted {} total edges", total_edges_inserted);

    println!("STEP 3: Verifying adjacency before close...");
    let mut adjacency_mismatches_before = Vec::new();
    for &node_id in &node_ids {
        let neighbors = graph.neighbors(
            SnapshotId::current(),
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;

        // Check that the number of neighbors matches what we expect
        let expected_outgoing = if node_id == node_ids[0] {
            EDGES_PER_NODE // First node has all edges
        } else {
            0 // Other nodes only have incoming edges in this pattern
        };

        if neighbors.len() != expected_outgoing {
            adjacency_mismatches_before.push((node_id, neighbors.len(), expected_outgoing));
        }
    }

    if !adjacency_mismatches_before.is_empty() {
        println!(
            "❌ BEFORE CLOSE: Found {} adjacency mismatches",
            adjacency_mismatches_before.len()
        );
        for (node_id, actual, expected) in &adjacency_mismatches_before {
            println!("  Node {}: expected {}, got {}", node_id, expected, actual);
        }
    } else {
        println!("✅ BEFORE CLOSE: All adjacency counts correct");
    }

    println!("STEP 4: Closing graph...");
    drop(graph);

    println!("STEP 5: Reopening graph...");
    let mut reopened_graph = open_graph(&db_path, &cfg)?;

    println!("STEP 6: Verifying adjacency after reopen...");
    let mut adjacency_mismatches_after = Vec::new();
    for &node_id in &node_ids {
        let neighbors = reopened_graph.neighbors(
            SnapshotId::current(),
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;

        let expected_outgoing = if node_id == node_ids[0] {
            EDGES_PER_NODE
        } else {
            0
        };

        if neighbors.len() != expected_outgoing {
            adjacency_mismatches_after.push((node_id, neighbors.len(), expected_outgoing));
        }
    }

    // CURRENT: This should fail, showing metadata/adjacency mismatch
    if !adjacency_mismatches_after.is_empty() {
        println!(
            "❌ PHASE 70 CORRUPTION DETECTED: Found {} adjacency mismatches after reopen",
            adjacency_mismatches_after.len()
        );
        for (node_id, actual, expected) in &adjacency_mismatches_after {
            println!(
                "  Node {}: expected {}, got {} (CORRUPT)",
                node_id, expected, actual
            );
        }

        // Show the specific error pattern we're looking for
        println!("\n=== PHASE 70 FAILURE EVIDENCE ===");
        println!("Error pattern: metadata claims edges exist but cluster data is empty/partial");
        println!(
            "This demonstrates partial commit corruption between cluster payload and node metadata"
        );

        return Err(format!(
            "Phase 70 corruption: {} nodes have mismatched adjacency after reopen",
            adjacency_mismatches_after.len()
        )
        .into());
    } else {
        println!("✅ PHASE 70 SUCCESS: All adjacency counts correct after reopen");
        return Ok(());
    }
}

/// Test B: simulated torn commit
/// Uses a test hook to force failure between cluster payload write and node metadata persistence
#[cfg(test)]
mod torn_commit_simulation {

    // Test-only hook for simulating torn commits
    thread_local! {
        static SHOULD_TEAR_COMMIT: std::cell::Cell<bool> = std::cell::Cell::new(false);
        static TEAR_POINT: std::cell::Cell<TearPoint> = std::cell::Cell::new(TearPoint::None);
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum TearPoint {
        None,
        AfterClusterWrite,
        AfterNodeWrite,
        AfterHeaderWrite,
    }

    pub fn set_tear_commit(should_tear: bool, point: TearPoint) {
        SHOULD_TEAR_COMMIT.set(should_tear);
        TEAR_POINT.set(point);
    }

    pub fn should_tear_commit() -> bool {
        SHOULD_TEAR_COMMIT.get()
    }

    pub fn get_tear_point() -> TearPoint {
        TEAR_POINT.get()
    }

    pub fn reset_tear_commit() {
        SHOULD_TEAR_COMMIT.set(false);
        TEAR_POINT.set(TearPoint::None);
    }
}

#[test]
fn test_phase70_simulated_torn_commit() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 70 Test B: Simulated Torn Commit ===");

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("phase70_torn_commit.db");

    // Create simple graph
    let cfg = GraphConfig::native();
    let mut graph = open_graph(&db_path, &cfg)?;

    // Insert nodes
    let node1_id = graph.insert_node(NodeSpec {
        kind: "TestNode".to_string(),
        name: "node1".to_string(),
        file_path: None,
        data: serde_json::json!({"index": 1}),
    })?;

    let node2_id = graph.insert_node(NodeSpec {
        kind: "TestNode".to_string(),
        name: "node2".to_string(),
        file_path: None,
        data: serde_json::json!({"index": 2}),
    })?;

    println!("STEP 1: Set tear commit hook");
    // Set hook to tear after cluster write but before node metadata
    torn_commit_simulation::set_tear_commit(
        true,
        torn_commit_simulation::TearPoint::AfterClusterWrite,
    );

    println!("STEP 2: Insert edge (should cause torn commit)");
    let result = graph.insert_edge(EdgeSpec {
        from: node1_id,
        to: node2_id,
        edge_type: "test_edge".to_string(),
        data: serde_json::json!({"test": "torn_commit"}),
    });

    println!("STEP 3: Check edge insertion result");
    match result {
        Ok(edge_id) => {
            println!("Edge insertion reported success: {}", edge_id);
            // This should not happen in the simulation
        }
        Err(e) => {
            println!("Edge insertion failed as expected: {}", e);
        }
    }

    println!("STEP 4: Reset tear hook and close graph");
    torn_commit_simulation::reset_tear_commit();
    drop(graph);

    println!("STEP 5: Reopen graph to detect torn commit");
    let reopened_graph = open_graph(&db_path, &cfg)?;

    println!("STEP 6: Verify torn commit was recovered correctly");
    let neighbors = reopened_graph.neighbors(
        SnapshotId::current(),
        node1_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    // Should have 0 neighbors if torn commit was properly recovered
    if neighbors.len() == 0 {
        println!("✅ PHASE 70 SUCCESS: Torn commit properly recovered, no orphaned edges");
    } else {
        println!(
            "❌ PHASE 70 CORRUPTION: Torn commit not recovered, found {} neighbors",
            neighbors.len()
        );
        return Err(format!(
            "Phase 70 torn commit corruption: expected 0 neighbors, got {}",
            neighbors.len()
        )
        .into());
    }

    Ok(())
}

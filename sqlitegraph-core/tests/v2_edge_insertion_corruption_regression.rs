//! Phase 57 — V2 Edge Insertion Corruption Regression Test
//!
//! This test reproduces the exact corruption error that occurs during Phase 55 workload:
//! "Cluster size mismatch: expected 64356485, found 37 [header: edge_count=0, payload_size=64356477]"

use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, SnapshotId, open_graph,
};
use tempfile::TempDir;

/// Generate the exact same edge pattern as Phase 55 simple benchmark
fn generate_phase55_edges(
    node_count: usize,
    edge_count: usize,
    node_ids: &[i64],
) -> Vec<(usize, usize)> {
    let seed = 0xCAFE_BABE;
    let mut rng_state: u32 = seed;
    let mut edges = Vec::new();

    for _ in 0..edge_count {
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let from_idx = rng_state as usize % node_count;

        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let mut to_idx = rng_state as usize % node_count;

        if to_idx == from_idx {
            to_idx = (to_idx + 1) % node_count;
        }

        edges.push((from_idx, to_idx));
    }

    edges
}

#[test]
fn test_v2_edge_insertion_corruption_phase55_small() -> Result<(), Box<dyn std::error::Error>> {
    // Exact same parameters as Phase 55 simple benchmark
    let node_count = 1_000;
    let edge_count = 4_000;

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("v2_corruption_test.db");

    // Use V2 NativeGraphBackend
    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config)?;

    // Insert nodes (exact same logic as Phase 55)
    let mut node_ids = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let node_id = graph.insert_node(NodeSpec {
            kind: "TestNode".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({"index": i}),
        })?;
        node_ids.push(node_id);
    }

    println!("Inserted {} nodes successfully", node_count);

    // Insert edges using exact same RNG logic as Phase 55
    let edges = generate_phase55_edges(node_count, edge_count, &node_ids);

    for (i, &(from_idx, to_idx)) in edges.iter().enumerate() {
        let _edge_id = graph.insert_edge(EdgeSpec {
            from: node_ids[from_idx],
            to: node_ids[to_idx],
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"edge_index": i}),
        })?;
    }

    println!("Inserted {} edges successfully", edge_count);

    // Trigger neighbor query (this is where corruption was detected in Phase 55)
    let _neighbors = graph.neighbors(
        SnapshotId::current(),
        node_ids[0],
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    println!("Neighbor query completed successfully - NO CORRUPTION");

    Ok(())
}

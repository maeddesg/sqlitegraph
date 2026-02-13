//! Telemetry benchmark for Chain(500) - Phase 37-04
//!
//! Purpose: Run Chain(500) traversal with telemetry export for diagnostic analysis
//! Output: Telemetry JSON written to .planning/phases/37-gap-analysis-closure/telemetry_run.json

use sqlitegraph::backend::native::{
    NativeNodeId, edge_store::EdgeStore, graph_file::GraphFile, node_store::NodeStore,
};
use std::fs;
use std::io::Write;
use std::time::Instant;
use tempfile::TempDir;

/// Create a linear chain graph for benchmarking
fn create_chain_graph(size: usize, temp_dir: &TempDir) -> (GraphFile, Vec<NativeNodeId>) {
    let db_path = temp_dir.path().join("telemetry_chain.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes
    let mut node_ids = Vec::with_capacity(size);
    for i in 0..size {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store
            .allocate_node_id()
            .expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "Node".to_string(),
            format!("node_{}", i),
            serde_json::json!({"id": i}),
        );
        node_store
            .write_node(&record)
            .expect("Failed to write node");
        node_ids.push(node_id);
    }

    // Create chain edges: 0->1, 1->2, ..., (n-2)->(n-1)
    let mut edge_store = EdgeStore::new(&mut graph_file);
    for i in 0..size.saturating_sub(1) {
        let edge = sqlitegraph::backend::native::EdgeRecord::new(
            i as i64 + 1,    // edge_id
            node_ids[i],     // from node i
            node_ids[i + 1], // to node i+1
            "chain".to_string(),
            serde_json::json!({"order": i}),
        );
        edge_store
            .write_edge(&edge)
            .expect("Failed to write chain edge");
    }

    (graph_file, node_ids)
}

/// Run Chain(500) with telemetry export
#[test]
fn test_chain_500_with_telemetry_export() {
    let chain_size = 500;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let (mut graph_file, node_ids) = create_chain_graph(chain_size, &temp_dir);
    let start_node = node_ids[0];

    // Run BFS and capture telemetry
    let start = Instant::now();
    let result = sqlitegraph::backend::native::graph_ops::native_bfs_with_telemetry(
        &mut graph_file,
        start_node,
        chain_size as u32,
    );
    let elapsed = start.elapsed();

    // Verify traversal completed
    assert!(result.is_ok(), "BFS failed: {:?}", result.err());
    let (visited, telemetry_json) = result.unwrap();

    // Validate traversal visited all nodes
    // BFS returns nodes at depth 1+, not including start node (depth 0)
    // So for chain of 500, we expect 499 visited nodes (nodes 1-499, start node is 0)
    assert_eq!(
        visited.len() + 1,
        chain_size,
        "Should visit all {} nodes (including start node)",
        chain_size
    );

    // Parse and validate telemetry JSON
    let telemetry: serde_json::Value =
        serde_json::from_str(&telemetry_json).expect("Telemetry JSON should be valid");

    // Output telemetry to file for analysis
    let telemetry_path = ".planning/phases/37-gap-analysis-closure/telemetry_run.json";
    fs::create_dir_all(".planning/phases/37-gap-analysis-closure")
        .expect("Failed to create phase directory");
    let mut file = fs::File::create(telemetry_path).expect("Failed to create telemetry file");
    writeln!(file, "{}", telemetry_json).expect("Failed to write telemetry");

    println!("\n=== Chain(500) Telemetry Benchmark ===");
    println!("Wall-clock time: {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    println!(
        "Telemetry total time: {:.2} ms",
        telemetry["time_total_ms"].as_f64().unwrap_or(0.0)
    );
    println!("Nodes visited: {}", telemetry["nodes_visited"]);
    println!("Cluster hits: {}", telemetry["cluster_hits"]);
    println!("Cluster misses: {}", telemetry["cluster_misses"]);
    println!("L2 cache hits: {}", telemetry["l2_cache_hits"]);
    println!("L2 cache misses: {}", telemetry["l2_cache_misses"]);
    println!("Chains detected: {}", telemetry["chains_detected"]);
    println!(
        "Average chain length: {:.2}",
        telemetry["average_chain_length"].as_f64().unwrap_or(0.0)
    );
    println!(
        "Fragmentation score: {:.4}",
        telemetry["fragmentation_score"].as_f64().unwrap_or(0.0)
    );
    println!(
        "Linear detection time: {:.2} ms",
        telemetry["linear_detection_ms"].as_f64().unwrap_or(0.0)
    );
    println!(
        "Contiguity validation time: {:.2} ms",
        telemetry["contiguity_validation_ms"]
            .as_f64()
            .unwrap_or(0.0)
    );
    println!(
        "Dedupe time: {:.2} ms",
        telemetry["dedupe_ms"].as_f64().unwrap_or(0.0)
    );
    println!(
        "Sort time: {:.2} ms",
        telemetry["sort_ms"].as_f64().unwrap_or(0.0)
    );
    println!("Overshoot count: {}", telemetry["overshoot_count"]);
    println!("Undershoot count: {}", telemetry["undershoot_count"]);
    println!(
        "Cluster buffer reallocs: {}",
        telemetry["cluster_buffer_reallocs"]
    );
    println!("Gap bytes: {}", telemetry["gap_bytes"]);
    println!("\nTelemetry exported to: {}", telemetry_path);
}

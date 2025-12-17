//! Benchmark Isolation Test
//!
//! Tests whether benchmark iterations have proper isolation or if there's
//! state contamination between runs that could cause node slot corruption.

use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};
use std::fs;
use std::io::Read;
use tempfile::TempDir;

/// Helper to directly read node slot version from disk
fn read_node_slot_version(
    path: &std::path::Path,
    node_id: i64,
) -> Result<u8, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(path)?;

    const NODE_SLOT_SIZE: u64 = 4096;
    const DEFAULT_NODE_DATA_START: u64 = 1024;

    let slot_offset = DEFAULT_NODE_DATA_START + ((node_id - 1) as u64 * NODE_SLOT_SIZE);

    use std::io::Seek;
    use std::io::SeekFrom;

    file.seek(SeekFrom::Start(slot_offset))?;
    let mut buffer = [0u8; 1];
    file.read_exact(&mut buffer)?;

    Ok(buffer[0])
}

#[test]
fn test_benchmark_iteration_isolation() {
    println!("=== BENCHMARK ITERATION ISOLATION TEST ===");

    // Simulate exactly what the benchmark does: multiple iterations with fresh temp dirs
    for iteration in 1..=3 {
        println!("\n--- ITERATION {} ---", iteration);

        // Create fresh temp directory (exactly like benchmark)
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("benchmark.db");

        println!("Created temp directory: {:?}", temp_dir.path());

        // Create V2 native graph (exactly like benchmark)
        let graph =
            open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

        // Create exactly 300 nodes (crossing the 256 boundary like the benchmark)
        let mut node_ids = Vec::new();
        for i in 1..=300 {
            let node_id = graph
                .insert_node(NodeSpec {
                    kind: "Node".to_string(),
                    name: format!("node_{}", i),
                    file_path: None,
                    data: serde_json::json!({"id": i}),
                })
                .expect(&format!("Failed to insert node {}", i));

            node_ids.push(node_id);

            // Log boundary nodes
            if i >= 250 && i <= 260 {
                println!("Created node {} -> node_id {}", i, node_id);
            }
        }

        // Immediately verify node 257 has version=2 on disk
        let version_257 =
            read_node_slot_version(&db_path, 257).expect("Should be able to read node 257 slot");
        println!("Node 257 version after node creation: {}", version_257);
        assert_eq!(
            version_257, 2,
            "Node 257 should have version=2 immediately after creation"
        );

        // Check a few critical boundary nodes
        for &node_id in &[255, 256, 257, 258, 259, 260] {
            let version = read_node_slot_version(&db_path, node_id)
                .expect(&format!("Should be able to read node {} slot", node_id));
            println!("Node {} version: {}", node_id, version);
            assert_eq!(version, 2, "Node {} should have version=2", node_id);
        }

        // Drop the graph instance (like what happens between benchmark iterations)
        drop(graph);

        // After dropping, verify nodes are still intact on disk
        println!("Verifying nodes after dropping graph instance...");

        for &node_id in &[255, 256, 257, 258, 259, 260] {
            let version = read_node_slot_version(&db_path, node_id).expect(&format!(
                "Should be able to read node {} slot after drop",
                node_id
            ));
            println!("Node {} version after drop: {}", node_id, version);
            assert_eq!(
                version, 2,
                "Node {} should still have version=2 after graph drop",
                node_id
            );
        }

        // TempDir will be automatically cleaned up when it goes out of scope
        println!(
            "Iteration {} completed successfully - temp directory will be cleaned up",
            iteration
        );
    }

    println!("\n=== ISOLATION TEST PASSED ===");
    println!("All iterations maintained proper node slot integrity");
}

#[test]
fn test_direct_corruption_reproduction() {
    println!("=== DIRECT CORRUPTION REPRODUCTION TEST ===");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test.db");

    // Create graph and nodes
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    // Create 300 nodes
    let mut node_ids = Vec::new();
    for i in 1..=300 {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: format!("node_{}", i),
                file_path: None,
                data: serde_json::json!({"id": i}),
            })
            .expect(&format!("Failed to insert node {}", i));
        node_ids.push(node_id);
    }

    // Verify node 257
    let version_257 = read_node_slot_version(&db_path, 257).unwrap();
    println!("Node 257 version after node creation: {}", version_257);
    assert_eq!(version_257, 2);

    // Drop graph
    drop(graph);

    // Verify after drop
    let version_257_after = read_node_slot_version(&db_path, 257).unwrap();
    println!("Node 257 version after graph drop: {}", version_257_after);
    assert_eq!(version_257_after, 2);

    // Re-open the graph (like what might happen in a benchmark loop)
    let graph_reopened =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to re-open V2 native graph");

    // Try to read node 257 through the API
    match graph_reopened.get_node(257) {
        Ok(node) => println!("Successfully read node 257: {}", node.name),
        Err(e) => panic!("Failed to read node 257 after reopen: {}", e),
    }

    // Verify node 257 still has version=2 on disk
    let version_257_reopened = read_node_slot_version(&db_path, 257).unwrap();
    println!("Node 257 version after reopen: {}", version_257_reopened);
    assert_eq!(version_257_reopened, 2);

    println!("=== DIRECT REPRODUCTION TEST PASSED ===");
}

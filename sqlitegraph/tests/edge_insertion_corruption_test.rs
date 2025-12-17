//! Edge Insertion Corruption Test
//!
//! Surgical test to identify exactly where node 257 gets corrupted during edge insertion.

use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};
use std::fs;
use std::io::Read;

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

fn print_node_status(db_path: &std::path::Path, node_ids: &[i64], label: &str) {
    println!("=== {} ===", label);
    for &node_id in node_ids {
        match read_node_slot_version(db_path, node_id) {
            Ok(version) => {
                let slot_offset = 1024 + ((node_id - 1) as u64 * 4096);
                println!(
                    "Node {}: slot_offset=0x{:x}, version={}",
                    node_id, slot_offset, version
                );
            }
            Err(e) => {
                println!("Node {}: ERROR reading slot - {}", node_id, e);
            }
        }
    }
}

#[test]
fn test_edge_insertion_corruption_isolation() {
    println!("=== EDGE INSERTION CORRUPTION ISOLATION TEST ===");

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create graph
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    // Create exactly 300 nodes to cross the 256 boundary
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

    // Critical nodes to monitor
    let critical_nodes = [255, 256, 257, 258, 259, 260];

    print_node_status(&db_path, &critical_nodes, "AFTER NODE CREATION");

    // Verify all critical nodes have version=2
    for &node_id in &critical_nodes {
        let version = read_node_slot_version(&db_path, node_id).unwrap();
        assert_eq!(
            version, 2,
            "Node {} should have version=2 after node creation",
            node_id
        );
    }

    println!("\n=== BEGINNING EDGE INSERTION WITH MONITORING ===");

    // Insert edges one by one and monitor critical nodes
    for i in 0..299 {
        let from_id = node_ids[i];
        let to_id = node_ids[i + 1];

        // Check critical nodes before inserting this edge
        if (from_id >= 250 && from_id <= 270) || (to_id >= 250 && to_id <= 270) {
            println!("\n--- EDGE {}: {} -> {} ---", i, from_id, to_id);

            // Check status BEFORE edge insertion
            for &node_id in &critical_nodes {
                let version = read_node_slot_version(&db_path, node_id).unwrap();
                if version != 2 {
                    println!(
                        "CORRUPTION DETECTED BEFORE EDGE {} - Node {} has version {}",
                        i, node_id, version
                    );
                    panic!(
                        "Node {} already corrupted before inserting edge {} ({} -> {})",
                        node_id, i, from_id, to_id
                    );
                }
            }
        }

        // Insert the edge - this is where corruption might happen
        match graph.insert_edge(EdgeSpec {
            from: from_id,
            to: to_id,
            edge_type: "chain".to_string(),
            data: serde_json::json!({"edge_index": i}),
        }) {
            Ok(_) => {
                // Check critical nodes after successful edge insertion
                if (from_id >= 250 && from_id <= 270) || (to_id >= 250 && to_id <= 270) {
                    print_node_status(
                        &db_path,
                        &critical_nodes,
                        &format!("AFTER EDGE {}: {} -> {}", i, from_id, to_id),
                    );

                    // Check if any node got corrupted
                    for &node_id in &critical_nodes {
                        let version = read_node_slot_version(&db_path, node_id).unwrap();
                        if version != 2 {
                            println!(
                                "🔥 CORRUPTION DETECTED! Node {} has version {} after inserting edge {} ({} -> {})",
                                node_id, version, i, from_id, to_id
                            );
                            panic!(
                                "CORRUPTION CONFIRMED: Edge {} ({} -> {}) corrupted node {} to version {}",
                                i, from_id, to_id, node_id, version
                            );
                        }
                    }
                }
            }
            Err(e) => {
                println!("Edge {} ({} -> {}) FAILED: {}", i, from_id, to_id, e);

                // Check if this is the corruption we're looking for
                if e.to_string().contains("uninitialized slot")
                    || e.to_string().contains("version=0")
                {
                    println!(
                        "🔥 TARGET CORRUPTION DETECTED at edge {} ({} -> {})",
                        i, from_id, to_id
                    );
                    print_node_status(&db_path, &critical_nodes, "CORRUPTION DETECTED");
                    panic!(
                        "REPRODUCED: V2 uninitialized slot corruption detected at edge {}",
                        i
                    );
                } else {
                    // Some other error
                    panic!("Unexpected error (not uninitialized slot): {}", e);
                }
            }
        }
    }

    println!("\n=== EDGE INSERTION COMPLETED SUCCESSFULLY ===");
    print_node_status(&db_path, &critical_nodes, "FINAL VERIFICATION");

    // Final verification
    for &node_id in &critical_nodes {
        let version = read_node_slot_version(&db_path, node_id).unwrap();
        assert_eq!(
            version, 2,
            "Node {} should still have version=2 after all edge insertions",
            node_id
        );
    }

    println!("✅ NO CORRUPTION DETECTED - All edge insertions completed successfully");
}

#[test]
fn test_direct_edge_to_node_257() {
    println!("=== DIRECT EDGE TO NODE 257 TEST ===");

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create graph
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    // Create exactly 257 nodes to focus on boundary
    let mut node_ids = Vec::new();
    for i in 1..=257 {
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

    print_node_status(&db_path, &[255, 256, 257], "AFTER NODE CREATION");

    // Verify node 257 has version=2
    let version_257 = read_node_slot_version(&db_path, 257).unwrap();
    assert_eq!(
        version_257, 2,
        "Node 257 should have version=2 after creation"
    );

    // Try to insert a single edge to node 257 - this should trigger corruption if it exists
    println!("Inserting edge: 256 -> 257");
    match graph.insert_edge(EdgeSpec {
        from: node_ids[255], // node 256
        to: node_ids[256],   // node 257
        edge_type: "test".to_string(),
        data: serde_json::json!({"test": true}),
    }) {
        Ok(_) => {
            println!("Edge 256 -> 257 inserted successfully");
            let version_257_after = read_node_slot_version(&db_path, 257).unwrap();
            println!(
                "Node 257 version after edge insertion: {}",
                version_257_after
            );
            assert_eq!(
                version_257_after, 2,
                "Node 257 should still have version=2 after edge insertion"
            );
        }
        Err(e) => {
            println!("Edge 256 -> 257 failed: {}", e);
            if e.to_string().contains("uninitialized slot") || e.to_string().contains("version=0") {
                panic!("REPRODUCED: Node 257 corruption when inserting edge 256 -> 257");
            } else {
                panic!("Different error occurred: {}", e);
            }
        }
    }

    println!("Direct edge to node 257 test completed successfully");
}

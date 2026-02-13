//! V2 BFS-Style Node Uninitialized Regression Test
//!
//! Reproduces the "V2 file contains uninitialized slot (version=0)" corruption
//! that occurs in the BFS benchmark. This test creates nodes beyond 256
//! and then performs edge insertion in the same pattern as the BFS benchmark
//! to trigger the corruption deterministically.

use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};
use std::fs::File;
use std::io::Read;

// Constants from the actual code (sqlitegraph/src/backend/native/graph_file.rs)
const NODE_SLOT_SIZE: u64 = 4096;
const DEFAULT_NODE_DATA_START: u64 = 1024;

/// Helper to directly read node slot version from disk
fn read_node_slot_version(
    path: &std::path::Path,
    node_id: i64,
) -> Result<u8, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;

    // Use the same formula as the actual code:
    // node_data_offset + ((node_id - 1) as u64 * 4096)
    // where node_data_offset = DEFAULT_NODE_DATA_START = 1024
    let slot_offset = DEFAULT_NODE_DATA_START + ((node_id - 1) as u64 * NODE_SLOT_SIZE);

    // Read just the first byte (version) from the slot
    use std::io::Seek;
    use std::io::SeekFrom;

    file.seek(SeekFrom::Start(slot_offset))?;
    let mut buffer = [0u8; 1];
    file.read_exact(&mut buffer)?;

    Ok(buffer[0])
}

/// Print forensic information about a node slot
fn print_node_slot_forensics(path: &std::path::Path, node_id: i64, label: &str) {
    match read_node_slot_version(path, node_id) {
        Ok(version) => {
            let slot_offset = DEFAULT_NODE_DATA_START + ((node_id - 1) as u64 * NODE_SLOT_SIZE);
            println!(
                "{}: node_id={}, slot_offset=0x{:x}, version={}",
                label, node_id, slot_offset, version
            );
        }
        Err(e) => {
            println!("{}: node_id={}, ERROR reading slot: {}", label, node_id, e);
        }
    }
}

#[test]
fn test_v2_bfs_style_node_uninitialized_regression() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    println!("Creating V2 native graph at: {:?}", db_path);

    // Create V2 native graph
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    println!("STEP 1: Creating 300 nodes (crossing 256 boundary)...");

    // Create 300 nodes to cross the 256 boundary
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

    println!("STEP 2: Verifying node slots have version=2 on disk...");

    // Verify critical boundary nodes have version=2 directly from disk
    let critical_nodes = [257, 300];
    for &node_id in &critical_nodes {
        print_node_slot_forensics(&db_path, node_id, "AFTER_NODE_CREATION");

        let version = read_node_slot_version(&db_path, node_id)
            .expect(&format!("Should be able to read node {} slot", node_id));

        assert_eq!(
            version, 2,
            "Node {} slot should have version=2 immediately after creation, got version={}",
            node_id, version
        );
    }

    println!("STEP 3: Beginning edge insertion phase (like BFS benchmark)...");

    // Insert edges in the same pattern as BFS benchmark
    // This should trigger the corruption if it exists
    for i in 0..299 {
        let from_id = node_ids[i];
        let to_id = node_ids[i + 1];

        // Check slot status before inserting edge that touches critical nodes
        if (i >= 255 && i <= 257) || (i >= 298 && i <= 299) {
            print_node_slot_forensics(&db_path, from_id, &format!("BEFORE_EDGE_{}_FROM", i));
            print_node_slot_forensics(&db_path, to_id, &format!("BEFORE_EDGE_{}_TO", i));
        }

        // This should eventually fail with "uninitialized slot (version=0)"
        match graph.insert_edge(EdgeSpec {
            from: from_id,
            to: to_id,
            edge_type: "chain".to_string(),
            data: serde_json::json!({"edge_index": i}),
        }) {
            Ok(_) => {
                // Success - continue
                if i <= 5 || (i >= 255 && i <= 260) {
                    println!("Edge {}: {} -> {} SUCCESS", i, from_id, to_id);
                }
            }
            Err(e) => {
                println!("Edge {}: {} -> {} FAILED: {}", i, from_id, to_id, e);

                // Check if this is the corruption we're looking for
                if e.to_string().contains("uninitialized slot")
                    || e.to_string().contains("version=0")
                {
                    println!(
                        "CORRUPTION DETECTED at edge {} ({} -> {})",
                        i, from_id, to_id
                    );

                    // Print forensic information
                    print_node_slot_forensics(&db_path, from_id, "CORRUPTION_FROM_NODE");
                    print_node_slot_forensics(&db_path, to_id, "CORRUPTION_TO_NODE");

                    // This is expected to fail on current HEAD before the fix
                    panic!(
                        "REPRODUCED: V2 uninitialized slot corruption detected at edge {}",
                        i
                    );
                } else {
                    // Some other error - not what we're looking for
                    panic!("Unexpected error (not uninitialized slot): {}", e);
                }
            }
        }
    }

    println!("STEP 4: Final verification of critical node slots...");

    // Final verification - if we get here, the bug is fixed
    for &node_id in &critical_nodes {
        print_node_slot_forensics(&db_path, node_id, "FINAL_VERIFICATION");

        let version = read_node_slot_version(&db_path, node_id)
            .expect(&format!("Should be able to read node {} slot", node_id));

        assert_eq!(
            version, 2,
            "Node {} slot should still have version=2 after all operations, got version={}",
            node_id, version
        );
    }

    println!("SUCCESS: No corruption detected - all node slots maintain version=2");
}

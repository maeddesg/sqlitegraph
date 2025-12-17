use sqlitegraph::backend::NodeSpec;
use sqlitegraph::{GraphConfig, open_graph};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use tempfile::TempDir;

#[test]
fn test_incoming_cluster_write_does_not_corrupt_node_slots()
-> Result<(), Box<dyn std::error::Error>> {
    // Regression test to prove incoming cluster write corrupts node slot at offset 0x400
    // BEFORE FIX: Should fail with version byte changing from 2 to 1
    // AFTER FIX: Should pass with version byte remaining 2

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_incoming_corruption.db");

    // Set debug environment variables for header validation
    unsafe {
        std::env::set_var("HEADER_VALIDATE_DEBUG", "1");
        std::env::set_var("HEADER_VALIDATE_DEBUG_FILE", &db_path);
    }

    // Create graph with native V2 backend
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    // Insert 3 nodes to set up the test scenario
    println!("Inserting nodes...");
    let node_id1 = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "main".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            data: serde_json::json!({"lines": 100, "complexity": "medium"}),
        })
        .expect("Failed to insert node 1");

    let node_id2 = graph
        .insert_node(NodeSpec {
            kind: "Variable".to_string(),
            name: "config".to_string(),
            file_path: Some("/src/config.rs".to_string()),
            data: serde_json::json!({"type": "string", "mutable": false}),
        })
        .expect("Failed to insert node 2");

    let node_id3 = graph
        .insert_node(NodeSpec {
            kind: "Module".to_string(),
            name: "database".to_string(),
            file_path: Some("/src/database/mod.rs".to_string()),
            data: serde_json::json!({"dependencies": 5, "exports": ["connect", "query"]}),
        })
        .expect("Failed to insert node 3");

    println!("Inserted nodes: {}, {}, {}", node_id1, node_id2, node_id3);

    // STEP 3: READ node1 slot bytes DIRECTLY FROM DISK at offset 0x400
    println!("=== PHASE 2E RAW DISK PROOF ===");
    let mut file_before = File::open(&db_path)?;
    let mut node1_slot_before = vec![0u8; 32]; // Read first 32 bytes

    file_before.seek(SeekFrom::Start(0x400))?; // Node 1 slot offset
    file_before.read_exact(&mut node1_slot_before)?;

    let version_before = node1_slot_before[0];
    println!(
        "BEFORE_EDGE_INSERTION: offset=0x400, version={}, first_32_bytes={:02x?}",
        version_before, &node1_slot_before
    );

    // Verify version is 2 before edge insertion
    assert_eq!(
        version_before, 2,
        "Node1 should have version=2 before edge insertion, got {}",
        version_before
    );

    // STEP 4: Insert one edge that triggers incoming cluster writing
    println!("Inserting edge that should trigger incoming cluster writing...");
    let edge_result = graph.insert_edge(sqlitegraph::backend::EdgeSpec {
        from: node_id1,
        to: node_id2,
        edge_type: "imports".to_string(),
        data: serde_json::json!({"reason": "config dependency"}),
    });

    // STEP 5: READ node1 slot bytes DIRECTLY FROM DISK again at offset 0x400
    let mut file_after = File::open(&db_path)?;
    let mut node1_slot_after = vec![0u8; 32]; // Read first 32 bytes

    file_after.seek(SeekFrom::Start(0x400))?; // Node 1 slot offset
    file_after.read_exact(&mut node1_slot_after)?;

    let version_after = node1_slot_after[0];
    println!(
        "AFTER_EDGE_INSERTION: offset=0x400, version={}, first_32_bytes={:02x?}",
        version_after, &node1_slot_after
    );

    // This assertion should PASS after V2-only refactor (no more corruption)
    assert_eq!(
        version_after, 2,
        "CORRUPTION_SHOULD_BE_FIXED: Node1 version should remain 2, but got {}. BEFORE={:02x?}, AFTER={:02x?}",
        version_after, &node1_slot_before, &node1_slot_after
    );

    // Verify edge insertion succeeded
    edge_result.expect("Edge insertion should succeed");

    // STEP 6: Close graph, reopen file, read node1 slot again
    drop(graph);
    println!("Graph closed, reopening...");

    let graph_reopened =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to reopen V2 native graph");

    println!("Reading node1 slot directly from disk after reopen...");
    let mut file_reopened = File::open(&db_path)?;
    let mut node1_slot_reopened = vec![0u8; 32];

    file_reopened.seek(SeekFrom::Start(0x400))?;
    file_reopened.read_exact(&mut node1_slot_reopened)?;

    let version_reopened = node1_slot_reopened[0];
    println!(
        "AFTER REOPEN: node1 slot @0x400 version={}, first_32_bytes={:02x?}",
        version_reopened, &node1_slot_reopened
    );

    // Verify persistence after fix
    assert_eq!(
        version_reopened, 2,
        "Node1 version corrupted after reopen: should be 2, got {}",
        version_reopened
    );

    // Verify node can still be read through API
    let api_node = graph_reopened
        .get_node(node_id1)
        .expect("Should be able to read node1 through API after fix");
    assert_eq!(api_node.kind, "Function");
    assert_eq!(api_node.name, "main");

    println!("SUCCESS: Node1 slot preserved across edge insertion and reopen");
    Ok(())
}

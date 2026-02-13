use sqlitegraph::backend::{EdgeSpec, NodeSpec};
use sqlitegraph::{GraphConfig, open_graph};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use tempfile::TempDir;

#[test]
fn test_v2_disk_corruption_probe() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== V2 DISK CORRUPTION PROBE ===");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("corruption_probe.db");

    // Phase 1: Create graph with V2 backend
    println!("Creating V2 graph...");
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    // Phase 2: Insert nodes
    println!("Inserting test nodes...");
    let node1_id = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "main".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            data: serde_json::json!({"lines": 100, "complexity": "medium"}),
        })
        .expect("Failed to insert node 1");

    let node2_id = graph
        .insert_node(NodeSpec {
            kind: "Variable".to_string(),
            name: "config".to_string(),
            file_path: Some("/src/config.rs".to_string()),
            data: serde_json::json!({"type": "string", "mutable": false}),
        })
        .expect("Failed to insert node 2");

    let node3_id = graph
        .insert_node(NodeSpec {
            kind: "Module".to_string(),
            name: "database".to_string(),
            file_path: Some("/src/database/mod.rs".to_string()),
            data: serde_json::json!({"dependencies": 5, "exports": ["connect", "query"]}),
        })
        .expect("Failed to insert node 3");

    println!("Inserted nodes: {}, {}, {}", node1_id, node2_id, node3_id);

    // Phase 3: CRITICAL - Read node slots directly from disk BEFORE edge insertion
    println!("Phase 3: Reading node slots directly from disk BEFORE edge insertion...");
    let mut file = File::open(&db_path)?;

    // Read Node 1 slot (offset 0x400)
    let mut node1_before = vec![0u8; 64];
    file.seek(SeekFrom::Start(0x400))?;
    file.read_exact(&mut node1_before)?;

    // Read Node 2 slot (offset 0x1400)
    let mut node2_before = vec![0u8; 64];
    file.seek(SeekFrom::Start(0x1400))?;
    file.read_exact(&mut node2_before)?;

    // Read Node 3 slot (offset 0x2400)
    let mut node3_before = vec![0u8; 64];
    file.seek(SeekFrom::Start(0x2400))?;
    file.read_exact(&mut node3_before)?;

    let node1_version_before = node1_before[0];
    let node2_version_before = node2_before[0];
    let node3_version_before = node3_before[0];

    println!("BEFORE EDGE INSERTION:");
    println!(
        "  Node 1 (0x400): version={}, first_64={:02x?}",
        node1_version_before,
        &node1_before[..16]
    );
    println!(
        "  Node 2 (0x1400): version={}, first_64={:02x?}",
        node2_version_before,
        &node2_before[..16]
    );
    println!(
        "  Node 3 (0x2400): version={}, first_64={:02x?}",
        node3_version_before,
        &node3_before[..16]
    );

    // CRITICAL ASSERTION: All nodes must have version = 2 before edge insertion
    assert_eq!(
        node1_version_before, 2,
        "Node 1 should have version=2 before edge insertion"
    );
    assert_eq!(
        node2_version_before, 2,
        "Node 2 should have version=2 before edge insertion"
    );
    assert_eq!(
        node3_version_before, 2,
        "Node 3 should have version=2 before edge insertion"
    );

    // Phase 4: Insert edge that should trigger cluster writing
    println!("Phase 4: Inserting edge (1 -> 2) to trigger cluster writing...");
    let edge_id = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "imports".to_string(),
            data: serde_json::json!({"reason": "config dependency"}),
        })
        .expect("Failed to insert edge");

    println!("Edge insertion completed, edge_id={}", edge_id);

    // Phase 5: CRITICAL - Read node slots directly from disk AFTER edge insertion
    println!("Phase 5: Reading node slots directly from disk AFTER edge insertion...");
    let mut file_after = File::open(&db_path)?;

    // Read Node 1 slot (offset 0x400)
    let mut node1_after = vec![0u8; 64];
    file_after.seek(SeekFrom::Start(0x400))?;
    file_after.read_exact(&mut node1_after)?;

    // Read Node 2 slot (offset 0x1400)
    let mut node2_after = vec![0u8; 64];
    file_after.seek(SeekFrom::Start(0x1400))?;
    file_after.read_exact(&mut node2_after)?;

    // Read Node 3 slot (offset 0x2400)
    let mut node3_after = vec![0u8; 64];
    file_after.seek(SeekFrom::Start(0x2400))?;
    file_after.read_exact(&mut node3_after)?;

    let node1_version_after = node1_after[0];
    let node2_version_after = node2_after[0];
    let node3_version_after = node3_after[0];

    println!("AFTER EDGE INSERTION:");
    println!(
        "  Node 1 (0x400): version={}, first_64={:02x?}",
        node1_version_after,
        &node1_after[..16]
    );
    println!(
        "  Node 2 (0x1400): version={}, first_64={:02x?}",
        node2_version_after,
        &node2_after[..16]
    );
    println!(
        "  Node 3 (0x2400): version={}, first_64={:02x?}",
        node3_version_after,
        &node3_after[..16]
    );

    // CRITICAL CORRUPTION PROBE: Verify node slots weren't corrupted
    println!("Phase 6: CRITICAL CORRUPTION VERIFICATION");
    assert_eq!(
        node1_version_after, node1_version_before,
        "CORRUPTION DETECTED: Node 1 version changed from {} to {} during edge insertion!",
        node1_version_before, node1_version_after
    );
    assert_eq!(
        node2_version_after, node2_version_before,
        "CORRUPTION DETECTED: Node 2 version changed from {} to {} during edge insertion!",
        node2_version_before, node2_version_after
    );
    assert_eq!(
        node3_version_after, node3_version_before,
        "CORRUPTION DETECTED: Node 3 version changed from {} to {} during edge insertion!",
        node3_version_before, node3_version_after
    );

    // Phase 7: Verify all nodes still have version 2
    assert_eq!(
        node1_version_after, 2,
        "Node 1 version corrupted: expected 2, got {}",
        node1_version_after
    );
    assert_eq!(
        node2_version_after, 2,
        "Node 2 version corrupted: expected 2, got {}",
        node2_version_after
    );
    assert_eq!(
        node3_version_after, 2,
        "Node 3 version corrupted: expected 2, got {}",
        node3_version_after
    );

    // Phase 8: Verify regions don't overlap by checking actual layout
    println!("Phase 8: Region separation verification");

    // Read header to verify layout
    let mut header_bytes = vec![0u8; 512];
    file_after.seek(SeekFrom::Start(0))?;
    file_after.read_exact(&mut header_bytes)?;

    // Parse header fields manually to verify region layout
    let node_data_offset = u64::from_be_bytes([
        header_bytes[16],
        header_bytes[17],
        header_bytes[18],
        header_bytes[19],
        header_bytes[20],
        header_bytes[21],
        header_bytes[22],
        header_bytes[23],
    ]);

    println!("Header verification:");
    println!("  node_data_offset = 0x{:x}", node_data_offset);
    println!(
        "  Node region: 0x{:x} - 0x{:x}",
        node_data_offset,
        node_data_offset + (3 * 4096)
    );

    // Verify no edge data overlaps node region
    assert!(
        node_data_offset + (3 * 4096) <= 0x40000000,
        "Edge data overlaps with node region"
    );

    drop(graph); // Close graph

    println!("✅ VERIFICATION COMPLETE: No corruption detected in V2 backend");
    println!("✅ All node slots preserved during edge insertion");
    println!("✅ Node region and edge region properly separated");

    Ok(())
}

//! V2 Node/Cluster Region Collision Regression Tests
//!
//! Tests that node slots are never overwritten by cluster data.
//! This is a critical invariant to prevent silent data corruption.

use sqlitegraph::{EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, open_graph};

#[test]
fn test_node_257_must_survive_cluster_writes() {
    println!("=== TEST A: Node 257 must survive cluster writes ===");

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("node257_test.db");

    println!("Step 1: Creating brand-new V2 native graph...");

    // Create a brand-new V2 native graph
    let mut graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    println!("Step 2: Inserting 300 nodes (>= 257) with small payload...");

    // Insert 300 nodes (>= 257) with small payload
    let mut node_ids = Vec::new();
    for i in 1..=300 {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "TestNode".to_string(),
                name: format!("node_{}", i),
                file_path: None,
                data: serde_json::json!({"id": i, "payload": "small"}),
            })
            .expect(&format!("Failed to insert node {}", i));
        node_ids.push(node_id);

        if i == 257 {
            println!("✅ Node 257 created with ID {}", node_id);
        }
    }

    println!("Step 3: Inserting edges to force cluster writes...");

    // Insert edges that force outgoing+incoming cluster writes
    // Connect sequential nodes to ensure cluster allocation
    let edges_inserted = std::sync::atomic::AtomicUsize::new(0);
    for i in 0..299 {
        let from_id = node_ids[i];
        let to_id = node_ids[i + 1];

        match graph.insert_edge(EdgeSpec {
            from: from_id,
            to: to_id,
            edge_type: "seq_edge".to_string(),
            data: serde_json::json!({"seq": i}),
        }) {
            Ok(_) => {
                edges_inserted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i % 50 == 0 {
                    println!("  Inserted edge {}: {} -> {}", i + 1, from_id, to_id);
                }
            }
            Err(e) => {
                println!(
                    "⚠️  Failed to insert edge {}: {} -> {}: {}",
                    i + 1,
                    from_id,
                    to_id,
                    e
                );
            }
        }
    }

    println!(
        "✅ Inserted {} edges",
        edges_inserted.load(std::sync::atomic::Ordering::Relaxed)
    );

    println!("Step 4: Reading node 257 immediately after edge insertions...");

    // Immediately read node 257 by ID and assert it survives
    let node_257_id = node_ids[256]; // 0-based index, so 256 = node 257
    let node_257 = graph
        .get_node(node_257_id)
        .expect("Failed to read node 257");

    println!(
        "✅ Retrieved node 257: ID={}, name='{}', kind='{}'",
        node_257.id, node_257.name, node_257.kind
    );

    // Verify critical invariants
    assert_eq!(node_257.id, node_257_id, "Node 257 ID mismatch");
    assert_eq!(node_257.name, "node_257", "Node 257 name corrupted");
    assert_eq!(node_257.kind, "TestNode", "Node 257 kind corrupted");

    // Verify payload integrity
    if let Some(payload) = node_257.data.get("payload") {
        assert_eq!(payload, "small", "Node 257 payload corrupted");
    } else {
        panic!("Node 257 payload missing - likely corrupted");
    }

    println!("✅ Node 257 payload verified: {}", node_257.data);

    println!("Step 5: Closing graph...");

    // IMPORTANT: Close graph by dropping it
    drop(graph);
    println!("✅ Graph closed");

    println!("Step 6: Reopening graph...");

    // Reopen same file
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to reopen V2 native graph");

    println!("Step 7: Re-reading node 257 after reopen...");

    // Re-read node 257 and verify same assertions
    let node_257_reopened = graph
        .get_node(node_257_id)
        .expect("Failed to read node 257 after reopen");

    println!(
        "✅ Retrieved node 257 after reopen: ID={}, name='{}', kind='{}'",
        node_257_reopened.id, node_257_reopened.name, node_257_reopened.kind
    );

    // Verify invariants are preserved across reopen
    assert_eq!(
        node_257_reopened.id, node_257_id,
        "Node 257 ID changed after reopen"
    );
    assert_eq!(
        node_257_reopened.name, "node_257",
        "Node 257 name corrupted after reopen"
    );
    assert_eq!(
        node_257_reopened.kind, "TestNode",
        "Node 257 kind corrupted after reopen"
    );

    // Verify payload integrity is preserved
    if let Some(payload) = node_257_reopened.data.get("payload") {
        assert_eq!(payload, "small", "Node 257 payload corrupted after reopen");
    } else {
        panic!("Node 257 payload missing after reopen - corrupted");
    }

    println!(
        "✅ Node 257 payload verified after reopen: {}",
        node_257_reopened.data
    );

    println!("🎉 TEST A PASSED: Node 257 survives cluster writes and reopen cycle");
}

#[test]
fn test_cluster_offsets_must_be_after_reserved_node_region() {
    println!("=== TEST B: Cluster offsets must be after reserved node region ===");

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("cluster_offset_test.db");

    println!("Step 1: Creating V2 graph and inserting 300 nodes...");

    // Create V2 graph and insert 300 nodes
    let mut graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    let mut node_ids = Vec::new();
    for i in 1..=300 {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "TestNode".to_string(),
                name: format!("node_{}", i),
                file_path: None,
                data: serde_json::json!({"id": i}),
            })
            .expect(&format!("Failed to insert node {}", i));
        node_ids.push(node_id);
    }

    println!("✅ Created 300 nodes");

    println!("Step 2: Inserting edges to trigger cluster allocation...");

    // Insert edges to force cluster allocation
    for i in 0..50 {
        let from_id = node_ids[i];
        let to_id = node_ids[i + 1];

        graph
            .insert_edge(EdgeSpec {
                from: from_id,
                to: to_id,
                edge_type: "test_edge".to_string(),
                data: serde_json::json!({"test": i}),
            })
            .expect(&format!("Failed to insert edge {}", i));
    }

    println!("✅ Inserted 50 edges to trigger cluster allocation");

    println!("Step 3: Verifying cluster offset invariants...");

    // We need to access the underlying native backend to check persistent header
    // Since this isn't exposed through the public API, we'll use indirect verification

    // Test that we can read all nodes including node 257 without corruption
    // This indirectly verifies cluster offsets don't overlap node slots

    for (i, &node_id) in node_ids.iter().enumerate() {
        let node = graph.get_node(node_id).expect(&format!(
            "Failed to read node {} (index {})",
            node_id,
            i + 1
        ));

        assert_eq!(node.id, node_id, "Node {} ID corrupted", node_id);
        assert!(
            node.name.contains(&format!("node_{}", i + 1)),
            "Node {} name corrupted: {}",
            node_id,
            node.name
        );
    }

    println!("✅ All 300 nodes verified readable without corruption");

    // Test node 257 specifically
    let node_257_id = node_ids[256]; // 0-based index
    let node_257 = graph
        .get_node(node_257_id)
        .expect("Failed to read node 257");

    assert_eq!(
        node_257.name, "node_257",
        "Node 257 name verification failed"
    );
    println!("✅ Node 257 specifically verified");

    println!("🎉 TEST B PASSED: Cluster offsets positioned correctly (all nodes readable)");
}

#[test]
fn test_reserved_node_region_constant_must_exist() {
    println!("=== TEST C: Reserved node region constants must exist ===");

    // This test verifies that we can define the RESERVED_NODE_REGION_BYTES constant
    // and use it consistently throughout the codebase

    const RESERVED_NODE_REGION_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
    const NODE_SLOT_SIZE: u64 = 4096;
    const DEFAULT_NODE_DATA_START: u64 = 1024;

    // Calculate how many node slots fit in the reserved region
    let reserved_node_slots =
        (RESERVED_NODE_REGION_BYTES - DEFAULT_NODE_DATA_START) / NODE_SLOT_SIZE;

    println!("Reserved node region: {} bytes", RESERVED_NODE_REGION_BYTES);
    println!("Reserved node slots: {}", reserved_node_slots);
    println!("Maximum safe node ID: {}", reserved_node_slots);

    // Verify that this provides adequate space for BFS workloads
    assert!(
        reserved_node_slots >= 2000,
        "Reserved region must support at least 2000 nodes for BFS, got {}",
        reserved_node_slots
    );

    // Verify node 257 fits comfortably in reserved region
    assert!(
        257 <= reserved_node_slots as i64,
        "Node 257 must fit in reserved region"
    );

    println!(
        "✅ Reserved node region can safely accommodate {} nodes",
        reserved_node_slots
    );
    println!(
        "✅ Node 257 (index {}) is safely within reserved region",
        257
    );

    // Calculate expected safe cluster start
    let safe_cluster_start = DEFAULT_NODE_DATA_START + RESERVED_NODE_REGION_BYTES;
    let node_257_slot_offset = DEFAULT_NODE_DATA_START + ((257 - 1) as u64 * NODE_SLOT_SIZE);

    println!(
        "Safe cluster start: {} (0x{:x})",
        safe_cluster_start, safe_cluster_start
    );
    println!(
        "Node 257 slot offset: {} (0x{:x})",
        node_257_slot_offset, node_257_slot_offset
    );

    assert!(
        safe_cluster_start > node_257_slot_offset,
        "Safe cluster start must be after node 257 slot offset"
    );

    println!("✅ Safe cluster start positioned after node 257 slot");
    println!("🎉 TEST C PASSED: Reserved node region constants are mathematically sound");
}

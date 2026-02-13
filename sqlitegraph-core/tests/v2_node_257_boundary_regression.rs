//! V2 Node 257 Boundary Regression Test
//!
//! This test verifies that node creation works correctly beyond the 256 boundary.
//! The BFS benchmark was failing because node creation stopped at ID 256,
//! causing edge insertion to fail when trying to read node 257.

use sqlitegraph::{GraphConfig, NodeSpec, SnapshotId, open_graph};

#[test]
fn test_v2_node_creation_beyond_256() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let graph = open_graph(&db_path, &GraphConfig::native()).unwrap();

    // Create nodes up to and beyond the 256 boundary
    let mut node_ids = Vec::new();

    println!("Creating nodes 1..300 to test boundary at 256...");

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

        // Log critical boundary nodes
        if i >= 250 && i <= 260 {
            println!("Created node {} with ID {}", i, node_id);
        }
    }

    println!("Total nodes created: {}", node_ids.len());
    assert_eq!(node_ids.len(), 300, "Should have created 300 nodes");

    // Verify nodes around the 256 boundary exist and can be read
    for node_id in 250..=260 {
        match graph.get_node(SnapshotId::current(), node_id) {
            Ok(node) => {
                println!(
                    "SUCCESS: Node {} exists: {} (ID: {})",
                    node_id, node.name, node.id
                );
                assert_eq!(node.id, node_id);
            }
            Err(e) => {
                panic!("ERROR: Cannot read node {}: {:?}", node_id, e);
            }
        }
    }

    // Specifically test node 257 (the problematic one from BFS benchmark)
    match graph.get_node(SnapshotId::current(), 257) {
        Ok(node) => {
            println!("SUCCESS: Node 257 exists: {} (ID: {})", node.name, node.id);
            assert_eq!(node.id, 257);
            assert_eq!(node.name, "node_257");
        }
        Err(e) => {
            panic!("CRITICAL: Node 257 cannot be read: {:?}", e);
        }
    }

    // Test edge creation across the boundary
    for i in 0..299 {
        let from_id = node_ids[i];
        let to_id = node_ids[i + 1];

        graph
            .insert_edge(sqlitegraph::EdgeSpec {
                from: from_id,
                to: to_id,
                edge_type: "test".to_string(),
                data: serde_json::json!({"index": i}),
            })
            .expect(&format!("Failed to insert edge {}->{}", from_id, to_id));
    }

    println!("Successfully created {} edges across 256 boundary", 299);
}

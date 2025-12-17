//! V2 Edge Cluster Corruption Regression Test
//!
//! This test reproduces the V2 edge cluster corruption issue deterministically.
//! It creates fresh TempDir, opens V2 native backend, inserts nodes and edges,
//! then verifies that cluster corruption occurs during neighbor queries.

use sqlitegraph::backend::{BackendDirection, EdgeSpec, NeighborQuery, NodeSpec};
use sqlitegraph::{BackendKind, GraphBackend, config::GraphConfig, open_graph};
use tempfile::TempDir;

/// Test that reproduces V2 edge cluster corruption deterministically
#[test]
fn test_v2_edge_cluster_corruption_regression() {
    // Create fresh TempDir for isolation
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("v2_corruption_test.db");

    // Configure V2 native backend
    let cfg = GraphConfig::new(BackendKind::Native);

    println!("=== PHASE 1: Creating V2 native backend ===");
    let graph = open_graph(&db_path, &cfg).expect("Failed to create V2 native graph");

    println!("=== PHASE 2: Inserting nodes ===");
    // Insert 3 nodes to create a simple graph structure
    let node_a = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "node_a".to_string(),
            file_path: Some("/src/a.rs".to_string()),
            data: serde_json::json!({"lines": 100}),
        })
        .expect("Failed to insert node_a");
    assert!(node_a > 0, "Node ID should be positive");
    println!("Inserted node_a with ID: {}", node_a);

    let node_b = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "node_b".to_string(),
            file_path: Some("/src/b.rs".to_string()),
            data: serde_json::json!({"lines": 50}),
        })
        .expect("Failed to insert node_b");
    assert!(node_b > 0, "Node ID should be positive");
    println!("Inserted node_b with ID: {}", node_b);

    let node_c = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "node_c".to_string(),
            file_path: Some("/src/c.rs".to_string()),
            data: serde_json::json!({"lines": 75}),
        })
        .expect("Failed to insert node_c");
    assert!(node_c > 0, "Node ID should be positive");
    println!("Inserted node_c with ID: {}", node_c);

    // Verify nodes can be retrieved
    let entity_a = graph.get_node(node_a).expect("Failed to get node_a");
    assert_eq!(entity_a.name, "node_a");
    println!("Verified node_a retrieval");

    println!("=== PHASE 3: Inserting edges to trigger cluster writes ===");
    // Insert edges that will trigger both outgoing and incoming cluster writes
    let edge_ab = graph
        .insert_edge(EdgeSpec {
            from: node_a,
            to: node_b,
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({"line": 10}),
        })
        .expect("Failed to insert edge_ab");
    assert!(edge_ab > 0, "Edge ID should be positive");
    println!("Inserted edge_ab with ID: {}", edge_ab);

    let edge_bc = graph
        .insert_edge(EdgeSpec {
            from: node_b,
            to: node_c,
            edge_type: "USES".to_string(),
            data: serde_json::json!({"line": 5}),
        })
        .expect("Failed to insert edge_bc");
    assert!(edge_bc > 0, "Edge ID should be positive");
    println!("Inserted edge_bc with ID: {}", edge_bc);

    let edge_ca = graph
        .insert_edge(EdgeSpec {
            from: node_c,
            to: node_a,
            edge_type: "RETURNS_TO".to_string(),
            data: serde_json::json!({"line": 15}),
        })
        .expect("Failed to insert edge_ca");
    assert!(edge_ca > 0, "Edge ID should be positive");
    println!("Inserted edge_ca with ID: {}", edge_ca);

    println!("=== PHASE 4: Close backend ===");
    // Explicitly drop/close the graph backend
    drop(graph);
    println!("Backend closed");

    println!("=== PHASE 5: Reopen backend ===");
    let graph = open_graph(&db_path, &cfg).expect("Failed to reopen V2 native graph");
    println!("Backend reopened successfully");

    println!("=== PHASE 6: Run neighbor queries to trigger corruption detection ===");
    // This is where the cluster corruption should manifest
    // Based on previous errors, this should trigger: "Corrupt edge record 1: V2 FRAMED: Cluster corruption detected"

    // Test outgoing neighbors from node_a
    println!("Querying outgoing neighbors from node_a...");
    let neighbors_a_out = match graph.neighbors(
        node_a,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    ) {
        Ok(neighbors) => {
            println!("✅ node_a outgoing neighbors: {:?}", neighbors);
            neighbors
        }
        Err(e) => {
            println!("❌ node_a outgoing neighbors failed with error: {}", e);
            panic!("V2 edge cluster corruption reproduced: {}", e);
        }
    };

    // Test incoming neighbors to node_a
    println!("Querying incoming neighbors to node_a...");
    let neighbors_a_in = match graph.neighbors(
        node_a,
        NeighborQuery {
            direction: BackendDirection::Incoming,
            edge_type: None,
        },
    ) {
        Ok(neighbors) => {
            println!("✅ node_a incoming neighbors: {:?}", neighbors);
            neighbors
        }
        Err(e) => {
            println!("❌ node_a incoming neighbors failed with error: {}", e);
            panic!("V2 edge cluster corruption reproduced: {}", e);
        }
    };

    println!("=== PHASE 7: Verify expected graph structure ===");
    // At this point, if we haven't hit corruption, verify the graph structure
    // Based on our edge insertions:
    // node_a -> node_b (edge_ab)
    // node_b -> node_c (edge_bc)
    // node_c -> node_a (edge_ca)

    // node_a should have 1 outgoing neighbor (node_b) and 1 incoming neighbor (node_c)
    assert_eq!(neighbors_a_out, vec![node_b], "node_a should call node_b");
    assert_eq!(
        neighbors_a_in,
        vec![node_c],
        "node_c should return to node_a"
    );

    // Test all nodes' neighbor counts to be thorough
    for (node_id, expected_outgoing, expected_incoming) in [
        (node_a, vec![node_b], vec![node_c]),
        (node_b, vec![node_c], vec![node_a]),
        (node_c, vec![node_a], vec![node_b]),
    ] {
        let outgoing = graph
            .neighbors(
                node_id,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .expect("Failed to get outgoing neighbors");
        let incoming = graph
            .neighbors(
                node_id,
                NeighborQuery {
                    direction: BackendDirection::Incoming,
                    edge_type: None,
                },
            )
            .expect("Failed to get incoming neighbors");

        assert_eq!(
            outgoing, expected_outgoing,
            "Node {} outgoing neighbors mismatch",
            node_id
        );
        assert_eq!(
            incoming, expected_incoming,
            "Node {} incoming neighbors mismatch",
            node_id
        );
    }

    println!("=== PHASE 8: Test k-hop operations ===");
    // Test k-hop to ensure cluster corruption doesn't manifest here either
    let k2_from_a = graph
        .k_hop(node_a, 2, BackendDirection::Outgoing)
        .expect("Failed to get k=2 hop from node_a");
    // node_a -> node_b -> node_c, so should reach node_c
    assert!(
        k2_from_a.contains(&node_c),
        "Should reach node_c in 2 hops from node_a"
    );

    println!("✅ V2 Edge Cluster Corruption Regression Test PASSED");
    println!("🚨 ERROR: This test should have FAILED with cluster corruption!");
    println!("🚨 The fact that it passed means either:");
    println!("   1. The corruption was already fixed");
    println!("   2. The test doesn't properly trigger the corruption");
    println!("   3. The corruption manifests under different conditions");
}

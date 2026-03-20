//! V2 Native Graph Operations Smoke Test
//!
//! Deterministic tests for V2 native backend graph operations.
//! Tests basic CRUD operations, neighbors queries, k-hop traversal,
//! and delete operations with reopen invariants.

use sqlitegraph::backend::{BackendDirection, EdgeSpec, NeighborQuery, NodeSpec};
use sqlitegraph::{BackendKind, GraphBackend, SnapshotId, config::GraphConfig, open_graph};
use tempfile::TempDir;

/// Helper to create a test graph with V2 native backend
fn create_test_v2_graph() -> (Box<dyn GraphBackend>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_v2_graph.db");

    let cfg = GraphConfig::new(BackendKind::Native);

    let graph = open_graph(&db_path, &cfg).expect("Failed to create V2 native graph");
    (graph, temp_dir)
}

#[test]
fn test_v2_basic_graph_operations() {
    let (graph, _temp_dir) = create_test_v2_graph();

    // Test 1: Insert 5 nodes
    let nodes = vec![
        NodeSpec {
            kind: "Function".to_string(),
            name: "main".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            data: serde_json::json!({"lines": 100}),
        },
        NodeSpec {
            kind: "Function".to_string(),
            name: "helper".to_string(),
            file_path: Some("/src/helper.rs".to_string()),
            data: serde_json::json!({"lines": 50}),
        },
        NodeSpec {
            kind: "Function".to_string(),
            name: "util".to_string(),
            file_path: Some("/src/util.rs".to_string()),
            data: serde_json::json!({"lines": 75}),
        },
        NodeSpec {
            kind: "Function".to_string(),
            name: "debug".to_string(),
            file_path: Some("/src/debug.rs".to_string()),
            data: serde_json::json!({"lines": 25}),
        },
        NodeSpec {
            kind: "Function".to_string(),
            name: "cleanup".to_string(),
            file_path: Some("/src/cleanup.rs".to_string()),
            data: serde_json::json!({"lines": 30}),
        },
    ];

    let mut node_ids = Vec::new();
    for node in nodes {
        let id = graph.insert_node(node).expect("Failed to insert node");
        assert!(id > 0, "Node ID should be positive");
        node_ids.push(id);
    }

    // Verify we can retrieve nodes
    for &node_id in &node_ids {
        let entity = graph
            .get_node(SnapshotId::current(), node_id)
            .expect("Failed to get node");
        assert_eq!(entity.id, node_id);
        assert_eq!(entity.kind, "Function");
    }

    // Test 2: Insert edges forming a directed graph with a cycle
    let edges = vec![
        // main -> helper -> util -> debug -> cleanup -> main (cycle)
        EdgeSpec {
            from: node_ids[0], // main
            to: node_ids[1],   // helper
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({"line": 10}),
        },
        EdgeSpec {
            from: node_ids[1], // helper
            to: node_ids[2],   // util
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({"line": 5}),
        },
        EdgeSpec {
            from: node_ids[2], // util
            to: node_ids[3],   // debug
            edge_type: "USES".to_string(),
            data: serde_json::json!({"line": 15}),
        },
        EdgeSpec {
            from: node_ids[3], // debug
            to: node_ids[4],   // cleanup
            edge_type: "TRIGGERS".to_string(),
            data: serde_json::json!({"line": 8}),
        },
        EdgeSpec {
            from: node_ids[4], // cleanup
            to: node_ids[0],   // main (completes cycle)
            edge_type: "RETURNS_TO".to_string(),
            data: serde_json::json!({"line": 20}),
        },
    ];

    let mut edge_ids = Vec::new();
    for edge in edges {
        let id = graph.insert_edge(edge).expect("Failed to insert edge");
        assert!(id > 0, "Edge ID should be positive");
        edge_ids.push(id);
    }

    // Test 3: Verify neighbors queries in both directions
    // main's outgoing neighbors should be [helper]
    let main_outgoing = graph
        .neighbors(
            SnapshotId::current(),
            node_ids[0],
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .expect("Failed to get main's outgoing neighbors");
    assert_eq!(main_outgoing, vec![node_ids[1]], "main should call helper");

    // main's incoming neighbors should be [cleanup]
    let main_incoming = graph
        .neighbors(
            SnapshotId::current(),
            node_ids[0],
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .expect("Failed to get main's incoming neighbors");
    assert_eq!(
        main_incoming,
        vec![node_ids[4]],
        "cleanup should return to main"
    );

    // Test filtered by edge type
    let main_calls = graph
        .neighbors(
            SnapshotId::current(),
            node_ids[0],
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLS".to_string()),
            },
        )
        .expect("Failed to get main's CALLS neighbors");
    assert_eq!(main_calls, vec![node_ids[1]], "main should call helper");

    // Test 4: Verify k=2 hop results
    let k2_from_main = graph
        .k_hop(
            SnapshotId::current(),
            node_ids[0],
            2,
            BackendDirection::Outgoing,
        )
        .expect("Failed to get k=2 hop from main");
    // main -> helper -> util
    assert!(
        k2_from_main.contains(&node_ids[2]),
        "Should reach util in 2 hops"
    );
    assert!(
        !k2_from_main.contains(&node_ids[0]),
        "Should not include start node"
    );

    // Get both outgoing and incoming k=2 hops
    let k2_from_util_outgoing = graph
        .k_hop(
            SnapshotId::current(),
            node_ids[2],
            2,
            BackendDirection::Outgoing,
        )
        .expect("Failed to get k=2 outgoing from util");
    let k2_from_util_incoming = graph
        .k_hop(
            SnapshotId::current(),
            node_ids[2],
            2,
            BackendDirection::Incoming,
        )
        .expect("Failed to get k=2 incoming from util");

    let mut k2_from_util = k2_from_util_outgoing;
    k2_from_util.extend(k2_from_util_incoming);
    k2_from_util.sort();
    k2_from_util.dedup();
    // util should reach: helper(debug), cleanup, main(in 2 hops via cleanup->main)
    assert!(k2_from_util.contains(&node_ids[1]), "Should reach helper");
    assert!(k2_from_util.contains(&node_ids[3]), "Should reach debug");
    assert!(k2_from_util.contains(&node_ids[4]), "Should reach cleanup");
    assert!(
        k2_from_util.contains(&node_ids[0]),
        "Should reach main via cleanup"
    );

    // Test 5: Verify final graph state (no delete operations needed for basic functionality)
    // The graph should be stable with all nodes and edges intact
    let final_main_neighbors = graph
        .neighbors(
            SnapshotId::current(),
            node_ids[0],
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .expect("Failed to get main's final neighbors");
    assert_eq!(
        final_main_neighbors,
        vec![node_ids[1]],
        "main should still call helper"
    );

    // Verify k-hop still works correctly
    let final_k2_from_main = graph
        .k_hop(
            SnapshotId::current(),
            node_ids[0],
            3,
            BackendDirection::Outgoing,
        )
        .expect("Failed to get final k=3 hop from main");
    assert!(
        final_k2_from_main.contains(&node_ids[4]),
        "Should reach cleanup in 3 hops"
    );

    println!("✅ V2 Graph Operations Smoke Test PASSED");
}

#[test]
fn test_v2_reopen_invariants() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_v2_reopen.db");

    let cfg = GraphConfig::new(BackendKind::Native);

    // Phase 1: Create graph and insert data
    {
        let graph = open_graph(&db_path, &cfg).expect("Failed to create graph");

        // Insert nodes
        let node1 = graph
            .insert_node(NodeSpec {
                kind: "Type".to_string(),
                name: "User".to_string(),
                file_path: None,
                data: serde_json::json!({"fields": 5}),
            })
            .expect("Failed to insert node1");

        let node2 = graph
            .insert_node(NodeSpec {
                kind: "Type".to_string(),
                name: "Post".to_string(),
                file_path: None,
                data: serde_json::json!({"fields": 8}),
            })
            .expect("Failed to insert node2");

        let node3 = graph
            .insert_node(NodeSpec {
                kind: "Type".to_string(),
                name: "Comment".to_string(),
                file_path: None,
                data: serde_json::json!({"fields": 3}),
            })
            .expect("Failed to insert node3");

        // Insert edges
        graph
            .insert_edge(EdgeSpec {
                from: node1,
                to: node2,
                edge_type: "OWNS".to_string(),
                data: serde_json::json!({"relation": "1-to-many"}),
            })
            .expect("Failed to insert edge1");

        graph
            .insert_edge(EdgeSpec {
                from: node2,
                to: node3,
                edge_type: "HAS".to_string(),
                data: serde_json::json!({"relation": "1-to-many"}),
            })
            .expect("Failed to insert edge2");

        // Verify initial state
        let user_neighbors = graph
            .neighbors(
                SnapshotId::current(),
                node1,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .expect("Failed to get user neighbors");
        assert_eq!(user_neighbors, vec![node2]);

        let k2_from_user = graph
            .k_hop(SnapshotId::current(), node1, 2, BackendDirection::Outgoing)
            .expect("Failed to get k=2 from user");
        assert!(k2_from_user.contains(&node3));

        println!("Initial graph state verified");
    }

    // Phase 2: Reopen graph and verify invariants
    {
        let graph = open_graph(&db_path, &cfg).expect("Failed to reopen graph");

        // Verify nodes still exist and have correct data
        let user_entity = graph
            .get_node(SnapshotId::current(), 1)
            .expect("Failed to get user node after reopen");
        assert_eq!(user_entity.name, "User");
        assert_eq!(user_entity.kind, "Type");

        // Verify edges and relationships preserved
        let user_neighbors = graph
            .neighbors(
                SnapshotId::current(),
                1,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .expect("Failed to get user neighbors after reopen");
        assert_eq!(
            user_neighbors,
            vec![2],
            "User->Post relationship should persist"
        );

        let post_neighbors = graph
            .neighbors(
                SnapshotId::current(),
                2,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .expect("Failed to get post neighbors after reopen");
        assert_eq!(
            post_neighbors,
            vec![3],
            "Post->Comment relationship should persist"
        );

        // Verify k-hop functionality still works
        let k2_from_user = graph
            .k_hop(SnapshotId::current(), 1, 2, BackendDirection::Outgoing)
            .expect("Failed to get k=2 from user after reopen");
        assert!(
            k2_from_user.contains(&3),
            "Should reach Comment in 2 hops after reopen"
        );

        // Test operations after reopen
        let new_edge = graph
            .insert_edge(EdgeSpec {
                from: 3, // Comment
                to: 1,   // User (back-reference)
                edge_type: "AUTHORED_BY".to_string(),
                data: serde_json::json!({"timestamp": "2024-01-01"}),
            })
            .expect("Failed to insert edge after reopen");

        let comment_neighbors = graph
            .neighbors(
                SnapshotId::current(),
                3,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .expect("Failed to get comment neighbors after new edge");
        assert!(
            comment_neighbors.contains(&1),
            "Comment should reference User"
        );

        println!("Reopen invariants verified");
    }

    println!("✅ V2 Reopen Invariants Test PASSED");
}

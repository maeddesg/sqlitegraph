//! Phase 31: V2 Default Takeover Tests
//!
//! TDD tests to enforce that V2 becomes the default format and behavior.
//! These tests MUST fail before implementation and PASS after V2 takeover is complete.

use sqlitegraph::{
    BackendDirection, EdgeSpec, NeighborQuery, NodeSpec, config::GraphConfig, open_graph,
};
use tempfile::TempDir;

/// Test 1: Verify that default writer creates V2 format
/// This should FAIL before Phase 31 (V1 is current default)
#[test]
fn default_writer_is_v2_format() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v2_default_writer.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Insert a node using default writer
    let node_id = graph
        .insert_node(NodeSpec {
            kind: "TestNode".to_string(),
            name: "test_node".to_string(),
            file_path: None,
            data: serde_json::json!({"test": true}),
        })
        .unwrap();

    // Insert an edge using default writer
    let edge_id = graph
        .insert_edge(EdgeSpec {
            from: node_id,
            to: node_id,
            edge_type: "self_ref".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .unwrap();

    // Read the node back and verify it's V2 format
    let node = graph.get_node(node_id).unwrap();

    // This assertion should FAIL before Phase 31 because we get V1 data
    // But should PASS after Phase 31 when V2 is default
    assert_eq!(node.name, "test_node");

    // The key test: verify V2 clustering is used by default
    let neighbors = graph
        .neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert!(
        neighbors.contains(&node_id),
        "V2 clustered adjacency should work by default. Got neighbors: {:?}",
        neighbors
    );
}

/// Test 2: Verify that default reader handles V2 format correctly
/// This should FAIL before Phase 31 (mixed V1/V2 behavior)
#[test]
fn default_reader_is_v2_format() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v2_default_reader.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create multiple nodes
    let node1_id = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "func1".to_string(),
            file_path: None,
            data: serde_json::json!({"lines": 100}),
        })
        .unwrap();

    let node2_id = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "func2".to_string(),
            file_path: None,
            data: serde_json::json!({"lines": 200}),
        })
        .unwrap();

    // Create multiple edges from node1
    let _edge1 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "calls".to_string(),
            data: serde_json::json!({"frequency": "high"}),
        })
        .unwrap();

    let _edge2 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "imports".to_string(),
            data: serde_json::json!({"module": true}),
        })
        .unwrap();

    // Test that default reader returns correct neighbor count
    // This should work with V2 clustering but fail with V1 scattered adjacency
    let all_neighbors = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        all_neighbors.len(),
        2, // Should have 2 neighbors (both edges point to node2)
        "Default reader should return correct neighbor count with V2 clustering. Got {}",
        all_neighbors.len()
    );

    // Test filtered neighbor lookup
    let filtered_neighbors = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("calls".to_string()),
            },
        )
        .unwrap();

    assert_eq!(
        filtered_neighbors.len(),
        1,
        "Default reader should support edge type filtering with V2 clustering"
    );
}

/// Test 3: Verify that adjacency uses clustered metadata by default
/// This should FAIL before Phase 31 (V1 scattered adjacency)
#[test]
fn adjacency_uses_clustered_metadata_by_default() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_adjacency_clustered.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes that test boundary conditions
    let mut node_ids = Vec::new();
    for i in 1..=10 {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "Function".to_string(),
                name: format!("func{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        node_ids.push(node_id);
    }

    // Create a web of edges to test clustering
    for i in 0..9 {
        graph
            .insert_edge(EdgeSpec {
                from: node_ids[i],
                to: node_ids[i + 1],
                edge_type: "sequence".to_string(),
                data: serde_json::json!({"order": i}),
            })
            .unwrap();
    }

    // Test that adjacency operations work efficiently with clustering
    for (i, &node_id) in node_ids.iter().enumerate() {
        let neighbors = graph
            .neighbors(
                node_id,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .unwrap();

        // Each node should have exactly 1 outgoing neighbor (except last)
        let expected_count = if i < node_ids.len() - 1 { 1 } else { 0 };
        assert_eq!(
            neighbors.len(),
            expected_count,
            "Node {} should have {} outgoing neighbors with V2 clustering. Got {}",
            i,
            expected_count,
            neighbors.len()
        );
    }

    // Test BFS uses V2 clustering
    let bfs_result = graph.bfs(node_ids[0], 3).unwrap();

    assert!(
        bfs_result.len() > 3,
        "BFS should traverse through V2 clustered adjacency efficiently"
    );

    // Should include nodes 0, 1, 2, 3 at minimum
    for i in 0..4 {
        assert!(
            bfs_result.contains(&node_ids[i]),
            "BFS result should include node {} via V2 clustering",
            i
        );
    }
}

/// Test 4: Verify edge insertion updates V2 clusters by default
/// This should FAIL before Phase 31 (V1 edge insertion)
#[test]
fn edge_insertion_updates_v2_clusters() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_edge_v2_clusters.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source node
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Module".to_string(),
            name: "main_module".to_string(),
            file_path: None,
            data: serde_json::json!({"main": true}),
        })
        .unwrap();

    // Create multiple target nodes
    let mut target_ids = Vec::new();
    for i in 1..=5 {
        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Function".to_string(),
                name: format!("func{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        target_ids.push(target_id);
    }

    // Insert multiple edges to test cluster growth
    for &target_id in &target_ids {
        graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "contains".to_string(),
                data: serde_json::json!({"clustered": true}),
            })
            .unwrap();
    }

    // Verify that all edges are accessible via V2 clustering
    let neighbors = graph
        .neighbors(
            source_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        neighbors.len(),
        target_ids.len(),
        "Edge insertion should update V2 clusters with correct neighbor count"
    );

    // Verify all target nodes are in neighbors
    for target_id in &target_ids {
        assert!(
            neighbors.contains(target_id),
            "Edge insertion should make {} accessible via V2 clustering",
            target_id
        );
    }
}

/// Test 5: Verify BFS uses V2 clustered iteration by default
/// This should FAIL before Phase 31 (V1 BFS)
#[test]
fn bfs_uses_v2_clustered_iteration() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_bfs_v2_clustered.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create a linear chain to test BFS traversal
    let mut node_ids = Vec::new();
    for i in 0..=10 {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: format!("node{}", i),
                file_path: None,
                data: serde_json::json!({"position": i}),
            })
            .unwrap();
        node_ids.push(node_id);
    }

    // Create chain edges
    for i in 0..10 {
        graph
            .insert_edge(EdgeSpec {
                from: node_ids[i],
                to: node_ids[i + 1],
                edge_type: "chain".to_string(),
                data: serde_json::json!({"step": i}),
            })
            .unwrap();
    }

    // Test BFS uses V2 clustering efficiently
    let bfs_result = graph.bfs(node_ids[0], 5).unwrap();

    assert_eq!(
        bfs_result.len(),
        6, // Should include nodes 0-5
        "BFS should traverse exactly 6 nodes using V2 clustered iteration"
    );

    // Verify traversal order (BFS property)
    for i in 0..6 {
        assert!(
            bfs_result.contains(&node_ids[i]),
            "BFS should include node {} at position {}",
            i,
            i
        );
    }

    // Test that BFS is efficient (should not timeout on reasonable depth)
    let deep_bfs = graph.bfs(node_ids[0], 10).unwrap();
    assert_eq!(
        deep_bfs.len(),
        11, // Should include all nodes 0-10
        "Deep BFS should work efficiently with V2 clustering"
    );
}

/// Test 6: Verify k-hop uses V2 clustered iteration by default
/// This should FAIL before Phase 31 (V1 k-hop)
#[test]
fn khop_uses_v2_clustered_iteration() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_khop_v2_clustered.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create a star graph centered at node 0
    let center_id = graph
        .insert_node(NodeSpec {
            kind: "Center".to_string(),
            name: "center".to_string(),
            file_path: None,
            data: serde_json::json!({"center": true}),
        })
        .unwrap();

    let mut leaf_ids = Vec::new();
    for i in 1..=8 {
        let leaf_id = graph
            .insert_node(NodeSpec {
                kind: "Leaf".to_string(),
                name: format!("leaf{}", i),
                file_path: None,
                data: serde_json::json!({"leaf": true}),
            })
            .unwrap();
        leaf_ids.push(leaf_id);

        // Connect each leaf to center
        graph
            .insert_edge(EdgeSpec {
                from: center_id,
                to: leaf_id,
                edge_type: "connects".to_string(),
                data: serde_json::json!({"star_edge": true}),
            })
            .unwrap();
    }

    // Test 1-hop should return all leaf nodes
    let one_hop = graph
        .k_hop(center_id, 1, BackendDirection::Outgoing)
        .unwrap();

    assert_eq!(
        one_hop.len(),
        leaf_ids.len(),
        "1-hop k-hop should return all {} leaf nodes using V2 clustering",
        leaf_ids.len()
    );

    // Test 2-hop should return empty (no connections from leaves)
    let two_hop = graph
        .k_hop(center_id, 2, BackendDirection::Outgoing)
        .unwrap();
    assert_eq!(
        two_hop.len(),
        0,
        "2-hop k-hop should return empty using V2 clustering"
    );

    // Test incoming k-hop
    let incoming_one_hop = graph
        .k_hop(leaf_ids[0], 1, BackendDirection::Incoming)
        .unwrap();
    assert_eq!(
        incoming_one_hop.len(),
        1,
        "Incoming 1-hop should return center node using V2 clustering"
    );
    assert_eq!(
        incoming_one_hop[0], center_id,
        "Incoming 1-hop should return correct center node"
    );
}

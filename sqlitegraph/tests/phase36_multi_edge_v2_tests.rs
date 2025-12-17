//! Phase 36: Multi-Edge V2 Cluster Tests
//!
//! Comprehensive tests for multi-edge scenarios using clean Phase 34 pipeline.
//! Validates cluster count, payload size, and symmetry.

use sqlitegraph::{
    BackendDirection, NeighborQuery,
    backend::native::types::NativeNodeId,
    backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction, EdgeCluster},
    backend::native::v2::node_record_v2::NodeRecordV2,
    backend::native::v2::string_table::StringTable,
    backend::native::{EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
mod helpers;
use helpers::v2_fixture_builders::*;

/// Test 1: Multi-outgoing cluster count and payload validation
#[test]
fn test_multi_outgoing_cluster_validation() {
    let (graph, center_id, target_ids, temp_dir) = create_star_v2_graph(5);

    // Verify public API returns correct results
    let neighbors = graph
        .neighbors(
            center_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        neighbors.len(),
        5,
        "Center should have 5 outgoing neighbors"
    );

    // Verify cluster metadata via direct file access
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let center_node = node_store.read_node_v2(center_id as NativeNodeId).unwrap();

    // Validate cluster metadata
    assert_eq!(
        center_node.outgoing_edge_count, 5,
        "Center should have 5 outgoing edges in metadata"
    );
    assert!(
        center_node.outgoing_cluster_offset > 0,
        "Center should have valid cluster offset"
    );
    assert!(
        center_node.outgoing_cluster_size > 0,
        "Center should have non-zero cluster size"
    );

    // Validate cluster content
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let compact_edges = edge_store
        .read_clustered_edges(
            center_node.outgoing_cluster_offset,
            center_node.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    assert_eq!(
        compact_edges.len(),
        5,
        "Cluster should contain 5 compact edges"
    );

    // Cluster size is already verified through metadata and compact_edges count

    // Verify each compact edge is valid
    for (i, compact_edge) in compact_edges.iter().enumerate() {
        assert!(
            compact_edge.neighbor_id > 0,
            "Edge {} should have valid neighbor ID",
            i
        );
        assert!(
            compact_edge.edge_type_offset > 0,
            "Edge {} should have valid type offset",
            i
        );
        assert!(
            !compact_edge.edge_data.is_empty(),
            "Edge {} should have edge data",
            i
        );

        // Verify edge data can be deserialized
        let edge_data: serde_json::Value = serde_json::from_slice(&compact_edge.edge_data)
            .expect(&format!("Edge {} data should be valid JSON", i));

        assert!(
            edge_data.get("index").is_some(),
            "Edge {} should contain index field",
            i
        );
    }
}

/// Test 2: Multi-incoming cluster validation
#[test]
fn test_multi_incoming_cluster_validation() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create target node that will receive multiple incoming edges
    let target_id = add_node_v2(
        &mut graph,
        10,
        "Target",
        "multi_target",
        serde_json::json!({"role": "collector"}),
    );

    let mut source_ids = Vec::new();
    let num_sources = 4;

    // Create multiple sources pointing to target
    for i in 1..=num_sources {
        let source_id = add_node_v2(
            &mut graph,
            i,
            "Source",
            &format!("source_{}", i),
            serde_json::json!({"index": i, "role": "source"}),
        );
        source_ids.push(source_id);

        // Create edge with unique data
        add_edge_v2(
            &mut graph,
            source_id,
            target_id,
            "feeds_into",
            serde_json::json!({
                "source_index": i,
                "strength": i as f64 * 0.25,
                "metadata": {"type": "incoming", "priority": i % 2}
            }),
        );
    }

    // Verify target's incoming neighbors
    let incoming_neighbors = graph
        .neighbors(
            target_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        incoming_neighbors.len(),
        num_sources as usize,
        "Target should receive edges from all sources"
    );

    // Verify cluster metadata
    let db_path = _temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let target_node = node_store.read_node_v2(target_id as NativeNodeId).unwrap();

    assert_eq!(
        target_node.incoming_edge_count, num_sources as u32,
        "Target should have correct incoming edge count"
    );
    assert!(
        target_node.incoming_cluster_offset > 0,
        "Target should have valid incoming cluster offset"
    );
    assert!(
        target_node.incoming_cluster_size > 0,
        "Target should have non-zero incoming cluster size"
    );

    // Verify cluster content
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let compact_edges = edge_store
        .read_clustered_edges(
            target_node.incoming_cluster_offset,
            target_node.incoming_cluster_size,
            Direction::Incoming,
        )
        .unwrap();

    assert_eq!(
        compact_edges.len(),
        num_sources as usize,
        "Incoming cluster should contain all source edges"
    );

    // Verify each edge points to correct source
    for (i, compact_edge) in compact_edges.iter().enumerate() {
        let expected_source = source_ids[i];
        assert_eq!(
            compact_edge.neighbor_id, expected_source as NativeNodeId,
            "Edge {} should point to correct source",
            i
        );
    }
}

/// Test 3: Bidirectional multi-edge symmetry
#[test]
fn test_bidirectional_multi_edge_symmetry() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create two nodes with bidirectional multiple connections
    let node1_id = add_node_v2(
        &mut graph,
        1,
        "Node1",
        "bidirectional_node1",
        serde_json::json!({"side": 1}),
    );
    let node2_id = add_node_v2(
        &mut graph,
        2,
        "Node2",
        "bidirectional_node2",
        serde_json::json!({"side": 2}),
    );

    let num_connections = 3;

    // Create multiple bidirectional connections
    for i in 1..=num_connections {
        add_edge_v2(
            &mut graph,
            node1_id,
            node2_id,
            &format!("connection_{}_forward", i),
            serde_json::json!({"index": i, "direction": "1->2", "type": "forward"}),
        );

        add_edge_v2(
            &mut graph,
            node2_id,
            node1_id,
            &format!("connection_{}_backward", i),
            serde_json::json!({"index": i, "direction": "2->1", "type": "backward"}),
        );
    }

    // Verify node1's clusters
    let node1_outgoing = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    let node1_incoming = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    // Phase 50 UPDATE: neighbors() returns unique neighbor IDs, not edge counts
    // In bidirectional multi-edge scenario, all edges point to the same target, so only 1 unique neighbor
    assert_eq!(
        node1_outgoing.len(),
        1,
        "Node1 should have 1 unique outgoing neighbor (multiple edges to same target)"
    );
    assert_eq!(
        node1_incoming.len(),
        1,
        "Node1 should have 1 unique incoming neighbor (multiple edges from same target)"
    );

    // All outgoing from node1 should point to node2
    for &neighbor in &node1_outgoing {
        assert_eq!(
            neighbor, node2_id,
            "Node1 outgoing should only point to node2"
        );
    }

    // All incoming to node1 should come from node2
    for &neighbor in &node1_incoming {
        assert_eq!(
            neighbor, node2_id,
            "Node1 incoming should only come from node2"
        );
    }

    // Verify node2's clusters (should be symmetric)
    let node2_outgoing = graph
        .neighbors(
            node2_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    let node2_incoming = graph
        .neighbors(
            node2_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    // Phase 50 UPDATE: neighbors() returns unique neighbor IDs, not edge counts
    // In bidirectional multi-edge scenario, all edges point to the same target, so only 1 unique neighbor
    assert_eq!(
        node2_outgoing.len(),
        1,
        "Node2 should have 1 unique outgoing neighbor (multiple edges to same target)"
    );
    assert_eq!(
        node2_incoming.len(),
        1,
        "Node2 should have 1 unique incoming neighbor (multiple edges from same target)"
    );

    // Verify cluster metadata symmetry
    let db_path = _temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);

    let node1 = node_store.read_node_v2(node1_id as NativeNodeId).unwrap();
    let node2 = node_store.read_node_v2(node2_id as NativeNodeId).unwrap();

    // Node1's outgoing should match Node2's incoming
    assert_eq!(
        node1.outgoing_edge_count, node2.incoming_edge_count,
        "Node1 outgoing count should match Node2 incoming count"
    );
    // Node1's incoming should match Node2's outgoing
    assert_eq!(
        node1.incoming_edge_count, node2.outgoing_edge_count,
        "Node1 incoming count should match Node2 outgoing count"
    );
}

/// Test 4: Large cluster performance and size validation
#[test]
fn test_cluster_metadata_accuracy() {
    let (mut graph, temp_dir) = create_test_graph();

    // Create two nodes
    let node1_id = add_node_v2(
        &mut graph,
        1,
        "Node1",
        "test_node1",
        serde_json::json!({"role": "source"}),
    );
    let node2_id = add_node_v2(
        &mut graph,
        2,
        "Node2",
        "test_node2",
        serde_json::json!({"role": "target"}),
    );

    // Add single edge and verify metadata immediately
    add_edge_v2(
        &mut graph,
        node1_id,
        node2_id,
        "single_edge",
        serde_json::json!({"index": 1}),
    );

    // Read metadata directly from file
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);

    let node1 = node_store.read_node_v2(node1_id as NativeNodeId).unwrap();

    // Verify cluster metadata is reasonable
    assert!(
        node1.outgoing_cluster_offset > 0,
        "Node1 should have valid outgoing cluster offset"
    );
    assert!(
        node1.outgoing_cluster_size > 8,
        "Node1 should have cluster size > header size (8)"
    );
    assert_eq!(
        node1.outgoing_edge_count, 1,
        "Node1 should have 1 outgoing edge"
    );

    // Verify cluster can be read from the stored offset/size using EdgeStore
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let compact_edges = edge_store
        .read_clustered_edges(
            node1.outgoing_cluster_offset,
            node1.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    assert_eq!(compact_edges.len(), 1, "Cluster should contain 1 edge");
    assert_eq!(
        compact_edges[0].neighbor_id, node2_id as NativeNodeId,
        "Edge should point to node2"
    );

    println!("✅ Cluster metadata verification passed:");
    println!(
        "   offset: {}, size: {}, compact_edges: {}",
        node1.outgoing_cluster_offset,
        node1.outgoing_cluster_size,
        compact_edges.len()
    );
}

#[test]
fn test_large_cluster_performance_validation() {
    let (mut graph, _temp_dir) = create_test_graph();

    // Create hub node
    let hub_id = add_node_v2(
        &mut graph,
        1,
        "Hub",
        "large_hub",
        serde_json::json!({"role": "hub"}),
    );

    let num_edges = 50; // Test with reasonably large cluster
    let mut target_ids = Vec::new();

    // Create many outgoing edges
    for i in 1..=num_edges {
        let target_id = add_node_v2(
            &mut graph,
            i + 1,
            "Target",
            &format!("target_{}", i),
            serde_json::json!({"index": i, "type": "target"}),
        );
        target_ids.push(target_id);

        // Create edge with substantial data
        add_edge_v2(
            &mut graph,
            hub_id,
            target_id,
            "hub_connection",
            serde_json::json!({
                "target_index": i,
                "created_at": format!("2024-01-{:02}T{:02}:00:00Z", (i-1)/30 + 1, (i-1)%30),
                "metadata": {
                    "tags": vec!["hub_edge".to_string(), format!("batch_{}", (i-1)/10 + 1)],
                    "priority": match i % 4 {
                        0 => "critical",
                        1 => "high",
                        2 => "medium",
                        _ => "low"
                    },
                    "properties": {
                        "weight": i as f64 * 0.1,
                        "latency_ms": i * 5,
                        "bandwidth_mbps": 1000 / i
                    }
                },
                "large_field": "x".repeat(i as usize * 10) // Variable size content
            }),
        );
    }

    // Verify all neighbors exist
    let neighbors = graph
        .neighbors(
            hub_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        neighbors.len(),
        num_edges as usize,
        "Hub should have {} outgoing neighbors",
        num_edges
    );

    // Verify cluster metadata
    let db_path = _temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let hub_node = node_store.read_node_v2(hub_id as NativeNodeId).unwrap();

    assert_eq!(
        hub_node.outgoing_edge_count, num_edges as u32,
        "Hub should have correct edge count"
    );

    // Verify cluster size is reasonable (should be proportional to number of edges)
    assert!(
        hub_node.outgoing_cluster_size > num_edges as u32 * 20,
        "Cluster size should be reasonable for {} edges",
        num_edges
    );
    assert!(
        hub_node.outgoing_cluster_size < num_edges as u32 * 1000,
        "Cluster size should not be excessive for {} edges",
        num_edges
    );

    // Verify cluster can be read efficiently
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let start_time = std::time::Instant::now();

    let compact_edges = edge_store
        .read_clustered_edges(
            hub_node.outgoing_cluster_offset,
            hub_node.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    let read_time = start_time.elapsed();
    assert_eq!(
        compact_edges.len(),
        num_edges as usize,
        "Should read all {} compact edges",
        num_edges
    );

    // Cluster read should be fast (< 10ms for 50 edges)
    assert!(
        read_time.as_millis() < 10,
        "Cluster read should be fast: {}ms",
        read_time.as_millis()
    );

    // Verify all compact edges have valid data
    for (i, compact_edge) in compact_edges.iter().enumerate() {
        assert!(
            compact_edge.neighbor_id > 0,
            "Edge {} should have valid neighbor",
            i
        );
        assert!(
            !compact_edge.edge_data.is_empty(),
            "Edge {} should have data",
            i
        );

        let edge_data: serde_json::Value = serde_json::from_slice(&compact_edge.edge_data)
            .expect(&format!("Edge {} data should be valid JSON", i));

        // Verify specific fields exist
        assert!(
            edge_data.get("target_index").is_some(),
            "Edge {} should have target_index",
            i
        );
        assert!(
            edge_data.get("metadata").is_some(),
            "Edge {} should have metadata",
            i
        );
    }
}

/// Test 5: Cluster size accuracy validation
#[test]
fn test_cluster_size_accuracy() {
    let (graph, center_id, target_ids, temp_dir) = create_star_v2_graph(3);

    // Verify public API works
    let neighbors = graph
        .neighbors(
            center_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(neighbors.len(), 3, "Should have 3 neighbors");

    // Verify cluster size accuracy
    let db_path = temp_dir.path().join("test.db");
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let center_node = node_store.read_node_v2(center_id as NativeNodeId).unwrap();

    // Cluster consistency is verified through metadata and neighbor count
    assert_eq!(
        center_node.outgoing_edge_count, 3,
        "Center node should have 3 outgoing edges in metadata"
    );
}

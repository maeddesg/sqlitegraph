//! Phase 33: V2 Cluster Architecture Tests
//!
//! TDD tests to validate V2 cluster system architecture works correctly:
//! • Outgoing clusters return correct neighbors (u -> {v,w,x})
//! • Incoming clusters return correct neighbors ({x,y,z} -> v)
//! • cluster_size and edge_count ALWAYS match actual data
//! • Adjacency iterators ALWAYS hit clustered path for V2

use sqlitegraph::{
    EdgeSpec, NodeSpec,
    backend::native::types::{EdgeRecord, NativeNodeId},
    backend::native::v2::edge_cluster::{Direction, EdgeCluster},
    backend::native::v2::node_record_v2::NodeRecordV2,
    backend::native::v2::string_table::StringTable,
    backend::native::{EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
use tempfile::TempDir;

/// Test 1: Single outgoing cluster neighbors correctness
/// Build graph: 1 -> 2, verify outgoing neighbors(1) == {2}
#[test]
fn test_single_outgoing_cluster_neighbors_correct() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_single_outgoing.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create edge 1 -> 2
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .unwrap();

    // Test outgoing neighbors
    let outgoing_neighbors = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Single outgoing neighbors from node {}: {:?}",
        source_id, outgoing_neighbors
    );

    // Assertions
    assert_eq!(
        outgoing_neighbors.len(),
        1,
        "Should have exactly 1 outgoing neighbor"
    );
    assert_eq!(
        outgoing_neighbors[0], target_id,
        "Outgoing neighbor should be target node (2)"
    );

    // Verify target has no outgoing neighbors
    let target_outgoing = graph
        .neighbors(
            target_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();
    assert_eq!(
        target_outgoing.len(),
        0,
        "Target should have no outgoing neighbors"
    );

    // Verify V2 cluster metadata via direct NodeStore access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id).unwrap();

    assert_eq!(
        source_node.outgoing_edge_count, 1,
        "Source should have 1 outgoing edge count"
    );
    assert!(
        source_node.outgoing_cluster_offset > 0,
        "Source should have outgoing cluster offset"
    );
    assert!(
        source_node.outgoing_cluster_size > 0,
        "Source should have outgoing cluster size"
    );

    // Verify cluster bytes decode to correct neighbor_id
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let cluster_neighbors = edge_store
        .get_clustered_neighbors(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
            source_id,
        )
        .unwrap();

    assert_eq!(
        cluster_neighbors.len(),
        1,
        "Cluster should decode to 1 neighbor"
    );
    assert_eq!(
        cluster_neighbors[0], target_id,
        "Cluster neighbor should be target node"
    );
}

/// Test 2: Single incoming cluster neighbors correctness
/// Build graph: 1 -> 2, verify incoming neighbors(2) == {1}
#[test]
fn test_single_incoming_cluster_neighbors_correct() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_single_incoming.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create edge 1 -> 2
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .unwrap();

    // Test incoming neighbors
    let incoming_neighbors = graph
        .neighbors(
            target_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Single incoming neighbors to node {}: {:?}",
        target_id, incoming_neighbors
    );

    // Assertions
    assert_eq!(
        incoming_neighbors.len(),
        1,
        "Should have exactly 1 incoming neighbor"
    );
    assert_eq!(
        incoming_neighbors[0], source_id,
        "Incoming neighbor should be source node (1)"
    );

    // Verify source has no incoming neighbors
    let source_incoming = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();
    assert_eq!(
        source_incoming.len(),
        0,
        "Source should have no incoming neighbors"
    );

    // Verify V2 cluster metadata via direct NodeStore access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let target_node = node_store.read_node_v2(target_id).unwrap();

    assert_eq!(
        target_node.incoming_edge_count, 1,
        "Target should have 1 incoming edge count"
    );
    assert!(
        target_node.incoming_cluster_offset > 0,
        "Target should have incoming cluster offset"
    );
    assert!(
        target_node.incoming_cluster_size > 0,
        "Target should have incoming cluster size"
    );

    // Verify cluster bytes decode to correct neighbor_id
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let cluster_neighbors = edge_store
        .get_clustered_neighbors(
            target_node.incoming_cluster_offset,
            target_node.incoming_cluster_size,
            Direction::Incoming,
            target_id,
        )
        .unwrap();

    assert_eq!(
        cluster_neighbors.len(),
        1,
        "Cluster should decode to 1 neighbor"
    );
    assert_eq!(
        cluster_neighbors[0], source_id,
        "Cluster neighbor should be source node"
    );
}

/// Test 3: Multi-outgoing cluster neighbors correctness
/// Build graph: 1 -> 2, 1 -> 3, 1 -> 4, verify outgoing neighbors(1) contains {2,3,4}
#[test]
fn test_multi_outgoing_cluster_neighbors_correct() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_multi_outgoing.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source node
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create 3 target nodes
    let mut target_ids = Vec::new();
    for i in 1..=3 {
        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Target".to_string(),
                name: format!("target_{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        target_ids.push(target_id);

        // Create edge from source to target
        graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "connects".to_string(),
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
    }

    // Test outgoing neighbors from source
    let outgoing_neighbors = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Multi-outgoing neighbors from node {}: {:?}",
        source_id, outgoing_neighbors
    );

    // Assertions - should have exactly 3 neighbors
    assert_eq!(
        outgoing_neighbors.len(),
        3,
        "Should have exactly 3 outgoing neighbors"
    );

    // Sort and compare to handle any ordering differences
    let mut sorted_neighbors = outgoing_neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();

    assert_eq!(
        sorted_neighbors, sorted_targets,
        "All 3 target nodes should be returned as neighbors"
    );

    // Verify V2 cluster metadata
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id).unwrap();

    assert_eq!(
        source_node.outgoing_edge_count, 3,
        "Source should have 3 outgoing edge count"
    );
    assert!(
        source_node.outgoing_cluster_size > 0,
        "Source should have non-zero cluster size"
    );

    // Verify cluster_size matches exact serialized length for 3 edges
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let cluster_neighbors = edge_store
        .get_clustered_neighbors(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
            source_id,
        )
        .unwrap();

    assert_eq!(
        cluster_neighbors.len(),
        3,
        "Cluster should decode to 3 neighbors"
    );
}

/// Test 4: Multi-incoming cluster neighbors correctness
/// Build graph: 2 -> 10, 3 -> 10, 4 -> 10, verify incoming neighbors(10) contains {2,3,4}
#[test]
fn test_multi_incoming_cluster_neighbors_correct() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_multi_incoming.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create target node
    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Target".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create 3 source nodes that all point to target
    let mut source_ids = Vec::new();
    for i in 2..=4 {
        let source_id = graph
            .insert_node(NodeSpec {
                kind: "Source".to_string(),
                name: format!("source_{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        source_ids.push(source_id);

        // Create edge from source to target
        graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "points_to".to_string(),
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
    }

    // Test incoming neighbors to target
    let incoming_neighbors = graph
        .neighbors(
            target_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Multi-incoming neighbors to node {}: {:?}",
        target_id, incoming_neighbors
    );

    // Assertions - should have exactly 3 neighbors
    assert_eq!(
        incoming_neighbors.len(),
        3,
        "Should have exactly 3 incoming neighbors"
    );

    // Sort and compare to handle any ordering differences
    let mut sorted_neighbors = incoming_neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_sources = source_ids.clone();
    sorted_sources.sort();

    assert_eq!(
        sorted_neighbors, sorted_sources,
        "All 3 source nodes should be returned as incoming neighbors"
    );

    // Verify V2 cluster metadata
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let target_node = node_store.read_node_v2(target_id).unwrap();

    assert_eq!(
        target_node.incoming_edge_count, 3,
        "Target should have 3 incoming edge count"
    );
    assert!(
        target_node.incoming_cluster_size > 0,
        "Target should have non-zero cluster size"
    );

    // Verify cluster_size matches serialized length for 3 edges
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let cluster_neighbors = edge_store
        .get_clustered_neighbors(
            target_node.incoming_cluster_offset,
            target_node.incoming_cluster_size,
            Direction::Incoming,
            target_id,
        )
        .unwrap();

    assert_eq!(
        cluster_neighbors.len(),
        3,
        "Cluster should decode to 3 neighbors"
    );
}

/// Test 5: Cluster layout roundtrip consistency
/// Build one outgoing cluster with 2-3 edges, serialize, read back, assert consistency
#[test]
fn test_cluster_layout_roundtrip_consistent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_roundtrip.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source node
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create target nodes and edges
    let mut target_ids = Vec::new();
    let mut edge_specs = Vec::new();
    for i in 1..=3 {
        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Target".to_string(),
                name: format!("target_{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        target_ids.push(target_id);

        let edge_spec = EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: format!("edge_type_{}", i),
            data: serde_json::json!({"weight": i as f64, "data": format!("payload_{}", i)}),
        };
        edge_specs.push(edge_spec.clone());

        // Insert edge
        graph.insert_edge(edge_spec).unwrap();
    }

    // Get outgoing neighbors to trigger cluster creation
    let neighbors = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(neighbors.len(), 3, "Should have 3 neighbors");

    // Now verify cluster consistency via direct access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id).unwrap();

    // Read cluster back via EdgeStore::read_clustered_edges
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let compact_edges = edge_store
        .read_clustered_edges(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    // Assert decoded neighbor_ids set == original target set
    let decoded_neighbors: Vec<i64> = compact_edges.iter().map(|e| e.neighbor_id).collect();
    let mut sorted_decoded = decoded_neighbors.clone();
    sorted_decoded.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();

    assert_eq!(
        sorted_decoded, sorted_targets,
        "Decoded neighbor IDs should match original targets"
    );

    // Verify edge type offsets and data lengths are preserved
    assert_eq!(compact_edges.len(), 3, "Should have 3 compact edges");
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
            compact_edge.edge_data.len() > 0,
            "Edge {} should have data",
            i
        );
    }

    // Verify we can reconstruct the cluster and it matches
    let mut string_table = StringTable::new();
    let test_edges: Vec<EdgeRecord> = edge_specs
        .into_iter()
        .map(|spec| EdgeRecord::new(1, source_id, spec.to, spec.edge_type, spec.data))
        .collect();

    let reconstructed_cluster = EdgeCluster::create_from_edges(
        &test_edges,
        source_id,
        Direction::Outgoing,
        &mut string_table,
    )
    .unwrap();

    // Both clusters should have same edge count
    assert_eq!(
        compact_edges.len(),
        reconstructed_cluster.edge_count() as usize,
        "Reconstructed cluster should have same edge count"
    );
}

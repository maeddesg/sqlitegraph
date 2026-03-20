//! V2 Test Fixture Builders
//!
//! Clean, zero-magic helpers for creating V2 graph test data.
//! All functions use real sqlitegraph APIs and Phase 34 pipeline.

use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeSpec, SnapshotId,
    SqliteGraphError, config::GraphConfig, open_graph,
};
pub type NodeId = i64;
use std::path::Path;
use tempfile::TempDir;

/// Clean test graph creation using real Phase 34 pipeline
/// Returns graph and temp dir to keep graph alive during test
pub fn create_test_graph() -> (Box<dyn GraphBackend>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).expect("Failed to create test graph");

    (graph, temp_dir)
}

/// Create a clean test graph at specific path
pub fn create_test_graph_at_path<P: AsRef<Path>>(path: P) -> Box<dyn GraphBackend> {
    let config = GraphConfig::native();
    open_graph(path, &config).expect("Failed to create test graph")
}

/// Add a V2 node using real Phase 34 pipeline
/// Returns node ID for use in edge creation
pub fn add_node_v2(
    graph: &mut Box<dyn GraphBackend>,
    id_hint: u32, // Used only for test predictability
    kind: &str,
    name: &str,
    data: serde_json::Value,
) -> NodeId {
    let node_spec = NodeSpec {
        kind: kind.to_string(),
        name: name.to_string(),
        file_path: None,
        data,
    };

    graph
        .insert_node(node_spec)
        .expect("Failed to insert V2 node")
}

/// Add a V2 node with file path using real Phase 34 pipeline
pub fn add_node_v2_with_path(
    graph: &mut Box<dyn GraphBackend>,
    kind: &str,
    name: &str,
    file_path: Option<&str>,
    data: serde_json::Value,
) -> NodeId {
    let node_spec = NodeSpec {
        kind: kind.to_string(),
        name: name.to_string(),
        file_path: file_path.map(|s| s.to_string()),
        data,
    };

    graph
        .insert_node(node_spec)
        .expect("Failed to insert V2 node with path")
}

/// Add a V2 edge using real Phase 34 pipeline
/// This will trigger Phase 34 clean cluster creation
pub fn add_edge_v2(
    graph: &mut Box<dyn GraphBackend>,
    from: NodeId,
    to: NodeId,
    edge_type: &str,
    data: serde_json::Value,
) -> NodeId {
    let edge_spec = EdgeSpec {
        from,
        to,
        edge_type: edge_type.to_string(),
        data,
    };

    graph
        .insert_edge(edge_spec)
        .expect("Failed to insert V2 edge")
}

/// Add a simple V2 edge with just edge type
pub fn add_simple_edge_v2(
    graph: &mut Box<dyn GraphBackend>,
    from: NodeId,
    to: NodeId,
    edge_type: &str,
) -> NodeId {
    add_edge_v2(graph, from, to, edge_type, serde_json::json!({}))
}

/// Verify V2 cluster metadata exists for a node
pub fn verify_v2_cluster_metadata(
    graph: &Box<dyn GraphBackend>,
    node_id: NodeId,
    expected_outgoing: u32,
    expected_incoming: u32,
) -> Result<(), SqliteGraphError> {
    // Verify by checking that neighbors work correctly
    // If V2 cluster metadata exists and is valid, neighbors will work
    let outgoing = graph.neighbors(
        SnapshotId::current(),
        node_id,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;

    let incoming = graph.neighbors(
        SnapshotId::current(),
        node_id,
        NeighborQuery {
            direction: BackendDirection::Incoming,
            edge_type: None,
        },
    )?;

    assert_eq!(
        outgoing.len() as u32,
        expected_outgoing,
        "Expected {} outgoing neighbors, got {}",
        expected_outgoing,
        outgoing.len()
    );

    assert_eq!(
        incoming.len() as u32,
        expected_incoming,
        "Expected {} incoming neighbors, got {}",
        expected_incoming,
        incoming.len()
    );

    Ok(())
}

/// Flush graph to disk and reopen to test persistence
/// Creates new GraphBackend instance pointing to same file
pub fn flush_and_reopen(graph: Box<dyn GraphBackend>, temp_dir: &TempDir) -> Box<dyn GraphBackend> {
    // Drop the old graph to flush any pending writes
    drop(graph);

    // Reopen the same graph file
    let db_path = temp_dir.path().join("test.db");
    let config = GraphConfig::native();
    open_graph(&db_path, &config).expect("Failed to reopen graph")
}

/// Create a complete 2-node, 1-edge V2 graph
/// Returns (graph, source_id, target_id, temp_dir)
pub fn create_simple_v2_graph() -> (Box<dyn GraphBackend>, NodeId, NodeId, TempDir) {
    let (mut graph, temp_dir) = create_test_graph();

    let source_id = add_node_v2(
        &mut graph,
        1,
        "Source",
        "source_node",
        serde_json::json!({"type": "source"}),
    );
    let target_id = add_node_v2(
        &mut graph,
        2,
        "Target",
        "target_node",
        serde_json::json!({"type": "target"}),
    );

    add_simple_edge_v2(&mut graph, source_id, target_id, "connects");

    (graph, source_id, target_id, temp_dir)
}

/// Create a star graph with center node connected to multiple targets
/// Returns (graph, center_id, target_ids, temp_dir)
pub fn create_star_v2_graph(
    num_targets: usize,
) -> (Box<dyn GraphBackend>, NodeId, Vec<NodeId>, TempDir) {
    let (mut graph, temp_dir) = create_test_graph();

    let center_id = add_node_v2(
        &mut graph,
        0,
        "Center",
        "center_node",
        serde_json::json!({"type": "center"}),
    );

    let mut target_ids = Vec::new();
    for i in 1..=num_targets {
        let target_id = add_node_v2(
            &mut graph,
            i as u32,
            "Target",
            &format!("target_{}", i),
            serde_json::json!({"index": i, "type": "target"}),
        );

        add_edge_v2(
            &mut graph,
            center_id,
            target_id,
            "connects",
            serde_json::json!({"index": i}),
        );
        target_ids.push(target_id);
    }

    (graph, center_id, target_ids, temp_dir)
}

/// Create a linear chain V2 graph: 1 -> 2 -> 3 -> ... -> n
/// Returns (graph, node_ids, temp_dir)
pub fn create_chain_v2_graph(length: usize) -> (Box<dyn GraphBackend>, Vec<NodeId>, TempDir) {
    let (mut graph, temp_dir) = create_test_graph();

    let mut node_ids = Vec::new();

    // Create all nodes
    for i in 0..length {
        let node_id = add_node_v2(
            &mut graph,
            i as u32,
            "Node",
            &format!("node_{}", i),
            serde_json::json!({"index": i, "type": "chain_node"}),
        );
        node_ids.push(node_id);
    }

    // Create edges forming a chain
    for i in 0..(length - 1) {
        add_simple_edge_v2(&mut graph, node_ids[i], node_ids[i + 1], "next");
    }

    (graph, node_ids, temp_dir)
}

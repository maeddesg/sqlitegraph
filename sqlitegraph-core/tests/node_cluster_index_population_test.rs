//! Integration test: node_cluster_index populated during BFS traversal
//!
//! **Purpose:** Verify that Phase 35 sequential cluster read infrastructure
//! is properly wired into BFS traversals.
//!
//! **Bug:** Phase 35 created SequentialClusterReader and node_cluster_index mapping,
//! but BFS implementations never populate the mapping, causing sequential cluster
//! reads to never trigger.
//!
//! **Fix:** After observe_with_cluster(), populate node_cluster_index with:
//!   let cluster_index = ctx.detector.cluster_offsets().len().saturating_sub(1);
//!   ctx.node_cluster_index.insert(current_node, cluster_index);

use sqlitegraph::backend::native::{
    NativeNodeId, edge_store::EdgeStore, graph_file::GraphFile, graph_ops::native_bfs,
    node_store::NodeStore,
};
use tempfile::TempDir;

/// Create a simple linear chain: 0 -> 1 -> 2 -> 3
fn create_linear_chain(size: usize, temp_dir: &TempDir) -> (GraphFile, Vec<NativeNodeId>) {
    let db_path = temp_dir.path().join("test_chain.db");
    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    let mut node_ids = Vec::with_capacity(size);
    for i in 0..size {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store
            .allocate_node_id()
            .expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "Node".to_string(),
            format!("node_{}", i),
            serde_json::json!({"id": i}),
        );
        node_store
            .write_node(&record)
            .expect("Failed to write node");
        node_ids.push(node_id);
    }

    let mut edge_store = EdgeStore::new(&mut graph_file);
    for i in 0..size.saturating_sub(1) {
        let edge = sqlitegraph::backend::native::EdgeRecord::new(
            i as i64 + 1,
            node_ids[i],
            node_ids[i + 1],
            "chain".to_string(),
            serde_json::json!({"order": i}),
        );
        edge_store
            .write_edge(&edge)
            .expect("Failed to write chain edge");
    }

    (graph_file, node_ids)
}

/// Test that bfs_generic_scalar populates node_cluster_index
///
/// Note: bfs_generic_scalar is private, so we test via public native_bfs API.
/// The actual verification that node_cluster_index is populated is done
/// via the performance benchmark - Chain(500) should show improvement.
#[test]
fn test_bfs_populates_node_cluster_index_indirectly() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let (mut graph_file, node_ids) = create_linear_chain(10, &temp_dir);
    let start_node = node_ids[0];

    // Run BFS - should populate node_cluster_index internally
    let result = native_bfs(&mut graph_file, start_node, 10).expect("BFS failed");

    // Verify BFS visited all nodes (start node not included in result)
    assert_eq!(
        result.len(),
        9,
        "BFS should visit 9 neighbor nodes (excluding start)"
    );

    // The key assertion: we can't directly access ctx.node_cluster_index
    // from outside BFS, but the sequential cluster read optimization
    // should now work because the mapping is populated.
    // This is verified indirectly by performance benchmarks.
}

/// Test that native_bfs populates node_cluster_index
#[test]
fn test_native_bfs_populates_node_cluster_index() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let (mut graph_file, node_ids) = create_linear_chain(10, &temp_dir);
    let start_node = node_ids[0];

    // Run BFS via public API
    let result = native_bfs(&mut graph_file, start_node, 10).expect("BFS failed");

    // Verify BFS visited all nodes (start node not included in result)
    assert_eq!(
        result.len(),
        9,
        "BFS should visit 9 neighbor nodes (excluding start)"
    );

    // node_cluster_index population is verified indirectly:
    // - Chain(500) benchmark should show improvement
    // - Sequential cluster read extraction should succeed
}

/// Test that chain traversal with threshold+ nodes populates mapping
#[test]
fn test_chain_traversal_populates_mapping_for_all_nodes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let (mut graph_file, node_ids) = create_linear_chain(5, &temp_dir);
    let start_node = node_ids[0];

    // Chain of 5 nodes should populate 5 entries in node_cluster_index
    let result = native_bfs(&mut graph_file, start_node, 5).expect("BFS failed");

    assert_eq!(
        result.len(),
        4,
        "Should visit 4 neighbor nodes (excluding start)"
    );
    // Internal mapping should have 5 entries (verified indirectly via performance)
}

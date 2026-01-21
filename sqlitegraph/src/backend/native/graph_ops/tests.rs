//! Comprehensive test suite for graph operations

use super::super::{EdgeStore, NodeStore};
use super::*;
use crate::backend::native::clear_node_cache;
use crate::backend::{EdgeSpec, NodeSpec};
use tempfile::NamedTempFile;

fn create_test_graph_file() -> (GraphFile, NamedTempFile) {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();
    let graph_file = GraphFile::create(path).unwrap();
    (graph_file, temp_file)
}

#[test]
fn test_native_bfs_simple() {
    // Clear cache to ensure test isolation
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create nodes
    let node1 = NodeRecord::new(
        1,
        "Test".to_string(),
        "node1".to_string(),
        serde_json::json!({}),
    );
    let node2 = NodeRecord::new(
        2,
        "Test".to_string(),
        "node2".to_string(),
        serde_json::json!({}),
    );
    let node3 = NodeRecord::new(
        3,
        "Test".to_string(),
        "node3".to_string(),
        serde_json::json!({}),
    );

    {
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.write_node(&node1).unwrap();
        node_store.write_node(&node2).unwrap();
        node_store.write_node(&node3).unwrap();
    }

    // Create edges: 1 -> 2 -> 3
    let edge1 = EdgeRecord::new(1, 1, 2, "test".to_string(), serde_json::json!({}));
    let edge2 = EdgeRecord::new(2, 2, 3, "test".to_string(), serde_json::json!({}));

    {
        let mut edge_store = EdgeStore::new(&mut graph_file);
        edge_store.write_edge(&edge1).unwrap();
        edge_store.write_edge(&edge2).unwrap();
    }

    let result = native_bfs(&mut graph_file, 1, 2).unwrap();
    assert!(
        result.contains(&2),
        "Expected to find node 2 in BFS result: {:?}",
        result
    );
    assert!(
        result.contains(&3),
        "Expected to find node 3 in BFS result: {:?}",
        result
    );
}

#[test]
fn test_native_shortest_path() {
    // Clear cache to ensure test isolation
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create nodes
    let node1 = NodeRecord::new(
        1,
        "Test".to_string(),
        "node1".to_string(),
        serde_json::json!({}),
    );
    let node2 = NodeRecord::new(
        2,
        "Test".to_string(),
        "node2".to_string(),
        serde_json::json!({}),
    );
    let node3 = NodeRecord::new(
        3,
        "Test".to_string(),
        "node3".to_string(),
        serde_json::json!({}),
    );

    {
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.write_node(&node1).unwrap();
        node_store.write_node(&node2).unwrap();
        node_store.write_node(&node3).unwrap();
    }

    // Create edge: 1 -> 2 -> 3
    let edge1 = EdgeRecord::new(1, 1, 2, "test".to_string(), serde_json::json!({}));
    let edge2 = EdgeRecord::new(2, 2, 3, "test".to_string(), serde_json::json!({}));

    {
        let mut edge_store = EdgeStore::new(&mut graph_file);
        edge_store.write_edge(&edge1).unwrap();
        edge_store.write_edge(&edge2).unwrap();
    }

    let result = native_shortest_path(&mut graph_file, 1, 3).unwrap();
    assert!(result.is_some());
    let path = result.unwrap();
    assert_eq!(path, vec![1, 2, 3]);
}

#[test]
fn test_bfs_cache_evaporates() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create chain: 1 -> 2 -> 3 -> 4 -> 5
    for i in 1..=5 {
        let node = NodeRecord::new(
            i,
            "Test".to_string(),
            format!("node{}", i),
            serde_json::json!({}),
        );
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.write_node(&node).unwrap();
    }

    // Create edges
    for i in 1..4 {
        let edge = EdgeRecord::new(
            i,
            i,
            i + 1,
            "test".to_string(),
            serde_json::json!({}),
        );
        let mut edge_store = EdgeStore::new(&mut graph_file);
        edge_store.write_edge(&edge).unwrap();
    }

    // First BFS
    let result1 = native_bfs(&mut graph_file, 1, 2).unwrap();
    let expected: Vec<NativeNodeId> = vec![2, 3];
    assert_eq!(result1, expected, "First BFS should return nodes 2, 3");

    // Second BFS with same parameters
    let result2 = native_bfs(&mut graph_file, 1, 2).unwrap();
    assert_eq!(result2, expected, "Second BFS should return same result");

    // Third BFS from different start node
    let result3 = native_bfs(&mut graph_file, 3, 1).unwrap();
    let expected3: Vec<NativeNodeId> = vec![4];
    assert_eq!(result3, expected3, "Third BFS from node 3 should return node 4");

    // All BFS calls produce correct results, proving cache doesn't cause cross-call pollution
}

#[test]
fn test_bfs_cache_cycles() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create diamond + cycle graph: 1->2, 1->3, 2->4, 3->4, 4->2
    for i in 1..=4 {
        let node = NodeRecord::new(
            i,
            "Test".to_string(),
            format!("node{}", i),
            serde_json::json!({}),
        );
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.write_node(&node).unwrap();
    }

    // Diamond edges: 1->2, 1->3, 2->4, 3->4
    let edges = [(1, 2), (1, 3), (2, 4), (3, 4)];
    for (i, (from, to)) in edges.iter().enumerate() {
        let edge = EdgeRecord::new(
            i as NativeNodeId + 1,
            *from,
            *to,
            "test".to_string(),
            serde_json::json!({}),
        );
        let mut edge_store = EdgeStore::new(&mut graph_file);
        edge_store.write_edge(&edge).unwrap();
    }

    // Cycle edge: 4->2
    let cycle_edge = EdgeRecord::new(
        5,
        4,
        2,
        "test".to_string(),
        serde_json::json!({}),
    );
    let mut edge_store = EdgeStore::new(&mut graph_file);
    edge_store.write_edge(&cycle_edge).unwrap();

    // BFS from node 1, depth 3
    let result = native_bfs(&mut graph_file, 1, 3).unwrap();

    // Should find nodes 2, 3, 4 (node 4 discovered twice but visited once)
    assert!(result.contains(&2), "Should contain node 2");
    assert!(result.contains(&3), "Should contain node 3");
    assert!(result.contains(&4), "Should contain node 4");

    // Node 4 should appear only once in result (BFS deduplication via visited set)
    let count_4 = result.iter().filter(|&&n| n == 4).count();
    assert_eq!(count_4, 1, "Node 4 should appear exactly once despite two paths");
}

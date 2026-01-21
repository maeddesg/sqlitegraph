//! Comprehensive test suite for graph operations

use super::super::{EdgeStore, NodeStore};
use super::*;
use crate::backend::native::adjacency::Direction;
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

#[test]
fn test_bfs_unchanged_behavior() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create star graph: 1 -> 2, 1 -> 3, 1 -> 4, 1 -> 5
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

    // Star edges from center node 1
    for i in 2..=5 {
        let edge = EdgeRecord::new(
            i,
            1,
            i,
            "test".to_string(),
            serde_json::json!({}),
        );
        let mut edge_store = EdgeStore::new(&mut graph_file);
        edge_store.write_edge(&edge).unwrap();
    }

    // BFS depth 1 should return all neighbors of node 1
    let result = native_bfs(&mut graph_file, 1, 1).unwrap();
    let mut expected = vec![2, 3, 4, 5];
    expected.sort();
    let mut result_sorted = result.clone();
    result_sorted.sort();
    assert_eq!(result_sorted, expected, "BFS depth 1 should return all direct neighbors");

    // BFS depth 0 should return just start node
    let result_zero = native_bfs(&mut graph_file, 1, 0).unwrap();
    assert_eq!(result_zero, vec![1], "BFS depth 0 should return start node only");

    // BFS from leaf node (no outgoing edges) should return empty
    let result_leaf = native_bfs(&mut graph_file, 2, 2).unwrap();
    assert!(result_leaf.is_empty(), "BFS from leaf node should return empty");
}

// K-hop cache tests

#[test]
fn test_k_hop_cache_evaporation() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create triangle graph: 1 -> 2 -> 3 -> 1
    for i in 1..=3 {
        let node = NodeRecord::new(
            i,
            "Test".to_string(),
            format!("node{}", i),
            serde_json::json!({}),
        );
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.write_node(&node).unwrap();
    }

    // Triangle edges: 1->2, 2->3, 3->1
    let edges = [(1, 2), (2, 3), (3, 1)];
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

    // First k-hop call (depth=1 from node 1)
    let result1 = native_k_hop(&mut graph_file, 1, 1, Direction::Outgoing).unwrap();
    let mut expected1 = vec![2];
    expected1.sort();
    let mut result1_sorted = result1.clone();
    result1_sorted.sort();
    assert_eq!(result1_sorted, expected1, "First k-hop should return node 2");

    // Second k-hop call with same parameters
    let result2 = native_k_hop(&mut graph_file, 1, 1, Direction::Outgoing).unwrap();
    let mut result2_sorted = result2.clone();
    result2_sorted.sort();
    assert_eq!(result2_sorted, expected1, "Second k-hop should return same result");

    // Third k-hop call with different parameters
    let result3 = native_k_hop(&mut graph_file, 2, 1, Direction::Outgoing).unwrap();
    let mut expected3 = vec![3];
    expected3.sort();
    let mut result3_sorted = result3.clone();
    result3_sorted.sort();
    assert_eq!(result3_sorted, expected3, "Third k-hop from node 2 should return node 3");

    // All k-hop calls produce correct results, proving cache doesn't cause cross-call pollution
}

#[test]
fn test_k_hop_cache_effectiveness() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create diamond graph: 1 -> 2, 1 -> 3, 2 -> 4, 3 -> 4
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

    // Diamond edges
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

    // K-hop depth=2 from node 1 should reach node 4 via two paths
    let result = native_k_hop(&mut graph_file, 1, 2, Direction::Outgoing).unwrap();

    // Node 4 should be in results (reached via 1->2->4 and 1->3->4)
    assert!(result.contains(&4), "Should contain node 4 at depth 2");

    // Nodes 2 and 3 should be in results (depth 1)
    assert!(result.contains(&2), "Should contain node 2 at depth 1");
    assert!(result.contains(&3), "Should contain node 3 at depth 1");

    // Node 4 should appear only once (k-hop deduplicates via visited set)
    let count_4 = result.iter().filter(|&&n| n == 4).count();
    assert_eq!(count_4, 1, "Node 4 should appear exactly once despite two paths");

    // Result correctness proves cache doesn't break k-hop semantics
}

// Shortest path cache tests

#[test]
fn test_shortest_path_cache_evaporation() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create linear graph: 1 -> 2 -> 3 -> 4
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

    // Linear edges: 1->2, 2->3, 3->4
    let edges = [(1, 2), (2, 3), (3, 4)];
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

    // First shortest path call
    let result1 = native_shortest_path(&mut graph_file, 1, 4).unwrap();
    assert!(result1.is_some(), "Should find a path from 1 to 4");
    assert_eq!(result1.unwrap(), vec![1, 2, 3, 4], "First shortest path should be [1,2,3,4]");

    // Second shortest path call with same parameters
    let result2 = native_shortest_path(&mut graph_file, 1, 4).unwrap();
    assert!(result2.is_some(), "Should find a path from 1 to 4");
    assert_eq!(result2.unwrap(), vec![1, 2, 3, 4], "Second shortest path should be [1,2,3,4]");

    // Third shortest path call with different parameters
    let result3 = native_shortest_path(&mut graph_file, 2, 4).unwrap();
    assert!(result3.is_some(), "Should find a path from 2 to 4");
    assert_eq!(result3.unwrap(), vec![2, 3, 4], "Third shortest path from 2 to 4 should be [2,3,4]");

    // All shortest path calls produce correct results, proving cache doesn't cause cross-call pollution
}

#[test]
fn test_shortest_path_cache_effectiveness() {
    clear_node_cache();

    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create diamond graph with branching paths: 1 -> 2 -> 4, 1 -> 3 -> 4
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

    // Shortest path from 1 to 4
    let result = native_shortest_path(&mut graph_file, 1, 4).unwrap();

    assert!(result.is_some(), "Should find a path from 1 to 4");
    let path = result.unwrap();

    // Either path [1,2,4] or [1,3,4] is valid (both are shortest paths of length 3)
    let valid_path = path == vec![1, 2, 4] || path == vec![1, 3, 4];
    assert!(valid_path, "Path should be [1,2,4] or [1,3,4], got {:?}", path);

    // Path length should be 3
    assert_eq!(path.len(), 3, "Shortest path should have length 3");

    // Path should start at 1 and end at 4
    assert_eq!(path[0], 1, "Path should start at node 1");
    assert_eq!(path[2], 4, "Path should end at node 4");

    // Result correctness proves cache doesn't break shortest path semantics
    // The BFS explores both paths; cache prevents re-reading node 1's neighbors when dequeuing 2 and 3
}

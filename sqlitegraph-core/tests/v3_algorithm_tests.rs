//! V3 Backend Algorithm Integration Tests
//!
//! Tests that verify graph algorithms work correctly with the V3 native backend
//! using only the GraphBackend trait methods.

use sqlitegraph::{
    backend::{EdgeSpec, GraphBackend, NodeSpec},
    backend::native::v3::V3Backend,
};
use tempfile::TempDir;

/// Helper to create a V3-backed graph for algorithm testing
fn create_v3_backend() -> (V3Backend, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v3.graph");
    let backend = V3Backend::create(&db_path).unwrap();
    (backend, temp_dir)
}

/// Build a simple chain graph: 1 -> 2 -> 3 -> 4
fn build_chain_graph(backend: &V3Backend) -> Vec<i64> {
    let mut nodes = Vec::new();
    
    // Create 4 nodes
    for i in 1..=4 {
        let id = backend.insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({}),
        }).unwrap();
        nodes.push(id);
    }
    
    // Create chain edges: 1 -> 2 -> 3 -> 4
    for i in 0..nodes.len()-1 {
        backend.insert_edge(EdgeSpec {
            from: nodes[i],
            to: nodes[i+1],
            edge_type: "links_to".to_string(),
            data: serde_json::json!({}),
        }).unwrap();
    }
    
    nodes
}

/// Build a star graph: center -> leaf1, center -> leaf2, center -> leaf3
fn build_star_graph(backend: &V3Backend) -> (i64, Vec<i64>) {
    let center = backend.insert_node(NodeSpec {
        kind: "Center".to_string(),
        name: "center".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let mut leaves = Vec::new();
    for i in 1..=3 {
        let leaf = backend.insert_node(NodeSpec {
            kind: "Leaf".to_string(),
            name: format!("leaf_{}", i),
            file_path: None,
            data: serde_json::json!({}),
        }).unwrap();
        
        backend.insert_edge(EdgeSpec {
            from: center,
            to: leaf,
            edge_type: "links_to".to_string(),
            data: serde_json::json!({}),
        }).unwrap();
        
        leaves.push(leaf);
    }
    
    (center, leaves)
}

/// Build a cycle graph: 1 -> 2 -> 3 -> 1
fn build_cycle_graph(backend: &V3Backend) -> Vec<i64> {
    let mut nodes = Vec::new();
    
    // Create 3 nodes
    for i in 1..=3 {
        let id = backend.insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({}),
        }).unwrap();
        nodes.push(id);
    }
    
    // Create cycle edges
    for i in 0..nodes.len() {
        let next = (i + 1) % nodes.len();
        backend.insert_edge(EdgeSpec {
            from: nodes[i],
            to: nodes[next],
            edge_type: "links_to".to_string(),
            data: serde_json::json!({}),
        }).unwrap();
    }
    
    nodes
}

// ============================================================================
// Core Graph Operations Tests (used by algorithms)
// ============================================================================

#[test]
fn test_v3_entity_ids_basic() {
    let (backend, _temp) = create_v3_backend();
    
    // Initially empty
    let ids = backend.entity_ids().unwrap();
    assert!(ids.is_empty(), "New database should have no entities");
    
    // Insert nodes
    let node1 = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "A".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node2 = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "B".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    // Verify enumeration
    let ids = backend.entity_ids().unwrap();
    assert_eq!(ids.len(), 2, "Should have 2 entities");
    assert!(ids.contains(&node1), "Should contain node1");
    assert!(ids.contains(&node2), "Should contain node2");
}

#[test]
fn test_v3_fetch_outgoing() {
    let (backend, _temp) = create_v3_backend();
    let nodes = build_chain_graph(&backend);
    
    // Test outgoing from each node
    let out_0 = backend.fetch_outgoing(nodes[0]).unwrap();
    assert_eq!(out_0.len(), 1, "Node 0 should have 1 outgoing edge");
    assert!(out_0.contains(&nodes[1]), "Node 0 should point to node 1");
    
    let out_1 = backend.fetch_outgoing(nodes[1]).unwrap();
    assert_eq!(out_1.len(), 1, "Node 1 should have 1 outgoing edge");
    assert!(out_1.contains(&nodes[2]), "Node 1 should point to node 2");
    
    let out_3 = backend.fetch_outgoing(nodes[3]).unwrap();
    assert!(out_3.is_empty(), "Node 3 should have no outgoing edges");
}

#[test]
fn test_v3_fetch_incoming() {
    let (backend, _temp) = create_v3_backend();
    let nodes = build_chain_graph(&backend);
    
    // Test incoming to each node
    let in_0 = backend.fetch_incoming(nodes[0]).unwrap();
    assert!(in_0.is_empty(), "Node 0 should have no incoming edges");
    
    let in_1 = backend.fetch_incoming(nodes[1]).unwrap();
    assert_eq!(in_1.len(), 1, "Node 1 should have 1 incoming edge");
    assert!(in_1.contains(&nodes[0]), "Node 1 should be pointed to by node 0");
    
    let in_3 = backend.fetch_incoming(nodes[3]).unwrap();
    assert_eq!(in_3.len(), 1, "Node 3 should have 1 incoming edge");
    assert!(in_3.contains(&nodes[2]), "Node 3 should be pointed to by node 2");
}

// ============================================================================
// Algorithm-Specific Tests
// ============================================================================

/// Test that demonstrates PageRank can be computed using GraphBackend trait
#[test]
fn test_v3_pagerank_via_trait() {
    let (backend, _temp) = create_v3_backend();
    let nodes = build_chain_graph(&backend);
    
    // Use trait methods to compute PageRank manually
    let all_ids = backend.entity_ids().unwrap();
    let n = all_ids.len();
    assert_eq!(n, 4, "Should have 4 nodes");
    
    // Initialize scores
    let mut scores: std::collections::HashMap<i64, f64> = 
        all_ids.iter().map(|&id| (id, 1.0 / n as f64)).collect();
    
    // Pre-compute outgoing counts
    let mut outgoing_counts: std::collections::HashMap<i64, usize> = 
        std::collections::HashMap::new();
    for &id in &all_ids {
        let count = backend.fetch_outgoing(id).unwrap().len();
        outgoing_counts.insert(id, count);
    }
    
    // Run a few iterations of PageRank
    let damping = 0.85;
    for _ in 0..20 {
        let mut new_scores: std::collections::HashMap<i64, f64> = 
            std::collections::HashMap::new();
        
        let base_score = (1.0 - damping) / n as f64;
        for &id in &all_ids {
            new_scores.insert(id, base_score);
        }
        
        let mut dangling_score = 0.0;
        
        for &id in &all_ids {
            let score = scores[&id];
            let out_count = outgoing_counts[&id];
            
            if out_count == 0 {
                dangling_score += score;
            } else {
                let share = score / out_count as f64;
                for &neighbor in &backend.fetch_outgoing(id).unwrap() {
                    *new_scores.get_mut(&neighbor).unwrap() += damping * share;
                }
            }
        }
        
        let dangling_share = damping * dangling_score / n as f64;
        for (_, score) in new_scores.iter_mut() {
            *score += dangling_share;
        }
        
        scores = new_scores;
    }
    
    // Verify scores sum to ~1.0
    let total: f64 = scores.values().sum();
    assert!((total - 1.0).abs() < 0.01, "Scores should sum to ~1.0, got {}", total);
    
    // In a chain, the end node should have highest score
    assert!(
        scores[&nodes[3]] > scores[&nodes[0]],
        "End of chain should have higher score than start"
    );
}

/// Test BFS traversal using GraphBackend trait
#[test]
fn test_v3_bfs_via_trait() {
    let (backend, _temp) = create_v3_backend();
    let nodes = build_chain_graph(&backend);
    
    // Manual BFS using trait methods
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();
    
    queue.push_back(nodes[0]);
    visited.insert(nodes[0]);
    
    while let Some(node) = queue.pop_front() {
        result.push(node);
        
        for neighbor in backend.fetch_outgoing(node).unwrap() {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }
    
    // Should visit all 4 nodes
    assert_eq!(result.len(), 4, "BFS should visit all 4 nodes");
    assert_eq!(result[0], nodes[0]);
    assert_eq!(result[1], nodes[1]);
    assert_eq!(result[2], nodes[2]);
    assert_eq!(result[3], nodes[3]);
}

/// Test SCC (cycle detection) using GraphBackend trait
#[test]
fn test_v3_scc_cycle_via_trait() {
    let (backend, _temp) = create_v3_backend();
    let nodes = build_cycle_graph(&backend);
    
    // Verify it's a cycle using outgoing/incoming
    for i in 0..nodes.len() {
        let outgoing = backend.fetch_outgoing(nodes[i]).unwrap();
        assert_eq!(outgoing.len(), 1, "Each node should have 1 outgoing");
        
        let next = (i + 1) % nodes.len();
        assert_eq!(outgoing[0], nodes[next], "Should point to next node in cycle");
    }
    
    // All nodes should have 1 incoming
    for node in &nodes {
        let incoming = backend.fetch_incoming(*node).unwrap();
        assert_eq!(incoming.len(), 1, "Each node should have 1 incoming in cycle");
    }
}

/// Test star graph topology using GraphBackend trait
#[test]
fn test_v3_star_topology() {
    let (backend, _temp) = create_v3_backend();
    let (center, leaves) = build_star_graph(&backend);
    
    // Center should have 3 outgoing
    let center_out = backend.fetch_outgoing(center).unwrap();
    assert_eq!(center_out.len(), 3, "Center should have 3 outgoing edges");
    
    for leaf in &leaves {
        assert!(center_out.contains(leaf), "Center should point to all leaves");
    }
    
    // Center should have 0 incoming
    let center_in = backend.fetch_incoming(center).unwrap();
    assert!(center_in.is_empty(), "Center should have no incoming edges");
    
    // Each leaf should have 0 outgoing, 1 incoming
    for leaf in &leaves {
        let leaf_out = backend.fetch_outgoing(*leaf).unwrap();
        assert!(leaf_out.is_empty(), "Leaf should have no outgoing edges");
        
        let leaf_in = backend.fetch_incoming(*leaf).unwrap();
        assert_eq!(leaf_in.len(), 1, "Leaf should have 1 incoming edge");
        assert_eq!(leaf_in[0], center, "Leaf should be pointed to by center");
    }
}

/// Test that demonstrates shortest path can be computed using GraphBackend trait
#[test]
fn test_v3_shortest_path_via_trait() {
    let (backend, _temp) = create_v3_backend();
    let nodes = build_chain_graph(&backend);
    
    // Dijkstra/BFS shortest path using trait methods
    let start = nodes[0];
    let end = nodes[3];
    
    let mut distances: std::collections::HashMap<i64, usize> = 
        std::collections::HashMap::new();
    let mut predecessors: std::collections::HashMap<i64, i64> = 
        std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::new();
    
    distances.insert(start, 0);
    queue.push_back(start);
    
    while let Some(node) = queue.pop_front() {
        let current_dist = distances[&node];
        
        for neighbor in backend.fetch_outgoing(node).unwrap() {
            if !distances.contains_key(&neighbor) {
                distances.insert(neighbor, current_dist + 1);
                predecessors.insert(neighbor, node);
                queue.push_back(neighbor);
            }
        }
    }
    
    // Should have found end node
    assert!(distances.contains_key(&end), "Should have found path to end node");
    assert_eq!(distances[&end], 3, "Path length should be 3 edges");
    
    // Reconstruct path
    let mut path = vec![end];
    let mut current = end;
    while let Some(&pred) = predecessors.get(&current) {
        path.push(pred);
        current = pred;
    }
    path.reverse();
    
    assert_eq!(path.len(), 4, "Path should have 4 nodes");
    assert_eq!(path[0], nodes[0]);
    assert_eq!(path[1], nodes[1]);
    assert_eq!(path[2], nodes[2]);
    assert_eq!(path[3], nodes[3]);
}

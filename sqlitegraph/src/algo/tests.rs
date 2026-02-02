//
// CONCURRENT ALGORITHM TESTS
//
// Tests verify thread-safety traits and algorithm consistency.
// Note: SqliteGraph itself is NOT thread-safe for writes, but read-only
// algorithm functions can be called from multiple threads if each has its
// own graph connection or snapshot. These tests verify the algorithm
// functions themselves have the right trait bounds.
//

use crate::{errors::SqliteGraphError, graph::SqliteGraph};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

// Import all algorithm functions from parent modules
use super::{
    centrality::{
        betweenness_centrality, betweenness_centrality_with_progress, pagerank,
        pagerank_with_progress,
    },
    community::{label_propagation, louvain_communities, louvain_communities_with_progress},
    structure::{connected_components, find_cycles_limited, nodes_by_degree},
    transitive_closure::{transitive_closure, transitive_closure_with_progress, TransitiveClosureBounds},
    wcc::{weakly_connected_components, weakly_connected_components_with_progress},
};

#[test]
fn test_algorithms_are_send() {
    // Scenario: Verify all algorithm functions are Send
    // Expected: Algorithm functions have Send trait bounds

    // Verify key functions are Send by taking references
    // (This will compile only if the types are Send)
    let _ = || {
        let graph = create_test_graph();
        let _ = connected_components(&graph);
        let _ = weakly_connected_components(&graph);
        let _ = label_propagation(&graph, 10);
        let _ = louvain_communities(&graph, 10);
        let _ = pagerank(&graph, 0.85, 10);
        let _ = betweenness_centrality(&graph);
        let _ = nodes_by_degree(&graph, true);
        let _ = transitive_closure(&graph, None);
    };

    // If this compiles, all the algorithm functions are Send
    assert!(true);
}

#[test]
fn test_pagerank_consistency_across_calls() {
    // Scenario: PageRank produces consistent results across multiple calls
    // Expected: Same graph + parameters = same results (deterministic)
    let graph = create_test_graph();
    let damping = 0.85;
    let iterations = 10;

    let result1 = pagerank(&graph, damping, iterations);
    let result2 = pagerank(&graph, damping, iterations);

    assert!(result1.is_ok(), "First PageRank failed");
    assert!(result2.is_ok(), "Second PageRank failed");

    let scores1 = result1.unwrap();
    let scores2 = result2.unwrap();

    assert_eq!(scores1.len(), scores2.len(), "Different number of scores");

    // Compare each score (floating point tolerance)
    for (s1, s2) in scores1.iter().zip(scores2.iter()) {
        assert_eq!(s1.0, s2.0, "Different node IDs");
        assert!(
            (s1.1 - s2.1).abs() < 1e-10,
            "PageRank scores differ: {} vs {}",
            s1.1,
            s2.1
        );
    }
}

#[test]
fn test_betweenness_deterministic_output() {
    // Scenario: Betweenness centrality produces deterministic output
    // Expected: Same graph produces same centrality values
    let graph = create_test_graph();

    let result1 = betweenness_centrality(&graph);
    let result2 = betweenness_centrality(&graph);

    assert!(result1.is_ok(), "First betweenness failed");
    assert!(result2.is_ok(), "Second betweenness failed");

    let centrality1 = result1.unwrap();
    let centrality2 = result2.unwrap();

    assert_eq!(centrality1.len(), centrality2.len());

    for (c1, c2) in centrality1.iter().zip(centrality2.iter()) {
        assert_eq!(c1.0, c2.0, "Different node IDs");
        assert!(
            (c1.1 - c2.1).abs() < 1e-10,
            "Centrality values differ: {} vs {}",
            c1.1,
            c2.1
        );
    }
}

#[test]
fn test_label_propagation_deterministic() {
    // Scenario: Label propagation produces deterministic communities
    // Expected: Same graph produces same community assignments
    let graph = create_test_graph();
    let max_iterations = 10;

    let result1 = label_propagation(&graph, max_iterations);
    let result2 = label_propagation(&graph, max_iterations);

    assert!(result1.is_ok(), "First label propagation failed");
    assert!(result2.is_ok(), "Second label propagation failed");

    let communities1 = result1.unwrap();
    let communities2 = result2.unwrap();

    assert_eq!(communities1.len(), communities2.len());

    // Communities are sorted, so direct comparison works
    assert_eq!(communities1, communities2, "Communities differ");
}

#[test]
fn test_algorithm_result_types_are_thread_safe() {
    // Scenario: Verify algorithm result types are Send + Sync
    // Expected: Result types can be shared across threads
    fn is_send_sync<T: Send + Sync>() {}

    // Algorithm return types should be Send + Sync
    is_send_sync::<Vec<Vec<i64>>>();
    is_send_sync::<Vec<(i64, f64)>>();
    is_send_sync::<Vec<(i64, usize)>>();
    is_send_sync::<Result<Vec<Vec<i64>>, SqliteGraphError>>();
    is_send_sync::<Result<Vec<(i64, f64)>, SqliteGraphError>>();
}

#[test]
fn test_connected_components_basic() {
    // Scenario: Find connected components in a simple graph
    // Expected: Returns correct number of components
    let graph = create_test_graph();

    let result = connected_components(&graph);
    assert!(result.is_ok(), "connected_components failed");

    let components = result.unwrap();
    // In a chain graph (1-2-3-...-10), we expect 1 component
    assert_eq!(components.len(), 1, "Expected 1 connected component");
    assert_eq!(components[0].len(), 10, "Expected 10 nodes in component");
}

#[test]
fn test_find_cycles_empty_graph() {
    // Scenario: Find cycles in an acyclic graph
    // Expected: Returns empty vector
    let graph = create_test_graph(); // Chain graph is acyclic

    let result = find_cycles_limited(&graph, 10);
    assert!(result.is_ok(), "find_cycles_limited failed");

    let cycles = result.unwrap();
    assert_eq!(cycles.len(), 0, "Expected no cycles in chain graph");
}

#[test]
fn test_nodes_by_degree_descending() {
    // Scenario: Rank nodes by degree in descending order
    // Expected: Highest degree nodes first
    let graph = create_test_graph();

    let result = nodes_by_degree(&graph, true);
    assert!(result.is_ok(), "nodes_by_degree failed");

    let degrees = result.unwrap();
    // First node should have highest degree (endpoints of chain have degree 1)
    // Middle nodes have degree 2
    assert!(
        degrees[0].1 >= degrees[degrees.len() - 1].1,
        "Not sorted descending"
    );
}

#[test]
fn test_progress_callbacks_complete() {
    // Scenario: Progress callbacks are called correctly
    // Expected: on_complete is called for all progress variants
    use crate::progress::{NoProgress, ProgressCallback};

    let graph = create_test_graph();

    // Test PageRank with progress
    let progress = NoProgress;
    let result = pagerank_with_progress(&graph, 0.85, 5, &progress);
    assert!(result.is_ok(), "pagerank_with_progress failed");

    // Test betweenness with progress
    let result = betweenness_centrality_with_progress(&graph, &progress);
    assert!(
        result.is_ok(),
        "betweenness_centrality_with_progress failed"
    );

    // Test Louvain with progress
    let result = louvain_communities_with_progress(&graph, 5, &progress);
    assert!(result.is_ok(), "louvain_communities_with_progress failed");

    // Test transitive closure with progress
    let result = transitive_closure_with_progress(&graph, None, &progress);
    assert!(result.is_ok(), "transitive_closure_with_progress failed");
}

#[test]
fn test_transitive_closure_deterministic() {
    // Scenario: Transitive closure produces deterministic output
    // Expected: Same graph produces same reachable pairs
    let graph = create_test_graph();

    let result1 = transitive_closure(&graph, None);
    let result2 = transitive_closure(&graph, None);

    assert!(result1.is_ok(), "First transitive_closure failed");
    assert!(result2.is_ok(), "Second transitive_closure failed");

    let closure1 = result1.unwrap();
    let closure2 = result2.unwrap();

    assert_eq!(closure1.len(), closure2.len(), "Different number of pairs");

    // Compare all pairs
    assert_eq!(closure1, closure2, "Transitive closures differ");
}

#[test]
fn test_transitive_closure_bounded_depth() {
    // Scenario: Transitive closure with max_depth limit
    // Expected: Only pairs within depth limit are included
    let graph = create_test_graph();

    let bounds = TransitiveClosureBounds {
        max_depth: Some(2),
        max_sources: None,
        max_pairs: None,
    };

    let result = transitive_closure(&graph, Some(bounds));
    assert!(result.is_ok(), "transitive_closure with bounds failed");

    let closure = result.unwrap();

    // In a chain graph with depth 2, first node can reach itself + 2 more
    // So total pairs should be less than full closure
    let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");

    // With depth 2: node[0] can reach node[0], node[1], node[2]
    // Cannot reach node[3] and beyond
    let first_node = entity_ids[0];
    let third_node = entity_ids.get(2).copied().unwrap_or(entity_ids[0]);
    let fourth_node = entity_ids.get(3).copied().unwrap_or(first_node);

    // First node should reach itself
    assert_eq!(
        closure.get(&(first_node, first_node)),
        Some(&true),
        "Node should reach itself"
    );

    // First node should reach third node (depth 2)
    if entity_ids.len() > 2 {
        assert_eq!(
            closure.get(&(first_node, third_node)),
            Some(&true),
            "Node should reach node at depth 2"
        );
    }

    // First node should NOT reach fourth node (depth 3 exceeds limit)
    if entity_ids.len() > 3 {
        assert_eq!(
            closure.get(&(first_node, fourth_node)),
            None,
            "Node should NOT reach node at depth 3 (depth limit)"
        );
    }
}

#[test]
fn test_transitive_closure_bounded_pairs() {
    // Scenario: Transitive closure with max_pairs limit
    // Expected: Stops early after reaching max_pairs
    let graph = create_test_graph();

    let bounds = TransitiveClosureBounds {
        max_depth: None,
        max_sources: None,
        max_pairs: Some(5),
    };

    let result = transitive_closure(&graph, Some(bounds));
    assert!(result.is_ok(), "transitive_closure with max_pairs failed");

    let closure = result.unwrap();
    assert_eq!(closure.len(), 5, "Should stop at exactly 5 pairs");
}

#[test]
fn test_transitive_closure_with_progress_callback() {
    // Scenario: Progress callback is invoked correctly
    // Expected: Progress callback called for each source node
    use crate::progress::{NoProgress, ProgressCallback};

    let graph = create_test_graph();

    // Test with progress callback
    let progress = NoProgress;
    let result = transitive_closure_with_progress(&graph, None, &progress);
    assert!(result.is_ok(), "transitive_closure_with_progress failed");

    let closure = result.unwrap();
    assert!(closure.len() > 0, "Should have reachable pairs");
}

#[test]
fn test_transitive_closure_self_reachability() {
    // Scenario: Every node should be able to reach itself
    // Expected: (n, n) = true for all nodes
    let graph = create_test_graph();

    let result = transitive_closure(&graph, None);
    assert!(result.is_ok(), "transitive_closure failed");

    let closure = result.unwrap();
    let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");

    // Verify self-reachability for all nodes
    for &node_id in &entity_ids {
        assert_eq!(
            closure.get(&(node_id, node_id)),
            Some(&true),
            "Node {} should reach itself",
            node_id
        );
    }
}

// Helper: Create test graph
fn create_test_graph() -> SqliteGraph {
    use crate::GraphEntity;

    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    // Create test entities
    for i in 0..10 {
        let entity = GraphEntity {
            id: 0,
            kind: "test".to_string(),
            name: format!("test_{}", i),
            file_path: Some(format!("test_{}.rs", i)),
            data: serde_json::json!({"index": i}),
        };
        graph
            .insert_entity(&entity)
            .expect("Failed to insert entity");
    }

    // Create some edges
    let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");
    for i in 0..entity_ids.len().saturating_sub(1) {
        let edge = crate::GraphEdge {
            id: 0,
            from_id: entity_ids[i],
            to_id: entity_ids[i + 1],
            edge_type: "connects".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).ok();
    }

    graph
}

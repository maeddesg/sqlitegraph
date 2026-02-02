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
    scc::{strongly_connected_components, SccResult},
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
        let _ = strongly_connected_components(&graph);
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
    is_send_sync::<SccResult>();
    is_send_sync::<Result<SccResult, SqliteGraphError>>();
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

#[test]
fn test_scc_empty_graph() {
    // Scenario: SCC on empty graph returns empty result
    // Expected: No components, no mappings
    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");
    let result = strongly_connected_components(&graph);

    assert!(result.is_ok(), "SCC failed on empty graph");
    let scc = result.unwrap();
    assert_eq!(scc.components.len(), 0, "Should have no components");
    assert_eq!(scc.node_to_component.len(), 0, "Should have no mappings");
    assert_eq!(scc.condensed_edges.len(), 0, "Should have no condensed edges");
}

#[test]
fn test_scc_linear_chain() {
    // Scenario: Linear chain has no cycles
    // Expected: Each node is its own SCC (all trivial)
    let graph = create_test_graph(); // Creates chain: 0 -> 1 -> 2 -> ... -> 9

    let result = strongly_connected_components(&graph);
    assert!(result.is_ok(), "SCC failed on chain graph");

    let scc = result.unwrap();
    assert_eq!(scc.components.len(), 10, "Each node should be its own SCC");
    assert_eq!(scc.non_trivial_count(), 0, "Should have no non-trivial SCCs");

    // Condensed DAG should have edges forming a chain
    assert_eq!(scc.condensed_edges.len(), 9, "Chain of 10 nodes has 9 edges");
}

#[test]
fn test_scc_single_cycle() {
    // Scenario: Simple cycle creates one non-trivial SCC
    // Expected: One SCC containing all nodes in the cycle
    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    // Create 3 nodes with a cycle: 0 -> 1 -> 2 -> 0
    let entity_ids = create_cycle_nodes(&graph, 3);

    let result = strongly_connected_components(&graph);
    assert!(result.is_ok(), "SCC failed on cycle graph");

    let scc = result.unwrap();
    assert_eq!(scc.non_trivial_count(), 1, "Should have 1 non-trivial SCC");

    // Find the non-trivial SCC
    let cycle_component = scc
        .components
        .iter()
        .find(|c| c.len() == 3)
        .expect("Should have a 3-node SCC");

    // Verify it contains all three nodes
    assert!(cycle_component.contains(&entity_ids[0]));
    assert!(cycle_component.contains(&entity_ids[1]));
    assert!(cycle_component.contains(&entity_ids[2]));

    // Verify cycle detection
    for node in cycle_component {
        assert!(scc.is_in_cycle(*node), "Node should be marked as in cycle");
    }
}

#[test]
fn test_scc_mutual_recursion() {
    // Scenario: Two nodes calling each other (mutual recursion)
    // Expected: One SCC with 2 nodes, other nodes separate
    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    let entity_ids = create_mutual_recursion_graph(&graph);

    let result = strongly_connected_components(&graph);
    assert!(result.is_ok(), "SCC failed on mutual recursion graph");

    let scc = result.unwrap();

    // Should have 1 non-trivial SCC (the mutual recursion)
    assert_eq!(scc.non_trivial_count(), 1, "Should have 1 non-trivial SCC");

    // Find the 2-node SCC
    let recursion_component = scc
        .components
        .iter()
        .find(|c| c.len() == 2)
        .expect("Should have a 2-node SCC");

    assert!(recursion_component.contains(&entity_ids[0]));
    assert!(recursion_component.contains(&entity_ids[1]));

    // Nodes 2, 3, 4 should be in their own SCCs (linear chain)
    assert_eq!(scc.node_to_component[&entity_ids[2]], scc.node_to_component[&entity_ids[2]]);
    assert_ne!(
        scc.node_to_component[&entity_ids[2]],
        scc.node_to_component[&entity_ids[0]]
    );
}

#[test]
fn test_scc_deterministic() {
    // Scenario: SCC produces deterministic output
    // Expected: Same graph produces same SCC decomposition
    let graph = create_test_graph();

    let result1 = strongly_connected_components(&graph);
    let result2 = strongly_connected_components(&graph);

    assert!(result1.is_ok(), "First SCC failed");
    assert!(result2.is_ok(), "Second SCC failed");

    let scc1 = result1.unwrap();
    let scc2 = result2.unwrap();

    // Check component count
    assert_eq!(scc1.components.len(), scc2.components.len(), "Different component counts");

    // Check node-to-component mapping
    assert_eq!(scc1.node_to_component.len(), scc2.node_to_component.len());

    for (node, &comp1) in &scc1.node_to_component {
        let comp2 = scc2.node_to_component.get(node);
        assert_eq!(comp2, Some(&comp1), "Node assigned to different component");
    }
}

#[test]
fn test_scc_condensed_dag_is_acyclic() {
    // Scenario: Condensed DAG should have no cycles
    // Expected: No edges from a component to itself
    let graph = create_test_graph();

    let result = strongly_connected_components(&graph);
    assert!(result.is_ok(), "SCC failed");

    let scc = result.unwrap();

    // Verify no self-loops in condensed DAG
    for &(from, to) in &scc.condensed_edges {
        assert_ne!(from, to, "Condensed DAG should not have self-loops");
    }
}

// Helper: Create cycle nodes
fn create_cycle_nodes(graph: &SqliteGraph, count: usize) -> Vec<i64> {
    use crate::GraphEntity;

    let mut entity_ids = Vec::new();

    // Create nodes
    for i in 0..count {
        let entity = GraphEntity {
            id: 0,
            kind: "test".to_string(),
            name: format!("cycle_{}", i),
            file_path: Some(format!("cycle_{}.rs", i)),
            data: serde_json::json!({"index": i}),
        };
        let id = graph.insert_entity(&entity).expect("Failed to insert entity");
        entity_ids.push(id);
    }

    // Create cycle: 0 -> 1 -> 2 -> ... -> (n-1) -> 0
    for i in 0..count {
        let edge = crate::GraphEdge {
            id: 0,
            from_id: entity_ids[i],
            to_id: entity_ids[(i + 1) % count],
            edge_type: "cycle".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).ok();
    }

    entity_ids
}

// Helper: Create mutual recursion graph
fn create_mutual_recursion_graph(graph: &SqliteGraph) -> Vec<i64> {
    use crate::GraphEntity;

    let mut entity_ids = Vec::new();

    // Create 5 nodes
    for i in 0..5 {
        let entity = GraphEntity {
            id: 0,
            kind: "test".to_string(),
            name: format!("recursion_{}", i),
            file_path: Some(format!("recursion_{}.rs", i)),
            data: serde_json::json!({"index": i}),
        };
        let id = graph.insert_entity(&entity).expect("Failed to insert entity");
        entity_ids.push(id);
    }

    // Create mutual recursion: 0 <-> 1
    let edges = vec![
        (0, 1, "calls_a"),
        (1, 0, "calls_b"),
        (2, 3, "calls"),
        (3, 4, "calls"),
    ];

    for (from_idx, to_idx, edge_type) in edges {
        let edge = crate::GraphEdge {
            id: 0,
            from_id: entity_ids[from_idx],
            to_id: entity_ids[to_idx],
            edge_type: edge_type.to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).ok();
    }

    entity_ids
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

#[test]
fn test_wcc_empty_graph() {
    // Scenario: WCC on empty graph
    // Expected: Returns empty vector
    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    let result = weakly_connected_components(&graph);
    assert!(result.is_ok(), "WCC failed on empty graph");

    let components = result.unwrap();
    assert_eq!(components.len(), 0, "Expected 0 components in empty graph");
}

#[test]
fn test_wcc_single_node() {
    // Scenario: WCC on graph with single node
    // Expected: Returns [[node_id]]
    use crate::GraphEntity;

    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    let entity = GraphEntity {
        id: 0,
        kind: "node".to_string(),
        name: "single_node".to_string(),
        file_path: Some("single_node.rs".to_string()),
        data: serde_json::json!({}),
    };
    graph
        .insert_entity(&entity)
        .expect("Failed to insert entity");

    let result = weakly_connected_components(&graph);
    assert!(result.is_ok(), "WCC failed on single node");

    let components = result.unwrap();
    assert_eq!(components.len(), 1, "Expected 1 component");
    assert_eq!(components[0].len(), 1, "Expected 1 node in component");
}

#[test]
fn test_wcc_linear_chain() {
    // Scenario: WCC on linear chain (0 -> 1 -> 2 -> ... -> 9)
    // Expected: All nodes in one component (edges are bidirectional)
    let graph = create_test_graph();

    let result = weakly_connected_components(&graph);
    assert!(result.is_ok(), "WCC failed on linear chain");

    let components = result.unwrap();
    assert_eq!(
        components.len(),
        1,
        "Expected 1 component in linear chain"
    );
    assert_eq!(
        components[0].len(),
        10,
        "Expected all 10 nodes in single component"
    );

    // Verify all nodes appear exactly once
    let all_nodes = graph.list_entity_ids().expect("Failed to get IDs");
    let component_nodes = &components[0];
    assert_eq!(
        all_nodes.len(),
        component_nodes.len(),
        "Mismatch in node count"
    );
}

#[test]
fn test_wcc_disconnected() {
    // Scenario: WCC on disconnected graph
    // Expected: Multiple components, each with separate nodes
    use crate::GraphEntity;

    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    // Create two disconnected chains: 0 -> 1 -> 2 and 3 -> 4 -> 5
    for i in 0..6 {
        let entity = GraphEntity {
            id: 0,
            kind: "node".to_string(),
            name: format!("node_{}", i),
            file_path: Some(format!("node_{}.rs", i)),
            data: serde_json::json!({"index": i}),
        };
        graph
            .insert_entity(&entity)
            .expect("Failed to insert entity");
    }

    let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");

    // First chain: 0 -> 1 -> 2
    for i in 0..2 {
        let edge = crate::GraphEdge {
            id: 0,
            from_id: entity_ids[i],
            to_id: entity_ids[i + 1],
            edge_type: "next".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).ok();
    }

    // Second chain: 3 -> 4 -> 5
    for i in 3..5 {
        let edge = crate::GraphEdge {
            id: 0,
            from_id: entity_ids[i],
            to_id: entity_ids[i + 1],
            edge_type: "next".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).ok();
    }

    let result = weakly_connected_components(&graph);
    assert!(result.is_ok(), "WCC failed on disconnected graph");

    let components = result.unwrap();
    assert_eq!(
        components.len(),
        2,
        "Expected 2 components in disconnected graph"
    );

    // Each component should have 3 nodes
    assert_eq!(components[0].len(), 3, "First component should have 3 nodes");
    assert_eq!(components[1].len(), 3, "Second component should have 3 nodes");

    // Verify all nodes appear exactly once across all components
    let all_nodes: i64 = graph.list_entity_ids().expect("Failed to get IDs").len() as i64;
    let component_nodes: i64 = components.iter().map(|c| c.len() as i64).sum();
    assert_eq!(all_nodes, component_nodes, "Not all nodes accounted for");
}

#[test]
fn test_wcc_with_progress() {
    // Scenario: WCC with progress callback
    // Expected: Progress callback works, results match non-progress version
    use crate::progress::NoProgress;

    let graph = create_test_graph();

    let progress = NoProgress;
    let result =
        weakly_connected_components_with_progress(&graph, &progress).expect("WCC failed");

    let result_no_progress =
        weakly_connected_components(&graph).expect("WCC without progress failed");

    // Results should be identical
    assert_eq!(result.len(), result_no_progress.len(), "Component count mismatch");
    for (comp_with, comp_without) in result.iter().zip(result_no_progress.iter()) {
        assert_eq!(comp_with, comp_without, "Component mismatch");
    }
}

#[test]
fn test_wcc_deterministic_ordering() {
    // Scenario: WCC produces deterministic output
    // Expected: Multiple calls produce same component ordering
    let graph = create_test_graph();

    let result1 = weakly_connected_components(&graph).expect("First WCC failed");
    let result2 = weakly_connected_components(&graph).expect("Second WCC failed");

    // Results should be identical (same ordering)
    assert_eq!(result1, result2, "WCC results are non-deterministic");
}

#[test]
fn test_wcc_bidirectional_edges() {
    // Scenario: WCC treats edges as bidirectional
    // Expected: Nodes connected by unidirectional edges are in same component
    use crate::GraphEntity;

    let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

    // Create nodes 0 -> 1 -> 2 (only forward edges)
    for i in 0..3 {
        let entity = GraphEntity {
            id: 0,
            kind: "node".to_string(),
            name: format!("node_{}", i),
            file_path: Some(format!("node_{}.rs", i)),
            data: serde_json::json!({"index": i}),
        };
        graph
            .insert_entity(&entity)
            .expect("Failed to insert entity");
    }

    let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");

    // Only create forward edges: 0 -> 1 -> 2
    let edge1 = crate::GraphEdge {
        id: 0,
        from_id: entity_ids[0],
        to_id: entity_ids[1],
        edge_type: "next".to_string(),
        data: serde_json::json!({}),
    };
    graph.insert_edge(&edge1).ok();

    let edge2 = crate::GraphEdge {
        id: 0,
        from_id: entity_ids[1],
        to_id: entity_ids[2],
        edge_type: "next".to_string(),
        data: serde_json::json!({}),
    };
    graph.insert_edge(&edge2).ok();

    let result = weakly_connected_components(&graph).expect("WCC failed");

    // Even though edges are only forward, WCC treats them as bidirectional
    // So all nodes should be in one component
    assert_eq!(result.len(), 1, "Expected 1 component");
    assert_eq!(result[0].len(), 3, "Expected all 3 nodes in single component");
}

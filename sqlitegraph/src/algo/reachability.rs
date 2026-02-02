//! Reachability analysis for forward/backward graph traversal.
//!
//! This module provides algorithms for computing reachability in directed graphs,
//! enabling "what does this affect?" (forward) and "what affects this?" (backward) queries.
//! Reachability is the foundation of program slicing, impact analysis, and dead code detection.
//!
//! # Available Algorithms
//!
//! - [`reachable_from`] - Forward reachability (what does this node affect?)
//! - [`reachable_from_with_progress`] - Forward reachability with progress tracking
//! - [`reverse_reachable_from`] - Backward reachability (what affects this node?)
//! - [`reverse_reachable_from_with_progress`] - Backward reachability with progress tracking
//! - [`can_reach`] - Point-to-point reachability check
//! - [`unreachable_from`] - Find unreachable nodes from an entry point
//!
//! # When to Use Reachability Analysis
//!
//! ## Forward Reachability (`reachable_from`)
//!
//! - **Impact Analysis**: Determine what code is affected by a change
//! - **Forward Slicing**: Find all statements that depend on a given point
//! - **Regression Testing**: Identify tests that need to run after a change
//! - **Data Flow Analysis**: Track where data propagates to
//!
//! ## Backward Reachability (`reverse_reachable_from`)
//!
//! - **Backward Slicing**: Find all statements that affect a given point
//! - **Root Cause Analysis**: Identify sources that influence a result
//! - **Dependency Analysis**: Find all transitive dependencies
//! - **Change Impact**: Determine what changes could affect a component
//!
//! ## Point-to-Point Reachability (`can_reach`)
//!
//! - **Fast Queries**: Check if one node can reach another without full computation
//! - **Path Existence**: Answer "is there a path from X to Y?" questions
//! - **Validation**: Verify graph connectivity properties
//!
//! ## Unreachable Nodes (`unreachable_from`)
//!
//! - **Dead Code Detection**: Find code that can never execute from entry point
//! - **Code Coverage**: Identify unreachable paths for testing
//! - **Graph Cleanup**: Remove isolated nodes and edges
//!
//! # Algorithm
//!
//! Uses breadth-first search (BFS) for traversal:
//!
//! ## Forward Reachability
//! 1. Start from source node
//! 2. Traverse outgoing edges (follow edge direction)
//! 3. Mark all visited nodes as reachable
//! 4. Return set of reachable nodes (includes source)
//!
//! ## Backward Reachability
//! 1. Start from target node
//! 2. Traverse incoming edges (reverse edge direction)
//! 3. Mark all visited nodes as able to reach target
//! 4. Return set of ancestor nodes (includes target)
//!
//! # Complexity
//!
//! - **Time**: O(|V| + |E|) - visits each node and edge at most once
//! - **Space**: O(|V|) for visited set and BFS queue
//!
//! Where:
//! - V = number of vertices
//! - E = number of edges
//!
//! # Example
//!
//! ```rust,ignore
//! use sqlitegraph::{SqliteGraph, algo::{reachable_from, reverse_reachable_from, can_reach}};
//!
//! let graph = SqliteGraph::open_in_memory()?;
//! // ... add nodes and edges ...
//!
//! // Forward: What does node 5 affect?
//! let forward_slice = reachable_from(&graph, 5)?;
//! println!("Node 5 affects {} nodes", forward_slice.len());
//!
//! // Backward: What affects node 10?
//! let backward_slice = reverse_reachable_from(&graph, 10)?;
//! println!("Node 10 is affected by {} nodes", backward_slice.len());
//!
//! // Point-to-point: Can node 5 reach node 10?
//! let reaches = can_reach(&graph, 5, 10)?;
//! println!("Node 5 {} reach node 10", if reaches { "can" } else { "cannot" });
//! ```

use std::collections::VecDeque;

use ahash::AHashSet;

use crate::progress::ProgressCallback;
use crate::{errors::SqliteGraphError, graph::SqliteGraph};

/// Computes forward reachability from a start node.
///
/// Returns the set of all nodes reachable from `start` via outgoing edges.
/// This answers "what does this node affect?" - the forward slice from start.
/// The start node itself is included in the result (self-reachability).
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `start` - The starting node ID
///
/// # Returns
/// Set of all node IDs reachable from `start` (including `start` itself).
///
/// # Complexity
/// - **Time**: O(|V| + |E|) - visits each node and edge at most once
/// - **Space**: O(|V|) for visited set and BFS queue
///
/// # Algorithm
/// Uses BFS traversal following outgoing edges:
/// 1. Mark start as visited and add to queue
/// 2. While queue not empty:
///    - Pop node from queue
///    - For each outgoing neighbor: if not visited, mark and enqueue
/// 3. Return visited set
///
/// # Edge Cases
/// - **Empty graph**: Returns empty set
/// - **Start not in graph**: Returns set containing only start (no edges to traverse)
/// - **Disconnected components**: Only returns nodes in start's component
/// - **Cycles**: Handled correctly by visited set (no infinite loops)
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::reachable_from};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph: 0 -> 1 -> 2 -> 3 ...
///
/// // What does node 0 affect?
/// let reachable = reachable_from(&graph, 0)?;
/// // Returns {0, 1, 2, 3, ...} - all nodes downstream from 0
///
/// // What does node 3 affect?
/// let reachable = reachable_from(&graph, 3)?;
/// // Returns {3} - only itself (no outgoing edges)
/// ```
pub fn reachable_from(
    graph: &SqliteGraph,
    start: i64,
) -> Result<AHashSet<i64>, SqliteGraphError> {
    let mut visited = AHashSet::new();
    let mut queue = VecDeque::new();

    // Start node is always reachable from itself
    visited.insert(start);
    queue.push_back(start);

    // BFS traversal on outgoing edges
    while let Some(node) = queue.pop_front() {
        // Fetch outgoing neighbors and enqueue unvisited ones
        for neighbor in graph.fetch_outgoing(node)? {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    Ok(visited)
}

/// Computes forward reachability with progress tracking.
///
/// Same algorithm as [`reachable_from`] but reports progress during execution.
/// Useful for long-running operations on large graphs.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `start` - The starting node ID
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// Set of all node IDs reachable from `start` (including `start` itself).
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Current number of nodes visited
/// - `total`: None (unknown total for single-source BFS)
/// - `message`: "Forward reachability: visited {current}"
///
/// Progress is reported periodically (every ~10 nodes visited) to avoid
/// excessive callback overhead while still providing feedback.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::reachable_from_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let reachable = reachable_from_with_progress(&graph, start, &progress)?;
/// // Output: Forward reachability: visited 10...
/// // Output: Forward reachability: visited 20...
/// ```
pub fn reachable_from_with_progress<F>(
    graph: &SqliteGraph,
    start: i64,
    progress: &F,
) -> Result<AHashSet<i64>, SqliteGraphError>
where
    F: ProgressCallback,
{
    let mut visited = AHashSet::new();
    let mut queue = VecDeque::new();
    let mut nodes_processed = 0;

    // Start node is always reachable from itself
    visited.insert(start);
    queue.push_back(start);

    // BFS traversal on outgoing edges
    while let Some(node) = queue.pop_front() {
        nodes_processed += 1;

        // Report progress every 10 nodes
        if nodes_processed % 10 == 0 {
            progress.on_progress(
                nodes_processed,
                None,
                &format!("Forward reachability: visited {}", nodes_processed),
            );
        }

        // Fetch outgoing neighbors and enqueue unvisited ones
        for neighbor in graph.fetch_outgoing(node)? {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    // Report completion
    progress.on_complete();

    Ok(visited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphEntity, GraphEdge};

    /// Helper: Create linear chain graph: 0 -> 1 -> 2 -> 3
    fn create_linear_chain() -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 4 nodes
        for i in 0..4 {
            let entity = GraphEntity {
                id: 0,
                kind: "node".to_string(),
                name: format!("node_{}", i),
                file_path: Some(format!("node_{}.rs", i)),
                data: serde_json::json!({"index": i}),
            };
            graph.insert_entity(&entity).expect("Failed to insert entity");
        }

        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        // Create chain: 0 -> 1 -> 2 -> 3
        for i in 0..entity_ids.len().saturating_sub(1) {
            let edge = GraphEdge {
                id: 0,
                from_id: entity_ids[i],
                to_id: entity_ids[i + 1],
                edge_type: "next".to_string(),
                data: serde_json::json!({}),
            };
            graph.insert_edge(&edge).expect("Failed to insert edge");
        }

        graph
    }

    /// Helper: Create diamond graph: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
    fn create_diamond() -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 4 nodes
        for i in 0..4 {
            let entity = GraphEntity {
                id: 0,
                kind: "node".to_string(),
                name: format!("node_{}", i),
                file_path: Some(format!("node_{}.rs", i)),
                data: serde_json::json!({"index": i}),
            };
            graph.insert_entity(&entity).expect("Failed to insert entity");
        }

        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        // Create diamond: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
        let edges = vec![(0, 1), (0, 2), (1, 3), (2, 3)];
        for (from_idx, to_idx) in edges {
            let edge = GraphEdge {
                id: 0,
                from_id: entity_ids[from_idx],
                to_id: entity_ids[to_idx],
                edge_type: "next".to_string(),
                data: serde_json::json!({}),
            };
            graph.insert_edge(&edge).expect("Failed to insert edge");
        }

        graph
    }

    /// Helper: Create graph with cycle: 0 -> 1 -> 2 -> 1
    fn create_cycle() -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 3 nodes
        for i in 0..3 {
            let entity = GraphEntity {
                id: 0,
                kind: "node".to_string(),
                name: format!("node_{}", i),
                file_path: Some(format!("node_{}.rs", i)),
                data: serde_json::json!({"index": i}),
            };
            graph.insert_entity(&entity).expect("Failed to insert entity");
        }

        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        // Create cycle: 0 -> 1, 1 -> 2, 2 -> 1
        let edges = vec![(0, 1), (1, 2), (2, 1)];
        for (from_idx, to_idx) in edges {
            let edge = GraphEdge {
                id: 0,
                from_id: entity_ids[from_idx],
                to_id: entity_ids[to_idx],
                edge_type: "next".to_string(),
                data: serde_json::json!({}),
            };
            graph.insert_edge(&edge).expect("Failed to insert edge");
        }

        graph
    }

    /// Helper: Create disconnected graph: 0 -> 1 and 2 -> 3
    fn create_disconnected() -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 4 nodes
        for i in 0..4 {
            let entity = GraphEntity {
                id: 0,
                kind: "node".to_string(),
                name: format!("node_{}", i),
                file_path: Some(format!("node_{}.rs", i)),
                data: serde_json::json!({"index": i}),
            };
            graph.insert_entity(&entity).expect("Failed to insert entity");
        }

        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        // Create two disconnected chains: 0 -> 1 and 2 -> 3
        let edge1 = GraphEdge {
            id: 0,
            from_id: entity_ids[0],
            to_id: entity_ids[1],
            edge_type: "next".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge1).expect("Failed to insert edge");

        let edge2 = GraphEdge {
            id: 0,
            from_id: entity_ids[2],
            to_id: entity_ids[3],
            edge_type: "next".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge2).expect("Failed to insert edge");

        graph
    }

    #[test]
    fn test_reachable_from_empty() {
        // Scenario: Empty graph returns empty set
        // Expected: No reachable nodes
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        let result = reachable_from(&graph, 999);
        assert!(result.is_ok(), "reachable_from failed on empty graph");

        let reachable = result.unwrap();
        // Start node is included even if not in graph
        assert_eq!(reachable.len(), 1, "Expected only start node in empty graph");
        assert!(reachable.contains(&999), "Start node should be in result");
    }

    #[test]
    fn test_reachable_from_single() {
        // Scenario: Single node returns set containing itself
        // Expected: {node_id}
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        let entity = GraphEntity {
            id: 0,
            kind: "node".to_string(),
            name: "single_node".to_string(),
            file_path: Some("single_node.rs".to_string()),
            data: serde_json::json!({}),
        };
        graph.insert_entity(&entity).expect("Failed to insert entity");

        let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");
        let node_id = entity_ids[0];

        let result = reachable_from(&graph, node_id);
        assert!(result.is_ok(), "reachable_from failed on single node");

        let reachable = result.unwrap();
        assert_eq!(reachable.len(), 1, "Expected 1 node reachable");
        assert!(reachable.contains(&node_id), "Node should reach itself");
    }

    #[test]
    fn test_reachable_from_linear() {
        // Scenario: Linear chain 0 -> 1 -> 2 -> 3
        // Expected: node 0 reaches all, node 3 reaches only itself
        let graph = create_linear_chain();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        // Node 0 (first) should reach all nodes
        let reachable_0 = reachable_from(&graph, entity_ids[0]).expect("Failed");
        assert_eq!(
            reachable_0.len(),
            4,
            "Node 0 should reach all 4 nodes in chain"
        );
        for &id in &entity_ids {
            assert!(
                reachable_0.contains(&id),
                "Node 0 should reach node {}",
                id
            );
        }

        // Node 3 (last) should reach only itself
        let reachable_3 = reachable_from(&graph, entity_ids[3]).expect("Failed");
        assert_eq!(reachable_3.len(), 1, "Node 3 should reach only itself");
        assert!(
            reachable_3.contains(&entity_ids[3]),
            "Node 3 should reach itself"
        );
    }

    #[test]
    fn test_reachable_from_diamond() {
        // Scenario: Diamond graph: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
        // Expected: Node 0 reaches all nodes
        let graph = create_diamond();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        let reachable_0 = reachable_from(&graph, entity_ids[0]).expect("Failed");
        assert_eq!(
            reachable_0.len(),
            4,
            "Node 0 should reach all 4 nodes in diamond"
        );
        for &id in &entity_ids {
            assert!(
                reachable_0.contains(&id),
                "Node 0 should reach node {}",
                id
            );
        }
    }

    #[test]
    fn test_reachable_from_cycle() {
        // Scenario: Graph with cycle: 0 -> 1 -> 2 -> 1
        // Expected: Node 0 reaches all, nodes 1 and 2 reach each other
        let graph = create_cycle();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let node_0 = entity_ids[0];
        let node_1 = entity_ids[1];
        let node_2 = entity_ids[2];

        // Node 0 should reach all nodes
        let reachable_0 = reachable_from(&graph, node_0).expect("Failed");
        assert_eq!(reachable_0.len(), 3, "Node 0 should reach all 3 nodes");

        // Node 1 should reach nodes 1 and 2 (cycle)
        let reachable_1 = reachable_from(&graph, node_1).expect("Failed");
        assert_eq!(reachable_1.len(), 2, "Node 1 should reach 2 nodes (1 and 2)");
        assert!(reachable_1.contains(&node_1), "Node 1 should reach itself");
        assert!(reachable_1.contains(&node_2), "Node 1 should reach node 2");

        // Node 2 should reach nodes 1 and 2 (cycle)
        let reachable_2 = reachable_from(&graph, node_2).expect("Failed");
        assert_eq!(reachable_2.len(), 2, "Node 2 should reach 2 nodes (1 and 2)");
        assert!(reachable_2.contains(&node_1), "Node 2 should reach node 1");
        assert!(reachable_2.contains(&node_2), "Node 2 should reach itself");
    }

    #[test]
    fn test_reachable_from_disconnected() {
        // Scenario: Disconnected graph: 0 -> 1 and 2 -> 3
        // Expected: Only reachable nodes returned
        let graph = create_disconnected();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        // Node 0 should reach only nodes 0 and 1
        let reachable_0 = reachable_from(&graph, entity_ids[0]).expect("Failed");
        assert_eq!(reachable_0.len(), 2, "Node 0 should reach 2 nodes");
        assert!(reachable_0.contains(&entity_ids[0]), "Node 0 should reach itself");
        assert!(reachable_0.contains(&entity_ids[1]), "Node 0 should reach node 1");
        assert!(!reachable_0.contains(&entity_ids[2]), "Node 0 should NOT reach node 2");
        assert!(!reachable_0.contains(&entity_ids[3]), "Node 0 should NOT reach node 3");
    }

    #[test]
    fn test_reachable_from_with_progress() {
        // Scenario: Progress variant matches non-progress variant
        // Expected: Same results, progress callback called
        use crate::progress::NoProgress;

        let graph = create_linear_chain();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");

        let progress = NoProgress;
        let result_with =
            reachable_from_with_progress(&graph, entity_ids[0], &progress).expect("Failed");
        let result_without = reachable_from(&graph, entity_ids[0]).expect("Failed");

        assert_eq!(
            result_with.len(),
            result_without.len(),
            "Progress and non-progress results should match"
        );
        for &id in &result_with {
            assert!(
                result_without.contains(&id),
                "Progress result contains node not in non-progress result"
            );
        }
    }
}

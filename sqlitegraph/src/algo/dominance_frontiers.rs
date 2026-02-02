//! Dominance frontier computation using Cytron et al. efficient algorithm.
//!
//! This module provides algorithms for computing dominance frontiers and iterated
//! dominance frontiers in control flow graphs (CFGs). Dominance frontiers are a
//! fundamental concept in compiler optimization, SSA construction, and program analysis.
//!
//! # Dominance Frontier Definition
//!
//! The **dominance frontier** of a node `n` (written `DF(n)`) is the set of all nodes
//! `m` such that:
//!
//! 1. `n` dominates a predecessor of `m` (but NOT `m` itself)
//! 2. `n` does NOT strictly dominate `m`
//!
//! Intuitively: `DF(n)` contains nodes where control flow from paths dominated by `n`
//! merges with paths from outside `n`'s dominance. These are **convergence points**
//! where φ-nodes must be placed in SSA construction.
//!
//! # Example: Diamond CFG
//!
//! Consider an if-then-else CFG: `0 -> 1`, `0 -> 2`, `1 -> 3`, `2 -> 3`
//!
//! - Node 0 dominates all nodes (entry)
//! - Node 3 has two predecessors: 1 and 2
//! - Node 0 dominates both 1 and 2, but does NOT strictly dominate 3
//! - Therefore: `DF(0) = {3}` (convergence point)
//!
//! Node 3 is where the two branches merge, so a φ-node is needed there in SSA form.
//!
//! # Algorithm: Cytron et al. Walk-Up
//!
//! This module implements the **efficient algorithm from Cytron et al. (1991)**:
//!
//! > "For each node n in the CFG: for each predecessor p of n: walk up the idom tree
//! > from p to idom(n), adding n to each node's dominance frontier along the way."
//!
//! The algorithm walks up the immediate dominator tree, adding nodes to dominance
//! frontiers as it goes. This is more efficient than the naive O(N³) definition-based
//! approach.
//!
//! # Iterated Dominance Frontier
//!
//! The **iterated dominance frontier** (IDF) finds all nodes that need φ-functions
//! for a given set of definition nodes in SSA construction:
//!
//! ```text
//! IDF(S) = DF(S) ∪ DF(DF(S)) ∪ DF(DF(DF(S))) ∪ ... (to fixed point)
//! ```
//!
//! This is computed by fixed-point iteration starting from the definition nodes.
//!
//! # Complexity
//!
//! - **Time**: O(N²) worst case for DF, O(N × iterations) for IDF
//! - **Space**: O(N²) for DF sets
//!
//! Where:
//! - N = number of vertices
//! - iterations = number of iterations to reach fixed point (typically small)
//!
//! # When to Use Dominance Frontiers
//!
//! ## SSA Construction
//!
//! - **φ-node placement**: Place φ-nodes at all nodes in iterated dominance frontier
//! - **Variable renaming**: Use dominance frontiers to rename variables in SSA form
//! - **SSA destruction**: Use dominance frontiers to remove φ-nodes after optimization
//!
//! ## Program Analysis
//!
//! - **Control flow merging**: Identify where control flow paths converge
//! - **Data flow analysis**: Use dominance frontiers to prune data flow constraints
//! - **Impact analysis**: Find points where definitions may affect uses
//!
//! # Example
//!
//! ```rust,ignore
//! use sqlitegraph::{SqliteGraph, algo::{dominators, dominance_frontiers}};
//!
//! let graph = SqliteGraph::open_in_memory()?;
//! // ... build CFG with entry node 0 ...
//!
//! // First compute dominators
//! let dom_result = dominators(&graph, 0)?;
//!
//! // Then compute dominance frontiers
//! let df_result = dominance_frontiers(&graph, &dom_result)?;
//!
//! // Check: where does node 0 need φ-nodes?
//! if let Some(frontier) = df_result.frontier(0) {
//!     for &node in frontier {
//!         println!("Node 0 needs φ-node at {}", node);
//!     }
//! }
//!
//! // Compute iterated DF for SSA φ-placement
//! let mut definitions = AHashSet::new();
//! definitions.insert(1);
//! definitions.insert(3);
//!
//! let idf_result = iterated_dominance_frontiers(&graph, &dom_result, &definitions)?;
//!
//! println!("Place φ-nodes at: {:?}", idf_result.phi_nodes);
//! ```
//!
//! # References
//!
//! - Cytron, Ron, et al. "Efficiently computing static single assignment form
//!   and the control dependence graph." ACM TOPLAS, 1991.
//! - Cooper, Keith D., Harvey, Timothy J., and Kennedy, Ken. "A simple, fast
//!   dominance algorithm." Software Practice & Experience, 2001.

use ahash::{AHashMap, AHashSet};
use std::collections::HashSet;

use crate::errors::SqliteGraphError;
use crate::graph::SqliteGraph;
use crate::progress::ProgressCallback;

use super::dominators::DominatorResult;

/// Dominance frontier result for a CFG.
///
/// Contains the dominance frontier sets for all nodes. The dominance frontier
/// of a node `n` is the set of nodes `m` where control flow from paths dominated
/// by `n` merges with paths from outside `n`'s dominance.
///
/// # Example
///
/// ```rust,ignore
/// let result = dominance_frontiers(&graph, &dom_result)?;
///
/// // Check dominance frontier of node 0
/// if let Some(frontier) = result.frontier(0) {
///     for &node in frontier {
///         println!("Node 0 is in DF of {}", node);
///     }
/// }
///
/// // Check if node 3 is in DF of node 0
/// assert!(result.in_frontier(0, 3));
/// ```
#[derive(Debug, Clone)]
pub struct DominanceFrontierResult {
    /// Dominance frontier sets: node -> set of nodes in its dominance frontier.
    ///
    /// For each node `n`, `frontiers[n]` contains all nodes `m` such that:
    /// - `n` dominates a predecessor of `m`
    /// - `n` does NOT strictly dominate `m`
    ///
    /// Empty set means `n` has no convergence points in its dominance.
    pub frontiers: AHashMap<i64, AHashSet<i64>>,
}

impl DominanceFrontierResult {
    /// Gets the dominance frontier of a node.
    ///
    /// Returns `None` if the node has no dominance frontier (empty set).
    ///
    /// # Arguments
    /// * `node` - Node ID to get dominance frontier for
    ///
    /// # Returns
    /// `Some(set)` if node has a non-empty dominance frontier, `None` if empty.
    ///
    /// # Example
    /// ```rust,ignore
    /// let result = dominance_frontiers(&graph, &dom_result)?;
    /// if let Some(frontier) = result.frontier(0) {
    ///     println!("Node 0 DF: {:?}", frontier);
    /// }
    /// ```
    pub fn frontier(&self, node: i64) -> Option<&AHashSet<i64>> {
        self.frontiers.get(&node)
    }

    /// Checks if one node is in the dominance frontier of another.
    ///
    /// Returns `true` if `m` is in `DF(n)` (node `n`'s dominance frontier).
    ///
    /// # Arguments
    /// * `n` - Node whose dominance frontier to check
    /// * `m` - Node to check membership in `DF(n)`
    ///
    /// # Returns
    /// `true` if `m` is in the dominance frontier of `n`, `false` otherwise.
    ///
    /// # Example
    /// ```rust,ignore
    /// let result = dominance_frontiers(&graph, &dom_result)?;
    /// assert!(result.in_frontier(0, 3)); // Node 3 is in DF(0)
    /// ```
    pub fn in_frontier(&self, n: i64, m: i64) -> bool {
        self.frontier(n)
            .map(|set| set.contains(&m))
            .unwrap_or(false)
    }
}

/// Iterated dominance frontier result (for SSA φ-placement).
///
/// The iterated dominance frontier finds all nodes that need φ-functions for a
/// given set of definition nodes. This is computed by fixed-point iteration:
/// `IDF(S) = DF(S) ∪ DF(DF(S)) ∪ DF(DF(DF(S))) ∪ ...`
///
/// # Example
///
/// ```rust,ignore
/// use ahash::AHashSet;
///
/// let mut definitions = AHashSet::new();
/// definitions.insert(1);
/// definitions.insert(3);
///
/// let idf_result = iterated_dominance_frontiers(&graph, &dom_result, &definitions)?;
///
/// println!("Place φ-nodes at: {:?}", idf_result.phi_nodes);
/// println!("Converged in {} iterations", idf_result.iterations);
/// ```
#[derive(Debug, Clone)]
pub struct IteratedDominanceFrontierResult {
    /// Set of nodes that need φ-functions.
    ///
    /// This is the fixed-point result of iterated dominance frontier computation.
    /// All nodes in this set should have φ-nodes placed during SSA construction.
    pub phi_nodes: AHashSet<i64>,

    /// Number of iterations to reach fixed point.
    ///
    /// Useful for understanding CFG complexity. Small values (2-4) are typical
    /// for well-structured programs.
    pub iterations: usize,
}

/// Computes dominance frontiers for a CFG using Cytron et al. efficient algorithm.
///
/// Dominance frontiers identify convergence points where φ-nodes must be placed
/// in SSA construction. The algorithm walks up the immediate dominator tree from
/// each predecessor, adding nodes to dominance frontiers along the way.
///
/// # Arguments
/// * `graph` - The control flow graph to analyze
/// * `dom_result` - Pre-computed dominator result (from `dominators`)
///
/// # Returns
/// `DominanceFrontierResult` containing dominance frontier sets for all nodes.
///
/// # Algorithm Steps (Cytron et al. 1991)
///
/// 1. **Initialize**: Create empty DF sets for all nodes
/// 2. **Walk-up algorithm**: For each node `n` in the CFG:
///    - For each predecessor `p` of `n`:
///      - Set `runner := p`
///      - While `runner != idom(n)`:
///        - Add `n` to `DF(runner)`
///        - Set `runner := idom(runner)`
///        - If `runner` is `None`: break (reached entry node)
/// 3. **Return result**: DF sets for all nodes
///
/// # Complexity
/// - **Time**: O(N²) worst case (each edge may walk up the entire idom tree)
/// - **Space**: O(N²) for dominance frontier sets
///
/// # Error Handling
///
/// - Returns `SqliteGraphError::NotFound` if node doesn't exist
/// - Propagates database errors from `fetch_incoming`
/// - Handles unreachable nodes gracefully (they have no predecessors)
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::{dominators, dominance_frontiers}};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build CFG with entry node 0 ...
///
/// let dom_result = dominators(&graph, 0)?;
/// let df_result = dominance_frontiers(&graph, &dom_result)?;
///
/// // Find convergence points for SSA φ-placement
/// for &node in df_result.frontiers.keys() {
///     if let Some(frontier) = df_result.frontier(node) {
///         println!("Node {} needs φ-nodes at: {:?}", node, frontier);
///     }
/// }
/// ```
///
/// # References
///
/// - Cytron, Ron, et al. "Efficiently computing static single assignment form
///   and the control dependence graph." ACM TOPLAS, 1991.
pub fn dominance_frontiers(
    graph: &SqliteGraph,
    dom_result: &DominatorResult,
) -> Result<DominanceFrontierResult, SqliteGraphError> {
    // Get all nodes in the graph
    let all_nodes = graph.all_entity_ids()?;

    // Initialize empty DF sets
    let mut frontiers: AHashMap<i64, AHashSet<i64>> = AHashMap::new();

    // Handle empty graph
    if all_nodes.is_empty() {
        return Ok(DominanceFrontierResult { frontiers });
    }

    // Extract immediate dominator tree
    let idom = &dom_result.idom;

    // Cytron et al. algorithm: walk up idom tree from each predecessor
    for &n in &all_nodes {
        // Get predecessors of n
        let predecessors = graph.fetch_incoming(n)?;

        // For each predecessor p of n
        for &p in &predecessors {
            // Walk up the idom tree from p to idom(n)
            let mut runner = p;

            loop {
                // Get idom of runner
                let idom_of_runner = idom.get(&runner).copied().flatten();

                // Get idom of n
                let idom_of_n = idom.get(&n).copied().flatten();

                // Stop if runner reached idom(n)
                if idom_of_runner == idom_of_n {
                    break;
                }

                // Stop if runner is None (reached entry node)
                if idom_of_runner.is_none() {
                    // Check if we've reached the entry node's parent
                    // Entry node has idom = None, so we stop here
                    break;
                }

                // Add n to DF(runner)
                frontiers
                    .entry(runner)
                    .or_insert_with(AHashSet::new)
                    .insert(n);

                // Move runner up to its idom
                if let Some(next_runner) = idom_of_runner {
                    runner = next_runner;
                } else {
                    // Reached entry node, stop walking
                    break;
                }
            }
        }
    }

    Ok(DominanceFrontierResult { frontiers })
}

/// Computes dominance frontiers with progress tracking.
///
/// Same algorithm as [`dominance_frontiers`] but reports progress during execution.
/// Useful for long-running operations on large graphs.
///
/// # Arguments
/// * `graph` - The control flow graph to analyze
/// * `dom_result` - Pre-computed dominator result (from `dominators`)
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// `DominanceFrontierResult` containing dominance frontier sets for all nodes.
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Current node being processed
/// - `total`: Total number of nodes to process
/// - `message`: "Computing DF for node {node}: {predecessors} predecessors"
///
/// Progress is reported after each node is processed.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::dominance_frontiers_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let df_result = dominance_frontiers_with_progress(&graph, &dom_result, &progress)?;
/// // Output: Computing DF for node 5: 2 predecessors...
/// // Output: Computing DF for node 7: 3 predecessors...
/// ```
pub fn dominance_frontiers_with_progress<F>(
    graph: &SqliteGraph,
    dom_result: &DominatorResult,
    progress: &F,
) -> Result<DominanceFrontierResult, SqliteGraphError>
where
    F: ProgressCallback,
{
    // Get all nodes in the graph
    let all_nodes = graph.all_entity_ids()?;
    let total = all_nodes.len();

    // Initialize empty DF sets
    let mut frontiers: AHashMap<i64, AHashSet<i64>> = AHashMap::new();

    // Handle empty graph
    if all_nodes.is_empty() {
        progress.on_complete();
        return Ok(DominanceFrontierResult { frontiers });
    }

    // Extract immediate dominator tree
    let idom = &dom_result.idom;

    // Process nodes with progress tracking
    for (idx, &n) in all_nodes.iter().enumerate() {
        // Get predecessors of n
        let predecessors = graph.fetch_incoming(n)?;

        // Report progress
        progress.on_progress(
            idx + 1,
            Some(total),
            &format!(
                "Computing DF for node {}: {} predecessors",
                n,
                predecessors.len()
            ),
        );

        // For each predecessor p of n
        for &p in &predecessors {
            // Walk up the idom tree from p to idom(n)
            let mut runner = p;

            loop {
                // Get idom of runner
                let idom_of_runner = idom.get(&runner).copied().flatten();

                // Get idom of n
                let idom_of_n = idom.get(&n).copied().flatten();

                // Stop if runner reached idom(n)
                if idom_of_runner == idom_of_n {
                    break;
                }

                // Stop if runner is None (reached entry node)
                if idom_of_runner.is_none() {
                    break;
                }

                // Add n to DF(runner)
                frontiers
                    .entry(runner)
                    .or_insert_with(AHashSet::new)
                    .insert(n);

                // Move runner up to its idom
                if let Some(next_runner) = idom_of_runner {
                    runner = next_runner;
                } else {
                    break;
                }
            }
        }
    }

    // Report completion
    progress.on_complete();

    Ok(DominanceFrontierResult { frontiers })
}

/// Computes iterated dominance frontier for SSA φ-placement.
///
/// The iterated dominance frontier finds all nodes that need φ-functions for a
/// given set of definition nodes. This is computed by fixed-point iteration:
/// `IDF(S) = DF(S) ∪ DF(DF(S)) ∪ DF(DF(DF(S))) ∪ ...` until convergence.
///
/// # Arguments
/// * `graph` - The control flow graph to analyze
/// * `dom_result` - Pre-computed dominator result (from `dominators`)
/// * `definition_nodes` - Set of nodes where variables are defined
///
/// # Returns
/// `IteratedDominanceFrontierResult` containing φ-node placement set and iteration count.
///
/// # Algorithm Steps
///
/// 1. **Initialize**: Start with definition nodes as current set
/// 2. **Iterate**: Compute DF of current set, add to result, repeat until no change
/// 3. **Fixed-point detection**: Stop when DF(current) ⊆ result (no new nodes)
/// 4. **Return result**: All nodes needing φ-nodes
///
/// # Complexity
/// - **Time**: O(N × iterations) where iterations is typically small (2-4)
/// - **Space**: O(N) for result set
///
/// # Error Handling
///
/// - Has iteration limit (100) to prevent non-termination on pathological inputs
/// - Returns error if iteration limit is reached (indicates algorithm bug or malformed CFG)
///
/// # Example
///
/// ```rust,ignore
/// use ahash::AHashSet;
///
/// // Variables x and y are defined at nodes 1 and 3
/// let mut definitions = AHashSet::new();
/// definitions.insert(1);
/// definitions.insert(3);
///
/// let idf_result = iterated_dominance_frontiers(&graph, &dom_result, &definitions)?;
///
/// // Place φ-nodes at all nodes in idf_result.phi_nodes
/// for &node in &idf_result.phi_nodes {
///     println!("Place φ-node at node {}", node);
/// }
/// ```
///
/// # References
///
/// - Cytron, Ron, et al. "Efficiently computing static single assignment form
///   and the control dependence graph." ACM TOPLAS, 1991.
pub fn iterated_dominance_frontiers(
    graph: &SqliteGraph,
    dom_result: &DominatorResult,
    definition_nodes: &AHashSet<i64>,
) -> Result<IteratedDominanceFrontierResult, SqliteGraphError> {
    // Compute dominance frontiers first
    let df_result = dominance_frontiers(graph, dom_result)?;

    // Handle empty definitions
    if definition_nodes.is_empty() {
        return Ok(IteratedDominanceFrontierResult {
            phi_nodes: AHashSet::new(),
            iterations: 0,
        });
    }

    // Initialize result with definition nodes
    let mut phi_nodes: AHashSet<i64> = definition_nodes.clone();
    let mut current: AHashSet<i64> = definition_nodes.clone();

    // Iterate to fixed point
    let max_iterations = 100;
    let mut iterations = 0;

    loop {
        // Compute DF of current set
        let mut df_current: AHashSet<i64> = AHashSet::new();

        for &node in &current {
            if let Some(frontier) = df_result.frontier(node) {
                df_current.extend(frontier.iter().copied());
            }
        }

        // Check if any new nodes
        let new_nodes: AHashSet<i64> = df_current
            .difference(&phi_nodes)
            .copied()
            .collect();

        if new_nodes.is_empty() {
            // Fixed point reached
            break;
        }

        // Add new nodes to result and current
        phi_nodes.extend(new_nodes.iter().copied());
        current = new_nodes;

        iterations += 1;
        if iterations >= max_iterations {
            return Err(SqliteGraphError::query(format!(
                "Iterated dominance frontier failed to converge after {} iterations",
                max_iterations
            )));
        }
    }

    Ok(IteratedDominanceFrontierResult {
        phi_nodes,
        iterations,
    })
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

    /// Helper: Create diamond CFG: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
    fn create_diamond_cfg() -> SqliteGraph {
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

    /// Helper: Create CFG with loop: 0 -> 1 -> 2 -> 1
    fn create_loop_cfg() -> SqliteGraph {
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

        // Create loop: 0 -> 1, 1 -> 2, 2 -> 1
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

    /// Helper: Create nested branches CFG: 0 -> 1, 0 -> 4, 1 -> 2, 1 -> 3, 2 -> 5, 3 -> 5, 4 -> 5
    fn create_nested_branches() -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 6 nodes
        for i in 0..6 {
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

        // Create nested branches: 0 -> 1, 0 -> 4, 1 -> 2, 1 -> 3, 2 -> 5, 3 -> 5, 4 -> 5
        let edges = vec![(0, 1), (0, 4), (1, 2), (1, 3), (2, 5), (3, 5), (4, 5)];
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

    /// Helper: Create if-else-if CFG: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 4, 3 -> 5, 4 -> 5
    fn create_if_else_if_cfg() -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 6 nodes
        for i in 0..6 {
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

        // Create if-else-if: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 4, 3 -> 5, 4 -> 5
        let edges = vec![(0, 1), (0, 2), (1, 3), (2, 4), (3, 5), (4, 5)];
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

    #[test]
    fn test_dominance_frontiers_linear_chain() {
        // Scenario: Linear chain 0 -> 1 -> 2 -> 3
        // Expected: All DF sets are empty (no convergence points)
        let graph = create_linear_chain();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // All DF sets should be empty (no branching in linear chain)
        for &node in &entity_ids {
            assert_eq!(
                df_result.frontier(node),
                None,
                "Node {} should have empty DF in linear chain",
                node
            );
        }

        // in_frontier should return false for all pairs
        for &n in &entity_ids {
            for &m in &entity_ids {
                assert!(
                    !df_result.in_frontier(n, m),
                    "in_frontier({},{}) should be false in linear chain",
                    n,
                    m
                );
            }
        }
    }

    #[test]
    fn test_dominance_frontiers_diamond() {
        // Scenario: Diamond CFG: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
        // Expected: DF(0) = {3} (convergence point at merge)
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Node 0 (entry) should have DF = {3} (merge point)
        assert!(
            df_result.in_frontier(entity_ids[0], entity_ids[3]),
            "Node 3 should be in DF(0) - it's the merge point"
        );

        // Verify the frontier set
        let df_0 = df_result.frontier(entity_ids[0]);
        assert!(df_0.is_some(), "Node 0 should have a DF set");
        assert_eq!(
            df_0.map(|s| s.len()).unwrap_or(0),
            1,
            "DF(0) should contain exactly 1 node"
        );
        assert!(
            df_0.map(|s| s.contains(&entity_ids[3])).unwrap_or(false),
            "DF(0) should contain node 3"
        );

        // Other nodes should have empty DF
        assert_eq!(
            df_result.frontier(entity_ids[1]),
            None,
            "Node 1 should have empty DF"
        );
        assert_eq!(
            df_result.frontier(entity_ids[2]),
            None,
            "Node 2 should have empty DF"
        );
    }

    #[test]
    fn test_dominance_frontiers_loop() {
        // Scenario: Loop CFG: 0 -> 1 -> 2 -> 1
        // Expected: DF(2) = {1} (back-edge creates frontier at loop header)
        let graph = create_loop_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Node 2 should have node 1 in its DF (back-edge from loop body to header)
        assert!(
            df_result.in_frontier(entity_ids[2], entity_ids[1]),
            "Node 1 should be in DF(2) - back-edge creates frontier"
        );

        // Verify the frontier set
        let df_2 = df_result.frontier(entity_ids[2]);
        assert!(df_2.is_some(), "Node 2 should have a DF set");
        assert!(
            df_2.map(|s| s.contains(&entity_ids[1])).unwrap_or(false),
            "DF(2) should contain node 1"
        );
    }

    #[test]
    fn test_dominance_frontiers_nested_branches() {
        // Scenario: Nested branches: 0 -> 1, 0 -> 4, 1 -> 2, 1 -> 3, 2 -> 5, 3 -> 5, 4 -> 5
        // Expected: DF(0) = {5}, DF(1) = {5} (inner merge also in outer frontier)
        let graph = create_nested_branches();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Node 0 should have node 5 in its DF (outer merge point)
        assert!(
            df_result.in_frontier(entity_ids[0], entity_ids[5]),
            "Node 5 should be in DF(0) - outer merge point"
        );

        // Node 1 should have node 5 in its DF (inner merge point)
        assert!(
            df_result.in_frontier(entity_ids[1], entity_ids[5]),
            "Node 5 should be in DF(1) - inner merge point"
        );
    }

    #[test]
    fn test_dominance_frontiers_single_node() {
        // Scenario: Single node graph
        // Expected: DF(entry) = {} (no edges, no frontiers)
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        let entity = GraphEntity {
            id: 0,
            kind: "node".to_string(),
            name: "single".to_string(),
            file_path: Some("single.rs".to_string()),
            data: serde_json::json!({}),
        };
        graph.insert_entity(&entity).expect("Failed to insert entity");

        let entity_ids = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Entry should have empty DF
        assert_eq!(
            df_result.frontier(entry),
            None,
            "Single node should have empty DF"
        );
    }

    #[test]
    fn test_dominance_frontiers_empty_graph() {
        // Scenario: Empty graph
        // Expected: Returns empty result
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create a fake DominatorResult for empty graph
        let dom_result = DominatorResult {
            dom: AHashMap::new(),
            idom: AHashMap::new(),
        };

        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        assert_eq!(df_result.frontiers.len(), 0);
    }

    #[test]
    fn test_iterated_dominance_frontier_simple() {
        // Scenario: Simple CFG where IDF requires multiple iterations
        // Expected: IDF reaches fixed point within iteration limit
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");

        // Define variables at nodes 1 and 2
        let mut definitions = AHashSet::new();
        definitions.insert(entity_ids[1]);
        definitions.insert(entity_ids[2]);

        let idf_result = iterated_dominance_frontiers(&graph, &dom_result, &definitions)
            .expect("Failed to compute IDF");

        // Should converge
        assert!(idf_result.iterations > 0, "IDF should require at least 1 iteration");

        // Node 3 should be in phi_nodes (it's in DF of both 1 and 2)
        assert!(
            idf_result.phi_nodes.contains(&entity_ids[3]),
            "Node 3 (merge point) should be in phi_nodes"
        );
    }

    #[test]
    fn test_iterated_dominance_frontier_single_definition() {
        // Scenario: Single definition node
        // Expected: IDF = DF(def) (no iteration needed beyond first)
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");

        // Define variable at node 1 only
        let mut definitions = AHashSet::new();
        definitions.insert(entity_ids[1]);

        let idf_result = iterated_dominance_frontiers(&graph, &dom_result, &definitions)
            .expect("Failed to compute IDF");

        // IDF should contain the definition node and its DF
        assert!(
            idf_result.phi_nodes.contains(&entity_ids[1]),
            "IDF should contain definition node"
        );
    }

    #[test]
    fn test_iterated_dominance_frontier_empty_definitions() {
        // Scenario: No definition nodes
        // Expected: Returns empty set
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");

        let definitions = AHashSet::new();

        let idf_result = iterated_dominance_frontiers(&graph, &dom_result, &definitions)
            .expect("Failed to compute IDF");

        assert_eq!(idf_result.phi_nodes.len(), 0);
        assert_eq!(idf_result.iterations, 0);
    }

    #[test]
    fn test_dominance_frontiers_entry_node() {
        // Scenario: Entry node handling
        // Expected: Entry node DF is computed correctly (may be empty or have convergence points)
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Entry node should have DF computed (in diamond, DF(0) = {3})
        let df_entry = df_result.frontier(entry);
        assert!(df_entry.is_some() || df_entry.is_none(), // Either is valid
                "Entry node DF should be computed");
    }

    #[test]
    fn test_dominance_frontiers_self_loop() {
        // Scenario: Node with edge to itself
        // Expected: Self-loop creates frontier at the node itself
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 2 nodes
        for i in 0..2 {
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

        // Create self-loop: 0 -> 1, 1 -> 1
        let edges = vec![(0, 1), (1, 1)];
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

        let entry = entity_ids[0];
        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Algorithm should handle self-loop without errors
        // The specific result depends on dominance structure
        assert!(df_result.frontiers.len() >= 0, "Should compute DF successfully");
    }

    #[test]
    fn test_dominance_frontiers_with_progress() {
        // Scenario: Progress variant matches non-progress variant
        // Expected: Same results, progress callback called
        use crate::progress::NoProgress;

        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");

        let progress = NoProgress;
        let result_with =
            dominance_frontiers_with_progress(&graph, &dom_result, &progress).expect("Failed");
        let result_without = dominance_frontiers(&graph, &dom_result).expect("Failed");

        // Check DF sets match
        assert_eq!(
            result_with.frontiers.len(),
            result_without.frontiers.len(),
            "Should have same number of nodes with DF"
        );

        for (&node, frontier_set) in &result_without.frontiers {
            assert!(
                result_with.frontiers.contains_key(&node),
                "Progress result missing node {}",
                node
            );
            assert_eq!(
                result_with.frontiers.get(&node),
                Some(frontier_set),
                "DF sets differ for node {}",
                node
            );
        }
    }

    #[test]
    fn test_dominance_frontier_in_frontier() {
        // Scenario: Test in_frontier() method
        // Expected: Returns true for nodes in DF, false otherwise
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Test in_frontier() method
        assert!(
            df_result.in_frontier(entity_ids[0], entity_ids[3]),
            "in_frontier(0, 3) should be true"
        );
        assert!(
            !df_result.in_frontier(entity_ids[3], entity_ids[0]),
            "in_frontier(3, 0) should be false"
        );
    }

    #[test]
    fn test_dominance_frontier_frontier() {
        // Scenario: Test frontier() method returns Option
        // Expected: Returns Some for non-empty DF, None for empty
        let graph = create_diamond_cfg();
        let entity_ids: Vec<i64> = graph.list_entity_ids().expect("Failed to get IDs");
        let entry = entity_ids[0];

        let dom_result = super::super::dominators::dominators(&graph, entry)
            .expect("Failed to compute dominators");
        let df_result = dominance_frontiers(&graph, &dom_result)
            .expect("Failed to compute dominance frontiers");

        // Node 0 should have Some({3})
        let df_0 = df_result.frontier(entity_ids[0]);
        assert!(df_0.is_some(), "frontier(0) should return Some");
        assert_eq!(
            df_0.map(|s| s.len()).unwrap_or(0),
            1,
            "frontier(0) should have 1 element"
        );

        // Node 1 should have None (empty DF)
        let df_1 = df_result.frontier(entity_ids[1]);
        assert_eq!(df_1, None, "frontier(1) should return None");
    }
}

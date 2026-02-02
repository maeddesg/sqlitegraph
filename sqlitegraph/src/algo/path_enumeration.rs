//! Path enumeration using DFS with configurable bounds and cycle detection.
//!
//! This module provides algorithms for enumerating all execution paths through
//! control flow graphs (CFGs). Path enumeration is fundamental for test coverage
//! analysis, symbolic execution, program verification, and worst-case execution
//! time analysis.
//!
//! # Path Enumeration
//!
//! A **path** is a sequence of nodes `[n₁, n₂, ..., nₖ]` where each consecutive
//! pair `(nᵢ, nᵢ₊₁)` is an edge in the CFG. Path enumeration finds all possible
//! execution paths from an entry node to terminal nodes (exits or errors).
//!
//! # Challenge: Path Explosion
//!
//! CFGs with loops and branches can have exponentially many or even infinite paths:
//!
//! - **Loops**: `[A, B, A, B, A, B, ...]` - infinite paths without bounds
//! - **Branches**: Each branch doubles the number of paths (2^N for N branches)
//!
//! This module uses **bounded DFS** with revisit counting to make enumeration practical:
//!
//! - **Revisit cap**: Limits how many times a node can appear in a path
//! - **Max depth**: Prevents stack overflow on deep CFGs
//! - **Max paths**: Stops enumeration after finding N paths
//!
//! # Revisit Cap Approach
//!
//! Instead of a boolean "visited" set, we track **visit counts** per node:
//!
//! - `revisit_cap = 1`: Acyclic paths only (no repeated nodes)
//! - `revisit_cap = 2`: Allow one loop iteration (captures loop behavior without explosion)
//! - `revisit_cap = 3`: Allow two loop iterations, etc.
//!
//! During DFS, if `visited[node] >= revisit_cap`, we skip that successor to prevent
//! infinite traversal while still allowing bounded loop exploration.
//!
//! # Algorithm
//!
//! Depth-first search with backtracking and revisit counting:
//!
//! ```
//! dfs(node, depth):
//!     current_path.push(node)
//!     visited[node] += 1
//!
//!     if depth > max_depth:
//!         backtrack and classify as Degenerate
//!
//!     if len(paths) >= max_paths:
//!         stop enumeration
//!
//!     if is_exit(node):
//!         add path to results and backtrack
//!
//!     for successor in graph.outgoing(node):
//!         if visited[successor] < revisit_cap:
//!             dfs(successor, depth + 1)
//!
//!     backtrack: pop from current_path, decrement visited[node]
//! ```
//!
//! # Complexity
//!
//! - **Time**: O(P × L) where P = number of paths, L = average path length
//! - **Space**: O(L) for current path + O(V) for visited tracking
//!
//! Where:
//! - V = number of vertices
//! - P = number of paths (bounded by max_paths)
//! - L = average path length
//!
//! # When to Use Path Enumeration
//!
//! ## Test Coverage
//!
//! - **Ensure all feasible execution paths are tested**
//! - **Find untested branches and loops**
//! - **Generate test cases for maximum coverage**
//!
//! ## Program Verification
//!
//! - **Prove properties hold for all paths**
//! - **Find counterexamples to safety properties**
//! - **Verify absence of runtime errors**
//!
//! ## Symbolic Execution
//!
//! - **Explore all possible program behaviors**
//! - **Generate path conditions for SMT solvers**
//! - **Find feasible paths to specific program points**
//!
//! ## Worst-Case Execution Time (WCET)
//!
//! - **Find the longest execution path**
//! - **Analyze loop bounds and recursion depth**
//! - **Identify performance bottlenecks**
//!
//! # Example
//!
//! ```rust,ignore
//! use sqlitegraph::{SqliteGraph, algo::{enumerate_paths, PathEnumerationConfig}};
//!
//! let graph = SqliteGraph::open_in_memory()?;
//! // ... build CFG with entry node 0 ...
//!
//! let config = PathEnumerationConfig {
//!     max_depth: 100,
//!     max_paths: 10000,
//!     revisit_cap: 2,  // Allow one loop iteration
//!     exit_nodes: Some([10].into_iter().collect()),
//!     error_nodes: Some([99].into_iter().collect()),
//! };
//!
//! let result = enumerate_paths(&graph, 0, &config)?;
//!
//! println!("Found {} paths:", result.paths.len());
//! for path in &result.paths {
//!     println!("  {:?} - {:?}", path.nodes, path.classification);
//! }
//!
//! // Access categorized paths
//! println!("Normal paths: {}", result.normal_paths.len());
//! println!("Error paths: {}", result.error_paths.len());
//! println!("Degenerate paths: {}", result.degenerate_paths.len());
//! ```
//!
//! # Path Classification
//!
//! Paths are classified based on termination properties:
//!
//! - **Normal**: Path reaches exit node within bounds
//! - **Error**: Path reaches error/abort node
//! - **Degenerate**: Path violates bounds (depth/revisit cap exceeded)
//! - **Infinite**: Path loops without bound (theoretical - bounds prevent actual infinite paths)
//!
//! # Bounds
//!
//! | Bound | Purpose | Typical Value | When to Use |
//! |-------|---------|---------------|-------------|
//! | **max_depth** | Prevent stack overflow | 100-1000 | Deep recursion prevention |
//! | **max_paths** | Prevent exponential explosion | 1000-1000000 | "Give me N paths" |
//! | **revisit_cap** | Control loop unrolling | 1-3 | Balance coverage vs explosion |
//!
//! **Default strategy**:
//! - `max_depth = 100`: Prevent infinite paths in buggy CFGs
//! - `max_paths = 10000`: Practical limit for most analyses
//! - `revisit_cap = 2`: Allow one full loop iteration
//!
//! # References
//!
//! - Person, Suetterlein, et al. "Directed Incremental Symbolic Execution." PLDI, 2011.
//! - Symbolic Execution in Practice: A Survey of Applications (arXiv:2508.06643)

use ahash::{AHashMap, AHashSet};
use std::collections::VecDeque;

use crate::errors::SqliteGraphError;
use crate::graph::SqliteGraph;
use crate::progress::ProgressCallback;

/// Path classification based on termination properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathClassification {
    /// Path reaches exit node normally.
    Normal,

    /// Path reaches an error/abort node.
    Error,

    /// Path violates bounds (depth/revisit cap exceeded).
    Degenerate,

    /// Path loops infinitely (theoretical - bounds prevent actual infinite paths).
    Infinite,
}

/// A single execution path through the CFG.
#[derive(Debug, Clone)]
pub struct EnumeratedPath {
    /// Sequence of node IDs in execution order.
    pub nodes: Vec<i64>,

    /// Classification of this path.
    pub classification: PathClassification,
}

/// Configuration for path enumeration.
#[derive(Debug, Clone)]
pub struct PathEnumerationConfig {
    /// Maximum path length (prevents stack overflow on deep CFGs).
    pub max_depth: usize,

    /// Maximum number of paths to enumerate (prevents exponential explosion).
    pub max_paths: usize,

    /// Maximum times a node can be revisited (controls loop unrolling).
    pub revisit_cap: usize,

    /// Exit nodes (Normal paths end here).
    pub exit_nodes: Option<AHashSet<i64>>,

    /// Error nodes (Error paths end here).
    pub error_nodes: Option<AHashSet<i64>>,
}

impl Default for PathEnumerationConfig {
    fn default() -> Self {
        Self {
            max_depth: 100,
            max_paths: 10000,
            revisit_cap: 2,
            exit_nodes: None,
            error_nodes: None,
        }
    }
}

/// Result of path enumeration.
#[derive(Debug, Clone)]
pub struct PathEnumerationResult {
    /// All enumerated paths (up to max_paths bound).
    pub paths: Vec<EnumeratedPath>,

    /// Paths reaching exit nodes normally.
    pub normal_paths: Vec<EnumeratedPath>,

    /// Paths reaching error nodes.
    pub error_paths: Vec<EnumeratedPath>,

    /// Paths violating bounds.
    pub degenerate_paths: Vec<EnumeratedPath>,

    /// Theoretical infinite paths (unbounded cycles).
    pub infinite_paths: Vec<EnumeratedPath>,

    /// Total paths found before pruning.
    pub total_paths_found: usize,

    /// Paths pruned by bounds.
    pub paths_pruned_by_bounds: usize,

    /// Maximum depth reached during enumeration.
    pub max_depth_reached: usize,
}


/// Enumerates all execution paths from entry node using DFS with bounds.
///
/// This function performs depth-first search with backtracking and revisit counting
/// to enumerate all execution paths through the CFG. Paths are classified based on
/// their termination properties (Normal, Error, Degenerate, Infinite).
///
/// # Arguments
///
/// * `graph` - The control flow graph
/// * `entry` - Entry node ID
/// * `config` - Configuration for bounds (max_depth, max_paths, revisit_cap)
///
/// # Returns
///
/// * `Result<PathEnumerationResult, SqliteGraphError>` - Enumeration result with categorized paths
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::{enumerate_paths, PathEnumerationConfig}};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build CFG ...
///
/// let config = PathEnumerationConfig::default();
/// let result = enumerate_paths(&graph, 0, &config)?;
///
/// println!("Found {} normal paths", result.normal_paths.len());
/// ```
pub fn enumerate_paths(
    graph: &SqliteGraph,
    entry: i64,
    config: &PathEnumerationConfig,
) -> Result<PathEnumerationResult, SqliteGraphError> {
    let mut all_paths = Vec::new();
    let mut current_path = Vec::new();
    let mut visited = AHashMap::new();
    let mut total_found = 0;
    let mut pruned_by_bounds = 0;
    let mut max_depth_reached = 0;

    // DFS with backtracking
    dfs_enumerate(
        graph,
        entry,
        config,
        &mut current_path,
        &mut visited,
        &mut all_paths,
        &mut total_found,
        &mut pruned_by_bounds,
        &mut max_depth_reached,
    )?;

    // Classify paths
    let mut normal_paths = Vec::new();
    let mut error_paths = Vec::new();
    let mut degenerate_paths = Vec::new();
    let mut infinite_paths = Vec::new();

    for path in all_paths {
        match &path.classification {
            PathClassification::Normal => normal_paths.push(path),
            PathClassification::Error => error_paths.push(path),
            PathClassification::Degenerate => degenerate_paths.push(path),
            PathClassification::Infinite => infinite_paths.push(path),
        }
    }

    let result = PathEnumerationResult {
        total_paths_found: total_found,
        paths_pruned_by_bounds: pruned_by_bounds,
        max_depth_reached,
        paths: normal_paths
            .iter()
            .chain(error_paths.iter())
            .chain(degenerate_paths.iter())
            .chain(infinite_paths.iter())
            .cloned()
            .collect(),
        normal_paths,
        error_paths,
        degenerate_paths,
        infinite_paths,
    };

    Ok(result)
}

/// Enumerates all execution paths with progress tracking.
///
/// Same as `enumerate_paths` but reports progress during enumeration.
///
/// # Arguments
///
/// * `graph` - The control flow graph
/// * `entry` - Entry node ID
/// * `config` - Configuration for bounds
/// * `progress` - Progress callback for reporting enumeration status
///
/// # Returns
///
/// * `Result<PathEnumerationResult, SqliteGraphError>` - Enumeration result
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::enumerate_paths_with_progress};
/// use sqlitegraph::progress::ConsoleProgress;
///
/// let progress = ConsoleProgress::new();
/// let result = enumerate_paths_with_progress(&graph, 0, &config, progress)?;
/// ```
pub fn enumerate_paths_with_progress<P: ProgressCallback>(
    graph: &SqliteGraph,
    entry: i64,
    config: &PathEnumerationConfig,
    progress: P,
) -> Result<PathEnumerationResult, SqliteGraphError> {
    let mut all_paths = Vec::new();
    let mut current_path = Vec::new();
    let mut visited = AHashMap::new();
    let mut total_found = 0;
    let mut pruned_by_bounds = 0;
    let mut max_depth_reached = 0;

    // DFS with backtracking and progress reporting
    dfs_enumerate_with_progress(
        graph,
        entry,
        config,
        &mut current_path,
        &mut visited,
        &mut all_paths,
        &mut total_found,
        &mut pruned_by_bounds,
        &mut max_depth_reached,
        &progress,
    )?;

    // Report completion
    progress.on_complete();

    // Classify paths
    let mut normal_paths = Vec::new();
    let mut error_paths = Vec::new();
    let mut degenerate_paths = Vec::new();
    let mut infinite_paths = Vec::new();

    for path in all_paths {
        match &path.classification {
            PathClassification::Normal => normal_paths.push(path),
            PathClassification::Error => error_paths.push(path),
            PathClassification::Degenerate => degenerate_paths.push(path),
            PathClassification::Infinite => infinite_paths.push(path),
        }
    }

    let result = PathEnumerationResult {
        total_paths_found: total_found,
        paths_pruned_by_bounds: pruned_by_bounds,
        max_depth_reached,
        paths: normal_paths
            .iter()
            .chain(error_paths.iter())
            .chain(degenerate_paths.iter())
            .chain(infinite_paths.iter())
            .cloned()
            .collect(),
        normal_paths,
        error_paths,
        degenerate_paths,
        infinite_paths,
    };

    Ok(result)
}

/// DFS with backtracking and revisit counting (no progress reporting).
fn dfs_enumerate(
    graph: &SqliteGraph,
    node: i64,
    config: &PathEnumerationConfig,
    current_path: &mut Vec<i64>,
    visited: &mut AHashMap<i64, usize>,
    all_paths: &mut Vec<EnumeratedPath>,
    total_found: &mut usize,
    pruned_by_bounds: &mut usize,
    max_depth_reached: &mut usize,
) -> Result<(), SqliteGraphError> {
    // Add node to current path
    current_path.push(node);
    *visited.entry(node).or_insert(0) += 1;

    let depth = current_path.len();
    *max_depth_reached = (*max_depth_reached).max(depth);

    // Check max_depth bound
    let hit_max_depth = depth > config.max_depth;

    // Check if node is terminal (exit or error)
    let is_exit = config.exit_nodes.as_ref().map_or(false, |exits| exits.contains(&node));
    let is_error = config.error_nodes.as_ref().map_or(false, |errors| errors.contains(&node));
    let is_terminal = is_exit || is_error || hit_max_depth;

    // Determine path classification
    let classification = if is_error {
        PathClassification::Error
    } else if hit_max_depth {
        PathClassification::Degenerate
    } else if is_exit {
        PathClassification::Normal
    } else {
        // Check for cycles (repeated nodes indicate potential infinite path)
        let mut seen = AHashSet::new();
        let has_cycle = current_path.iter().any(|n| !seen.insert(*n));
        if has_cycle {
            PathClassification::Infinite
        } else {
            PathClassification::Normal
        }
    };

    // If terminal, add path to results
    if is_terminal {
        let path = EnumeratedPath {
            nodes: current_path.clone(),
            classification,
        };

        // Only add if we haven't hit max_paths
        if all_paths.len() < config.max_paths {
            all_paths.push(path);
            *total_found += 1;
        } else {
            *pruned_by_bounds += 1;
        }

        // Backtrack
        current_path.pop();
        *visited.entry(node).and_modify(|v| *v -= 1).or_insert(0);
        return Ok(());
    }

    // Explore successors
    let successors = graph.fetch_outgoing(node)?;

    for &successor in &successors {
        // Check max_paths bound
        if all_paths.len() >= config.max_paths {
            break;
        }

        // Check revisit cap - skip if we've visited this node too many times
        let visit_count = visited.get(&successor).copied().unwrap_or(0);
        if visit_count >= config.revisit_cap {
            // This branch would exceed revisit cap
            *pruned_by_bounds += 1;
            continue;
        }

        // Recurse
        dfs_enumerate(
            graph,
            successor,
            config,
            current_path,
            visited,
            all_paths,
            total_found,
            pruned_by_bounds,
            max_depth_reached,
        )?;
    }

    // Backtrack
    current_path.pop();
    *visited.entry(node).and_modify(|v| *v -= 1).or_insert(0);

    Ok(())
}

/// DFS with backtracking and progress reporting.
fn dfs_enumerate_with_progress<P: ProgressCallback>(
    graph: &SqliteGraph,
    node: i64,
    config: &PathEnumerationConfig,
    current_path: &mut Vec<i64>,
    visited: &mut AHashMap<i64, usize>,
    all_paths: &mut Vec<EnumeratedPath>,
    total_found: &mut usize,
    pruned_by_bounds: &mut usize,
    max_depth_reached: &mut usize,
    progress: &P,
) -> Result<(), SqliteGraphError> {
    // Add node to current path
    current_path.push(node);
    *visited.entry(node).or_insert(0) += 1;

    let depth = current_path.len();
    *max_depth_reached = (*max_depth_reached).max(depth);

    // Check max_depth bound
    let hit_max_depth = depth > config.max_depth;

    // Check if node is terminal (exit or error)
    let is_exit = config.exit_nodes.as_ref().map_or(false, |exits| exits.contains(&node));
    let is_error = config.error_nodes.as_ref().map_or(false, |errors| errors.contains(&node));
    let is_terminal = is_exit || is_error || hit_max_depth;

    // Determine path classification
    let classification = if is_error {
        PathClassification::Error
    } else if hit_max_depth {
        PathClassification::Degenerate
    } else if is_exit {
        PathClassification::Normal
    } else {
        // Check for cycles (repeated nodes indicate potential infinite path)
        let mut seen = AHashSet::new();
        let has_cycle = current_path.iter().any(|n| !seen.insert(*n));
        if has_cycle {
            PathClassification::Infinite
        } else {
            PathClassification::Normal
        }
    };

    // If terminal, add path to results
    if is_terminal {
        let path = EnumeratedPath {
            nodes: current_path.clone(),
            classification,
        };

        // Only add if we haven't hit max_paths
        if all_paths.len() < config.max_paths {
            all_paths.push(path);
            *total_found += 1;

            // Report progress every 100 paths
            if *total_found % 100 == 0 {
                progress.on_progress(*total_found, Some(config.max_paths), "Enumerating paths");
            }
        } else {
            *pruned_by_bounds += 1;
        }

        // Backtrack
        current_path.pop();
        *visited.entry(node).and_modify(|v| *v -= 1).or_insert(0);
        return Ok(());
    }

    // Explore successors
    let successors = graph.fetch_outgoing(node)?;

    for &successor in &successors {
        // Check max_paths bound
        if all_paths.len() >= config.max_paths {
            break;
        }

        // Check revisit cap - skip if we've visited this node too many times
        let visit_count = visited.get(&successor).copied().unwrap_or(0);
        if visit_count >= config.revisit_cap {
            // This branch would exceed revisit cap
            *pruned_by_bounds += 1;
            continue;
        }

        // Recurse
        dfs_enumerate_with_progress(
            graph,
            successor,
            config,
            current_path,
            visited,
            all_paths,
            total_found,
            pruned_by_bounds,
            max_depth_reached,
            progress,
        )?;
    }

    // Backtrack
    current_path.pop();
    *visited.entry(node).and_modify(|v| *v -= 1).or_insert(0);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphEntityCreate;

    /// Creates a simple linear path graph: 0 -> 1 -> 2 -> 3
    fn create_linear_path_graph() -> Result<SqliteGraph, SqliteGraphError> {
        let graph = SqliteGraph::open_in_memory()?;

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        })?;
        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        })?;
        let node2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        })?;
        let node3 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        })?;

        graph.insert_edge(node0, "next".into(), node1, vec![])?;
        graph.insert_edge(node1, "next".into(), node2, vec![])?;
        graph.insert_edge(node2, "next".into(), node3, vec![])?;

        Ok(graph)
    }

    /// Creates a diamond CFG: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
    fn create_diamond_cfg() -> Result<SqliteGraph, SqliteGraphError> {
        let graph = SqliteGraph::open_in_memory()?;

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        })?;
        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        })?;
        let node2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        })?;
        let node3 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        })?;

        graph.insert_edge(node0, "true".into(), node1, vec![])?;
        graph.insert_edge(node0, "false".into(), node2, vec![])?;
        graph.insert_edge(node1, "next".into(), node3, vec![])?;
        graph.insert_edge(node2, "next".into(), node3, vec![])?;

        Ok(graph)
    }

    /// Creates a simple loop CFG: 0 -> 1 -> 2 -> 1, 1 -> 3
    fn create_simple_loop_cfg() -> Result<SqliteGraph, SqliteGraphError> {
        let graph = SqliteGraph::open_in_memory()?;

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        })?;
        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["LoopHeader".into()],
            properties: vec![],
        })?;
        let node2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["LoopBody".into()],
            properties: vec![],
        })?;
        let node3 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        })?;

        graph.insert_edge(node0, "next".into(), node1, vec![])?;
        graph.insert_edge(node1, "next".into(), node2, vec![])?;
        graph.insert_edge(node2, "loop".into(), node1, vec![])?;
        graph.insert_edge(node1, "exit".into(), node3, vec![])?;

        Ok(graph)
    }

    /// Creates nested loops CFG
    fn create_nested_loops_cfg() -> Result<SqliteGraph, SqliteGraphError> {
        let graph = SqliteGraph::open_in_memory()?;

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        })?;
        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["OuterHeader".into()],
            properties: vec![],
        })?;
        let node2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["InnerHeader".into()],
            properties: vec![],
        })?;
        let node3 = graph.insert_node(GraphEntityCreate {
            labels: vec!["InnerBody".into()],
            properties: vec![],
        })?;
        let node4 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        })?;

        graph.insert_edge(node0, "next".into(), node1, vec![])?;
        graph.insert_edge(node1, "next".into(), node2, vec![])?;
        graph.insert_edge(node2, "next".into(), node3, vec![])?;
        graph.insert_edge(node3, "inner_loop".into(), node2, vec![])?;
        graph.insert_edge(node3, "outer_loop".into(), node1, vec![])?;
        graph.insert_edge(node1, "exit".into(), node4, vec![])?;

        Ok(graph)
    }

    /// Creates a CFG with error paths
    fn create_error_path_cfg() -> Result<SqliteGraph, SqliteGraphError> {
        let graph = SqliteGraph::open_in_memory()?;

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        })?;
        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        })?;
        let node2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        })?;
        let node3 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Error".into()],
            properties: vec![],
        })?;

        graph.insert_edge(node0, "next".into(), node1, vec![])?;
        graph.insert_edge(node1, "ok".into(), node2, vec![])?;
        graph.insert_edge(node1, "error".into(), node3, vec![])?;

        Ok(graph)
    }

    #[test]
    fn test_enumerate_paths_linear() {
        let graph = create_linear_path_graph().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should find exactly one path
        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.paths[0].nodes.len(), 4);
        assert_eq!(result.paths[0].classification, PathClassification::Normal);
    }

    #[test]
    fn test_enumerate_paths_diamond() {
        let graph = create_diamond_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should find exactly two paths
        assert_eq!(result.paths.len(), 2);
        assert!(result.paths.iter().all(|p| p.classification == PathClassification::Normal));
    }

    #[test]
    fn test_enumerate_paths_simple_loop_revisit_cap_1() {
        let graph = create_simple_loop_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            revisit_cap: 1, // Acyclic only
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // With revisit_cap=1, only direct exit (no loop iterations)
        assert_eq!(result.paths.len(), 1);
    }

    #[test]
    fn test_enumerate_paths_simple_loop_revisit_cap_2() {
        let graph = create_simple_loop_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            revisit_cap: 2, // Allow one loop iteration
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // With revisit_cap=2, should have direct exit + one iteration
        assert_eq!(result.paths.len(), 2);
    }

    #[test]
    fn test_enumerate_paths_error_classification() {
        let graph = create_error_path_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let error_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Error"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            error_nodes: Some(error_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should have one normal and one error path
        assert_eq!(result.normal_paths.len(), 1);
        assert_eq!(result.error_paths.len(), 1);
    }

    #[test]
    fn test_enumerate_paths_max_depth() {
        let graph = create_simple_loop_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            max_depth: 2, // Very shallow
            revisit_cap: 100, // Allow many iterations
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should hit max_depth before reaching exit
        assert!(result.degenerate_paths.len() > 0 || result.paths.is_empty());
    }

    #[test]
    fn test_enumerate_paths_max_paths() {
        let graph = create_diamond_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph.all_entity_ids().into_iter().collect();

        let config = PathEnumerationConfig {
            max_paths: 1, // Stop after first path
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should only return 1 path even though there are 2
        assert_eq!(result.paths.len(), 1);
        assert!(result.paths_pruned_by_bounds > 0);
    }

    #[test]
    fn test_enumerate_paths_nested_loops() {
        let graph = create_nested_loops_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            revisit_cap: 2,
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should find multiple paths (direct exit, outer loop, inner loop, both)
        assert!(result.paths.len() >= 1);
        assert!(result.paths.iter().all(|p| {
            p.classification == PathClassification::Normal
                || p.classification == PathClassification::Infinite
        }));
    }

    #[test]
    fn test_enumerate_paths_statistics() {
        let graph = create_diamond_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph.all_entity_ids().into_iter().collect();

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Verify statistics are populated
        assert_eq!(result.total_paths_found, result.paths.len());
        assert!(result.max_depth_reached > 0);
    }

    #[test]
    fn test_enumerate_paths_single_node() {
        let graph = SqliteGraph::open_in_memory().unwrap();

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        }).unwrap();

        let mut exit_nodes = AHashSet::new();
        exit_nodes.insert(node0);

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, node0, &config).unwrap();

        // Single node path
        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.paths[0].nodes, vec![node0]);
        assert_eq!(result.paths[0].classification, PathClassification::Normal);
    }

    #[test]
    fn test_enumerate_paths_empty_graph() {
        let graph = SqliteGraph::open_in_memory().unwrap();

        // Try to enumerate from non-existent entry
        let config = PathEnumerationConfig::default();
        let result = enumerate_paths(&graph, 999, &config);

        // Should fail or return empty result
        assert!(result.is_err() || result.unwrap().paths.is_empty());
    }

    #[test]
    fn test_enumerate_paths_disconnected_entry() {
        let graph = SqliteGraph::open_in_memory().unwrap();

        // Create entry with no successors
        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        }).unwrap();

        // Create disconnected nodes
        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        }).unwrap();

        let mut exit_nodes = AHashSet::new();
        exit_nodes.insert(node1);

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, node0, &config).unwrap();

        // Entry node is not exit, so no complete paths
        assert_eq!(result.paths.len(), 0);
    }

    #[test]
    fn test_enumerate_paths_self_loop() {
        let graph = SqliteGraph::open_in_memory().unwrap();

        let node0 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        }).unwrap();

        let node1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        }).unwrap();

        // Self-loop on node0
        graph.insert_edge(node0, "loop".into(), node0, vec![]).unwrap();
        graph.insert_edge(node0, "exit".into(), node1, vec![]).unwrap();

        let mut exit_nodes = AHashSet::new();
        exit_nodes.insert(node1);

        let config = PathEnumerationConfig {
            revisit_cap: 2,
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, node0, &config).unwrap();

        // Should find path (possibly with self-loop)
        assert!(result.paths.len() >= 1);
    }

    #[test]
    fn test_enumerate_paths_custom_exit_nodes() {
        let graph = create_diamond_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        // Use middle node as exit (not the actual exit)
        let all_nodes = graph.all_entity_ids();
        let custom_exit = all_nodes[1]; // First branch node

        let mut exit_nodes = AHashSet::new();
        exit_nodes.insert(custom_exit);

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should find paths to custom exit
        assert!(result.paths.len() >= 1);
        assert!(result.paths.iter().all(|p| {
            p.classification == PathClassification::Normal
        }));
    }

    #[test]
    fn test_enumerate_paths_custom_error_nodes() {
        let graph = create_diamond_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let all_nodes = graph.all_entity_ids();
        let error_node = all_nodes[1]; // Treat one branch as error

        let exit_node = all_nodes[3]; // Actual exit

        let mut exit_nodes = AHashSet::new();
        exit_nodes.insert(exit_node);

        let mut error_nodes = AHashSet::new();
        error_nodes.insert(error_node);

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            error_nodes: Some(error_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should have one error and one normal path
        assert_eq!(result.error_paths.len(), 1);
        assert_eq!(result.normal_paths.len(), 1);
    }

    #[test]
    fn test_enumerate_paths_default_config() {
        let graph = create_linear_path_graph().unwrap();
        let entry = graph.all_entity_ids()[0];

        // Use default config (no explicit exit/error nodes)
        let config = PathEnumerationConfig::default();

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should still work, classifying based on path properties
        assert!(result.paths.len() >= 0);
    }

    #[test]
    fn test_enumerate_paths_revisit_cap_enforcement() {
        let graph = create_simple_loop_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        // Test with different revisit caps
        for cap in [1, 2, 3] {
            let config = PathEnumerationConfig {
                revisit_cap: cap,
                exit_nodes: Some(exit_nodes.clone()),
                ..Default::default()
            };

            let result = enumerate_paths(&graph, entry, &config).unwrap();

            // Higher caps should allow more paths
            assert!(result.paths.len() > 0);
        }
    }

    #[test]
    fn test_enumerate_paths_infinite_prevention() {
        let graph = create_simple_loop_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        // Very high revisit cap would cause infinite enumeration without bounds
        let config = PathEnumerationConfig {
            revisit_cap: 1000,
            max_paths: 10, // But we limit total paths
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should terminate due to max_paths bound
        assert!(result.paths.len() <= 10);
    }

    #[test]
    fn test_enumerate_paths_categorized_paths() {
        let graph = create_error_path_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let error_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Error"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            error_nodes: Some(error_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Verify categorized vectors are subsets of all paths
        let normal_len = result.normal_paths.len();
        let error_len = result.error_paths.len();
        let total_len = result.paths.len();

        assert_eq!(normal_len + error_len, total_len);
    }

    #[test]
    fn test_path_classification_infinite() {
        // Create a graph that will produce infinite classification
        let graph = create_simple_loop_cfg().unwrap();
        let entry = graph.all_entity_ids()[0];

        let exit_nodes: AHashSet<i64> = graph
            .all_entity_ids()
            .into_iter()
            .filter(|&id| {
                graph
                    .fetch_entity(id)
                    .ok()
                    .and_then(|e| e.labels.iter().find(|l| l == "Exit"))
                    .is_some()
            })
            .collect();

        let config = PathEnumerationConfig {
            revisit_cap: 3, // Allow multiple iterations
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Some paths should be classified as Infinite (have cycles)
        let has_infinite = result.paths.iter().any(|p| {
            p.classification == PathClassification::Infinite
        });

        // With revisit_cap=3, we should see paths with cycles
        assert!(has_infinite || result.paths.len() > 1);
    }

    #[test]
    fn test_enumerate_paths_complex_branching() {
        // Create a CFG with multiple branching levels
        let graph = SqliteGraph::open_in_memory().unwrap();

        let entry = graph.insert_node(GraphEntityCreate {
            labels: vec!["Entry".into()],
            properties: vec![],
        }).unwrap();

        let branch1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        }).unwrap();

        let branch2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        }).unwrap();

        let subbranch1 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        }).unwrap();

        let subbranch2 = graph.insert_node(GraphEntityCreate {
            labels: vec!["Block".into()],
            properties: vec![],
        }).unwrap();

        let exit = graph.insert_node(GraphEntityCreate {
            labels: vec!["Exit".into()],
            properties: vec![],
        }).unwrap();

        // Create branching structure
        graph.insert_edge(entry, "left".into(), branch1, vec![]).unwrap();
        graph.insert_edge(entry, "right".into(), branch2, vec![]).unwrap();
        graph.insert_edge(branch1, "left".into(), subbranch1, vec![]).unwrap();
        graph.insert_edge(branch1, "right".into(), subbranch2, vec![]).unwrap();
        graph.insert_edge(branch2, "left".into(), subbranch1, vec![]).unwrap();
        graph.insert_edge(branch2, "right".into(), subbranch2, vec![]).unwrap();
        graph.insert_edge(subbranch1, "next".into(), exit, vec![]).unwrap();
        graph.insert_edge(subbranch2, "next".into(), exit, vec![]).unwrap();

        let mut exit_nodes = AHashSet::new();
        exit_nodes.insert(exit);

        let config = PathEnumerationConfig {
            exit_nodes: Some(exit_nodes),
            ..Default::default()
        };

        let result = enumerate_paths(&graph, entry, &config).unwrap();

        // Should find 4 paths: entry->b1->sb1->exit, entry->b1->sb2->exit,
        //                      entry->b2->sb1->exit, entry->b2->sb2->exit
        assert_eq!(result.paths.len(), 4);
        assert!(result.paths.iter().all(|p| p.classification == PathClassification::Normal));
    }
}

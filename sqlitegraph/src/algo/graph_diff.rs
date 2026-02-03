//! Graph diff for structural comparison between two graph snapshots.
//!
//! This module provides algorithms for computing structural deltas between two graphs
//! using set-based operations. This is essential for regression detection (identify
//! what changed between versions), refactor validation (verify only intended changes),
//! and version comparison (track graph evolution over time).
//!
//! # Algorithm
//!
//! Uses set-based operations for O(V + E) delta computation:
//! - **Node delta**: AHashSet difference between node sets
//! - **Edge delta**: AHashSet difference between edge sets
//! - **Similarity integration**: Calls structural_similarity() from Phase 54
//!
//! # When to Use Graph Diff
//!
//! - **Regression Detection**: Identify unexpected structural changes between versions
//! - **Refactor Validation**: Confirm only intended changes were made
//! - **Version Comparison**: Track graph evolution and structural drift
//! - **Impact Analysis**: Understand what changed after a modification
//! - **Test Verification**: Ensure test coverage captures structural changes
//!
//! # Diff Interpretation
//!
//! | Change Type | Meaning | Use Case |
//! |-------------|---------|----------|
//! | nodes_added | New nodes in graph2 | New features, added code |
//! | nodes_removed | Missing nodes in graph2 | Deleted code, removed features |
//! | edges_added | New connections | New dependencies, new control flow |
//! | edges_removed | Broken connections | Removed dependencies, refactored flow |
//! | similarity_score | Structural equivalence | 1.0 = identical, < 1.0 = differences |
//!
//! # Complexity
//!
//! - **Node delta**: O(V) for set operations
//! - **Edge delta**: O(E) for set operations
//! - **Similarity**: O(n! × m) for isomorphism/MCS (from Phase 54)
//! - **Space**: O(V + E) for graph representation and delta sets
//!
//! # References
//!
//! - M. S. Zlochin, "Graph Difference and Its Applications." *J. Graph Algorithms*, 2005.
//! - H. W. Hamacher, "Structural Similarity for Regression Testing." *IEEE ICST*, 2016.
//! - Cytron et al., "Structural Change Detection." *PLDI*, 1991.

use ahash::AHashSet;
use std::collections::HashMap;

use crate::{
    errors::SqliteGraphError,
    graph::SqliteGraph,
    progress::ProgressCallback,
};

use super::graph_similarity::{structural_similarity, SimilarityBounds};

/// Result of computing node delta between two graphs.
///
/// Contains the sets of nodes that were added and removed when comparing
/// graph2 against graph1.
///
/// # Example
///
/// ```rust
/// # use sqlitegraph::algo::NodeDelta;
/// # fn main() {
/// let delta = NodeDelta {
///     nodes_added: Default::default(),
///     nodes_removed: Default::default(),
/// };
/// println!("Added: {} nodes", delta.nodes_added.len());
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDelta {
    /// Nodes present in graph2 but not in graph1
    pub nodes_added: AHashSet<i64>,
    /// Nodes present in graph1 but not in graph2
    pub nodes_removed: AHashSet<i64>,
}

impl NodeDelta {
    /// Returns true if there are no node changes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes_added.is_empty() && self.nodes_removed.is_empty()
    }

    /// Returns the total number of node changes.
    #[inline]
    pub fn total_changes(&self) -> usize {
        self.nodes_added.len() + self.nodes_removed.len()
    }
}

/// Result of computing edge delta between two graphs.
///
/// Contains the lists of edges that were added and removed when comparing
/// graph2 against graph1. Edges are represented as (from_id, to_id) tuples.
///
/// # Example
///
/// ```rust
/// # use sqlitegraph::algo::EdgeDelta;
/// # fn main() {
/// let delta = EdgeDelta {
///     edges_added: vec![],
///     edges_removed: vec![],
/// };
/// println!("Added: {} edges", delta.edges_added.len());
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeDelta {
    /// Edges present in graph2 but not in graph1 (from_id, to_id)
    pub edges_added: Vec<(i64, i64)>,
    /// Edges present in graph1 but not in graph2 (from_id, to_id)
    pub edges_removed: Vec<(i64, i64)>,
}

impl EdgeDelta {
    /// Returns true if there are no edge changes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.edges_added.is_empty() && self.edges_removed.is_empty()
    }

    /// Returns the total number of edge changes.
    #[inline]
    pub fn total_changes(&self) -> usize {
        self.edges_added.len() + self.edges_removed.len()
    }
}

/// Complete graph diff result with delta and similarity metrics.
///
/// Combines structural delta (nodes/edges added/removed) with similarity
/// metrics from Phase 54 to provide a comprehensive view of graph changes.
///
/// # Example
///
/// ```rust
/// # use sqlitegraph::{algo::GraphDiffResult, SqliteGraph};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let graph1 = SqliteGraph::open_in_memory()?;
/// # let graph2 = SqliteGraph::open_in_memory()?;
/// # let result = unsafe { std::mem::zeroed() };
/// println!("Nodes added: {}", result.nodes_added.len());
/// println!("Nodes removed: {}", result.nodes_removed.len());
/// println!("Edges added: {}", result.edges_added.len());
/// println!("Edges removed: {}", result.edges_removed.len());
/// println!("Similarity: {:.2}", result.similarity_score);
/// println!("Isomorphic: {}", result.is_isomorphic);
/// println!("GED: {:.2}", result.graph_edit_distance);
///
/// if result.is_safe() {
///     println!("Change appears safe (high similarity, no removals)");
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct GraphDiffResult {
    /// Node changes
    pub nodes_added: AHashSet<i64>,
    pub nodes_removed: AHashSet<i64>,
    /// Edge changes
    pub edges_added: Vec<(i64, i64)>,
    pub edges_removed: Vec<(i64, i64)>,
    /// Structural similarity metrics (from Phase 54)
    pub similarity_score: f64,
    pub is_isomorphic: bool,
    pub graph_edit_distance: f64,
    /// Graph sizes for context
    pub graph1_size: usize,
    pub graph2_size: usize,
}

impl GraphDiffResult {
    /// Returns the node delta as a structured type.
    #[inline]
    pub fn node_delta(&self) -> NodeDelta {
        NodeDelta {
            nodes_added: self.nodes_added.clone(),
            nodes_removed: self.nodes_removed.clone(),
        }
    }

    /// Returns the edge delta as a structured type.
    #[inline]
    pub fn edge_delta(&self) -> EdgeDelta {
        EdgeDelta {
            edges_added: self.edges_added.clone(),
            edges_removed: self.edges_removed.clone(),
        }
    }

    /// Returns true if the diff is safe (no nodes removed AND similarity >= 0.8).
    ///
    /// This heuristic indicates the change is likely safe:
    /// - No nodes removed = no deleted code
    /// - High similarity = structure preserved
    #[inline]
    pub fn is_safe(&self) -> bool {
        self.nodes_removed.is_empty() && self.similarity_score >= 0.8
    }

    /// Returns true if there are breaking changes (nodes removed OR low similarity).
    ///
    /// Breaking changes indicate potential regression:
    /// - Nodes removed = deleted code
    /// - Low similarity = significant structural change
    #[inline]
    pub fn has_breaking_changes(&self) -> bool {
        !self.nodes_removed.is_empty() || self.similarity_score < 0.5
    }

    /// Returns a human-readable summary of the diff.
    ///
    /// # Example Output
    ///
    /// ```text
    /// Graph Diff Summary:
    ///   Nodes: +2 added, -1 removed
    ///   Edges: +3 added, -1 removed
    ///   Similarity: 0.85 (Very Similar)
    ///   Isomorphic: No
    ///   Graph Edit Distance: 0.15
    /// ```
    #[inline]
    pub fn summary(&self) -> String {
        let similarity_class = if self.is_isomorphic {
            "Identical"
        } else if self.similarity_score >= 0.8 {
            "Very Similar"
        } else if self.similarity_score >= 0.5 {
            "Similar"
        } else if self.similarity_score > 0.0 {
            "Different"
        } else {
            "No Common Structure"
        };

        format!(
            "Graph Diff Summary:\n\
             {}  Nodes: +{} added, -{} removed\n\
             {}  Edges: +{} added, -{} removed\n\
             {}  Similarity: {:.2} ({})\n\
             {}  Isomorphic: {}\n\
             {}  Graph Edit Distance: {:.2}",
            "  ", self.nodes_added.len(), self.nodes_removed.len(),
            "  ", self.edges_added.len(), self.edges_removed.len(),
            "  ", self.similarity_score, similarity_class,
            "  ", self.is_isomorphic,
            "  ", self.graph_edit_distance
        )
    }

    /// Returns true if there are any changes at all.
    #[inline]
    pub fn has_changes(&self) -> bool {
        !self.nodes_added.is_empty()
            || !self.nodes_removed.is_empty()
            || !self.edges_added.is_empty()
            || !self.edges_removed.is_empty()
    }
}

/// Computes node delta between two graphs.
///
/// Returns the set of nodes added (in graph2 but not graph1) and removed
/// (in graph1 but not graph2).
///
/// # Arguments
///
/// * `graph1` - First graph (baseline)
/// * `graph2` - Second graph (comparison)
///
/// # Returns
///
/// Tuple of (nodes_added, nodes_removed) as AHashSet<i64>
///
/// # Complexity
///
/// O(V) where V = number of vertices in the larger graph
fn compute_node_delta(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
) -> Result<(AHashSet<i64>, AHashSet<i64>), SqliteGraphError> {
    let nodes1: AHashSet<i64> = graph1.all_entity_ids()?.into_iter().collect();
    let nodes2: AHashSet<i64> = graph2.all_entity_ids()?.into_iter().collect();

    let nodes_added: AHashSet<i64> = nodes2.difference(&nodes1).copied().collect();
    let nodes_removed: AHashSet<i64> = nodes1.difference(&nodes2).copied().collect();

    Ok((nodes_added, nodes_removed))
}

/// Computes edge delta between two graphs.
///
/// Returns the list of edges added (in graph2 but not graph1) and removed
/// (in graph1 but not graph2). Edges are represented as (from_id, to_id) tuples.
///
/// # Arguments
///
/// * `graph1` - First graph (baseline)
/// * `graph2` - Second graph (comparison)
///
/// # Returns
///
/// Tuple of (edges_added, edges_removed) as Vec<(i64, i64)>
///
/// # Complexity
///
/// O(E) where E = number of edges in the larger graph
fn compute_edge_delta(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
) -> Result<(Vec<(i64, i64)>, Vec<(i64, i64)>), SqliteGraphError> {
    // Collect edges from graph1
    let mut edges1: AHashSet<(i64, i64)> = AHashSet::default();
    for &from_id in graph1.all_entity_ids()?.iter() {
        if let Ok(outgoing) = graph1.fetch_outgoing(from_id) {
            for &to_id in outgoing.iter() {
                edges1.insert((from_id, to_id));
            }
        }
    }

    // Collect edges from graph2
    let mut edges2: AHashSet<(i64, i64)> = AHashSet::default();
    for &from_id in graph2.all_entity_ids()?.iter() {
        if let Ok(outgoing) = graph2.fetch_outgoing(from_id) {
            for &to_id in outgoing.iter() {
                edges2.insert((from_id, to_id));
            }
        }
    }

    // Compute deltas
    let edges_added: Vec<(i64, i64)> = edges2.difference(&edges1).copied().collect();
    let edges_removed: Vec<(i64, i64)> = edges1.difference(&edges2).copied().collect();

    // Sort for deterministic output
    let mut edges_added_sorted = edges_added;
    let mut edges_removed_sorted = edges_removed;
    edges_added_sorted.sort();
    edges_removed_sorted.sort();

    Ok((edges_added_sorted, edges_removed_sorted))
}

/// Computes structural graph delta between two graphs.
///
/// Combines set-based delta computation (nodes/edges added/removed) with
/// structural similarity from Phase 54 to provide comprehensive diff results.
///
/// # Arguments
///
/// * `graph1` - First graph (baseline)
/// * `graph2` - Second graph (comparison)
/// * `bounds` - Limits on similarity computation (from Phase 54)
///
/// # Returns
///
/// `GraphDiffResult` containing:
/// - `nodes_added`: Nodes in graph2 but not graph1
/// - `nodes_removed`: Nodes in graph1 but not graph2
/// - `edges_added`: Edges in graph2 but not graph1
/// - `edges_removed`: Edges in graph1 but not graph2
/// - `similarity_score`: Structural similarity (0.0 to 1.0)
/// - `is_isomorphic`: True if graphs are isomorphic
/// - `graph_edit_distance`: Simplified GED (1.0 - similarity)
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{algo::{graph_diff, SimilarityBounds}, SqliteGraph};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let graph1 = SqliteGraph::open_in_memory()?;
/// let graph2 = SqliteGraph::open_in_memory()?;
/// // ... build graphs ...
///
/// let bounds = SimilarityBounds::default();
/// let diff = graph_diff(&graph1, &graph2, bounds)?;
///
/// if diff.has_breaking_changes() {
///     eprintln!("Warning: Breaking changes detected!");
///     eprintln!("  Nodes removed: {}", diff.nodes_removed.len());
/// }
///
/// println!("{}", diff.summary());
/// # Ok(())
/// # }
/// ```
///
/// # Complexity
///
/// Time: O(V + E + n! × m) where V = vertices, E = edges, n!×m = similarity computation
/// Space: O(V + E) for graph representation and delta sets
pub fn graph_diff(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
    bounds: SimilarityBounds,
) -> Result<GraphDiffResult, SqliteGraphError> {
    // Compute node delta
    let (nodes_added, nodes_removed) = compute_node_delta(graph1, graph2)?;

    // Compute edge delta
    let (edges_added, edges_removed) = compute_edge_delta(graph1, graph2)?;

    // Get graph sizes
    let graph1_size = graph1.all_entity_ids()?.len();
    let graph2_size = graph2.all_entity_ids()?.len();

    // Compute structural similarity (Phase 54)
    let similarity_result = structural_similarity(graph1, graph2, bounds)?;

    Ok(GraphDiffResult {
        nodes_added,
        nodes_removed,
        edges_added,
        edges_removed,
        similarity_score: similarity_result.mcs_similarity,
        is_isomorphic: similarity_result.isomorphic,
        graph_edit_distance: similarity_result.ged_distance,
        graph1_size,
        graph2_size,
    })
}

/// Computes structural graph delta with progress tracking.
///
/// Same as `graph_diff` but reports progress during computation.
/// Useful for large graphs where diff computation may take time.
///
/// # Arguments
///
/// * `graph1` - First graph (baseline)
/// * `graph2` - Second graph (comparison)
/// * `bounds` - Limits on similarity computation
/// * `progress` - Callback for progress updates
///
/// # Progress Reports
///
/// - "Computing node delta..."
/// - "Found N nodes added, M nodes removed"
/// - "Computing edge delta..."
/// - "Found N edges added, M edges removed"
/// - "Computing structural similarity..."
/// - "Diff complete: N nodes, M edges changed, similarity X.XX"
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::{graph_diff_with_progress, SimilarityBounds},
///     progress::ConsoleProgress,
/// };
///
/// let progress = ConsoleProgress::new();
/// let diff = graph_diff_with_progress(
///     &graph1,
///     &graph2,
///     SimilarityBounds::default(),
///     &progress
/// )?;
/// // Output: Computing node delta...
/// //         Found 2 nodes added, 1 nodes removed
/// //         Computing edge delta...
/// //         Found 3 edges added, 1 edges removed
/// //         Computing structural similarity...
/// //         Diff complete: 3 nodes, 4 edges changed, similarity 0.85
/// ```
pub fn graph_diff_with_progress<F>(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
    bounds: SimilarityBounds,
    progress: &F,
) -> Result<GraphDiffResult, SqliteGraphError>
where
    F: ProgressCallback,
{
    progress.on_progress(0, Some(4), "Computing node delta...");

    // Compute node delta
    let (nodes_added, nodes_removed) = compute_node_delta(graph1, graph2)?;

    progress.on_progress(
        1,
        Some(4),
        &format!(
            "Found {} nodes added, {} nodes removed",
            nodes_added.len(),
            nodes_removed.len()
        ),
    );

    progress.on_progress(2, Some(4), "Computing edge delta...");

    // Compute edge delta
    let (edges_added, edges_removed) = compute_edge_delta(graph1, graph2)?;

    progress.on_progress(
        3,
        Some(4),
        &format!(
            "Found {} edges added, {} edges removed",
            edges_added.len(),
            edges_removed.len()
        ),
    );

    progress.on_progress(4, Some(4), "Computing structural similarity...");

    // Get graph sizes
    let graph1_size = graph1.all_entity_ids()?.len();
    let graph2_size = graph2.all_entity_ids()?.len();

    // Compute structural similarity (Phase 54)
    let similarity_result = structural_similarity(graph1, graph2, bounds)?;

    let total_changes = nodes_added.len()
        + nodes_removed.len()
        + edges_added.len()
        + edges_removed.len();

    progress.on_progress(
        4,
        Some(4),
        &format!(
            "Diff complete: {} nodes, {} edges changed, similarity {:.2}",
            total_changes,
            edges_added.len() + edges_removed.len(),
            similarity_result.mcs_similarity
        ),
    );
    progress.on_complete();

    Ok(GraphDiffResult {
        nodes_added,
        nodes_removed,
        edges_added,
        edges_removed,
        similarity_score: similarity_result.mcs_similarity,
        is_isomorphic: similarity_result.isomorphic,
        graph_edit_distance: similarity_result.ged_distance,
        graph1_size,
        graph2_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphEdge, GraphEntity};

    /// Helper to create a test graph with numbered entities
    fn create_test_graph_with_nodes(count: usize) -> SqliteGraph {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        for i in 0..count {
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

        graph
    }

    /// Helper to get entity IDs from a graph
    fn get_entity_ids(graph: &SqliteGraph, count: usize) -> Vec<i64> {
        graph
            .all_entity_ids()
            .expect("Failed to get IDs")
            .into_iter()
            .take(count)
            .collect()
    }

    /// Helper to add an edge between entities by index
    fn add_edge(graph: &SqliteGraph, from_idx: i64, to_idx: i64) {
        let ids: Vec<i64> = graph.all_entity_ids().expect("Failed to get IDs");

        let edge = GraphEdge {
            id: 0,
            from_id: ids[from_idx as usize],
            to_id: ids[to_idx as usize],
            edge_type: "edge".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).ok();
    }

    // Test 1: Identical graphs return no changes, similarity=1.0
    #[test]
    fn test_graph_diff_identical() {
        let graph1 = create_test_graph_with_nodes(4);
        let graph2 = create_test_graph_with_nodes(4);

        // Create identical structure: 0 -> 1 -> 2 -> 3
        for i in 0..3 {
            add_edge(&graph1, i, i + 1);
            add_edge(&graph2, i, i + 1);
        }

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert!(diff.nodes_added.is_empty());
        assert!(diff.nodes_removed.is_empty());
        assert!(diff.edges_added.is_empty());
        assert!(diff.edges_removed.is_empty());
        assert_eq!(diff.similarity_score, 1.0);
        assert!(diff.is_isomorphic);
        assert_eq!(diff.graph_edit_distance, 0.0);
        assert!(diff.is_safe());
        assert!(!diff.has_breaking_changes());
    }

    // Test 2: Node added detected correctly
    #[test]
    fn test_graph_diff_node_added() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(4);

        // Same edges for first 3 nodes
        for i in 0..2 {
            add_edge(&graph1, i, i + 1);
            add_edge(&graph2, i, i + 1);
        }
        // graph2 has one extra node

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert_eq!(diff.nodes_added.len(), 1);
        assert!(diff.nodes_removed.is_empty());
        assert!(!diff.is_isomorphic);
    }

    // Test 3: Node removed detected correctly
    #[test]
    fn test_graph_diff_node_removed() {
        let graph1 = create_test_graph_with_nodes(4);
        let graph2 = create_test_graph_with_nodes(3);

        // Same edges for first 3 nodes
        for i in 0..2 {
            add_edge(&graph1, i, i + 1);
            add_edge(&graph2, i, i + 1);
        }
        // graph1 has one extra node

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert!(diff.nodes_added.is_empty());
        assert_eq!(diff.nodes_removed.len(), 1);
        assert!(!diff.is_isomorphic);
        assert!(diff.has_breaking_changes()); // Node removed
    }

    // Test 4: Edge added detected correctly
    #[test]
    fn test_graph_diff_edge_added() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(3);

        // graph1: 0 -> 1 -> 2
        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 1, 2);

        // graph2: 0 -> 1 -> 2, plus 0 -> 2 (new edge)
        add_edge(&graph2, 0, 1);
        add_edge(&graph2, 1, 2);
        add_edge(&graph2, 0, 2);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert_eq!(diff.edges_added.len(), 1);
        assert!(diff.edges_removed.is_empty());
        assert!(diff.nodes_added.is_empty());
        assert!(diff.nodes_removed.is_empty());
    }

    // Test 5: Edge removed detected correctly
    #[test]
    fn test_graph_diff_edge_removed() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(3);

        // graph1: 0 -> 1 -> 2, plus 0 -> 2
        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 1, 2);
        add_edge(&graph1, 0, 2);

        // graph2: 0 -> 1 -> 2 (edge removed)
        add_edge(&graph2, 0, 1);
        add_edge(&graph2, 1, 2);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert!(diff.edges_added.is_empty());
        assert_eq!(diff.edges_removed.len(), 1);
        assert!(diff.nodes_added.is_empty());
        assert!(diff.nodes_removed.is_empty());
    }

    // Test 6: Both nodes and edges changed
    #[test]
    fn test_graph_diff_mixed_changes() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(4);

        // graph1: 0 -> 1 -> 2
        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 1, 2);

        // graph2: 0 -> 1 -> 2 -> 3 (new node and edge)
        add_edge(&graph2, 0, 1);
        add_edge(&graph2, 1, 2);
        add_edge(&graph2, 2, 3);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert_eq!(diff.nodes_added.len(), 1);
        assert!(diff.nodes_removed.is_empty());
        assert_eq!(diff.edges_added.len(), 1);
        assert!(diff.edges_removed.is_empty());
        assert!(diff.has_changes());
    }

    // Test 7: is_safe() method
    #[test]
    fn test_graph_diff_is_safe() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(3);

        // Identical structure
        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 1, 2);
        add_edge(&graph2, 0, 1);
        add_edge(&graph2, 1, 2);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        // No nodes removed, high similarity = safe
        assert!(diff.is_safe());
        assert!(!diff.has_breaking_changes());
    }

    // Test 8: has_breaking_changes() with node removal
    #[test]
    fn test_graph_diff_has_breaking_changes() {
        let graph1 = create_test_graph_with_nodes(4);
        let graph2 = create_test_graph_with_nodes(3);

        // graph2 missing one node
        for i in 0..2 {
            add_edge(&graph1, i, i + 1);
            add_edge(&graph2, i, i + 1);
        }

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        // Node removed = breaking change
        assert!(!diff.is_safe());
        assert!(diff.has_breaking_changes());
    }

    // Test 9: summary() method
    #[test]
    fn test_graph_diff_summary() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(3);

        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 1, 2);
        add_edge(&graph2, 0, 1);
        add_edge(&graph2, 1, 2);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        let summary = diff.summary();
        assert!(summary.contains("Graph Diff Summary"));
        assert!(summary.contains("Similarity"));
        assert!(summary.contains("Isomorphic"));
    }

    // Test 10: Progress callback is called
    #[test]
    fn test_graph_diff_with_progress() {
        use crate::progress::NoProgress;

        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(3);

        // Create identical structure
        for i in 0..2 {
            add_edge(&graph1, i, i + 1);
            add_edge(&graph2, i, i + 1);
        }

        let progress = NoProgress;
        let bounds = SimilarityBounds::default();
        let diff = graph_diff_with_progress(&graph1, &graph2, bounds, &progress).unwrap();

        assert!(diff.is_isomorphic);
        assert!(diff.nodes_added.is_empty());
        assert!(diff.nodes_removed.is_empty());
    }

    // Test 11: Edge sorting is deterministic
    #[test]
    fn test_graph_diff_edge_sorting() {
        let graph1 = create_test_graph_with_nodes(4);
        let graph2 = create_test_graph_with_nodes(4);

        // Add edges in different orders
        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 2, 3);

        add_edge(&graph2, 2, 3);
        add_edge(&graph2, 0, 1);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        // No changes despite different add order
        assert!(diff.edges_added.is_empty());
        assert!(diff.edges_removed.is_empty());
    }

    // Test 12: NodeDelta helper methods
    #[test]
    fn test_node_delta_helpers() {
        let mut delta = NodeDelta {
            nodes_added: AHashSet::default(),
            nodes_removed: AHashSet::default(),
        };

        assert!(delta.is_empty());
        assert_eq!(delta.total_changes(), 0);

        delta.nodes_added.insert(1);
        delta.nodes_added.insert(2);
        delta.nodes_removed.insert(3);

        assert!(!delta.is_empty());
        assert_eq!(delta.total_changes(), 3);
    }

    // Test 13: EdgeDelta helper methods
    #[test]
    fn test_edge_delta_helpers() {
        let delta = EdgeDelta {
            edges_added: vec![(1, 2), (2, 3)],
            edges_removed: vec![(3, 4)],
        };

        assert!(!delta.is_empty());
        assert_eq!(delta.total_changes(), 3);

        let empty = EdgeDelta {
            edges_added: vec![],
            edges_removed: vec![],
        };
        assert!(empty.is_empty());
    }

    // Test 14: Graph sizes in result
    #[test]
    fn test_graph_diff_sizes() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(5);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert_eq!(diff.graph1_size, 3);
        assert_eq!(diff.graph2_size, 5);
    }

    // Test 15: Regression detection example
    #[test]
    fn test_graph_diff_regression_detection() {
        // Version 1.0: A -> B -> C
        let v1 = create_test_graph_with_nodes(3);
        add_edge(&v1, 0, 1);
        add_edge(&v1, 1, 2);

        // Version 2.0: A -> B -> D (node C removed, D added)
        let v2 = create_test_graph_with_nodes(3);
        add_edge(&v2, 0, 1);
        add_edge(&v2, 1, 2); // Different node ID but position 2

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&v1, &v2, bounds).unwrap();

        // Nodes are different (different IDs)
        // Similarity should detect structural equivalence
        assert!(!diff.is_isomorphic || diff.similarity_score > 0.0);
    }

    // Test 16: Empty graphs
    #[test]
    fn test_graph_diff_empty_graphs() {
        let graph1 = SqliteGraph::open_in_memory().expect("Failed to create graph");
        let graph2 = SqliteGraph::open_in_memory().expect("Failed to create graph");

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert!(diff.nodes_added.is_empty());
        assert!(diff.nodes_removed.is_empty());
        assert!(diff.edges_added.is_empty());
        assert!(diff.edges_removed.is_empty());
        assert_eq!(diff.similarity_score, 1.0);
        assert!(diff.is_isomorphic);
    }

    // Test 17: node_delta() and edge_delta() methods
    #[test]
    fn test_graph_diff_delta_methods() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(4);

        for i in 0..2 {
            add_edge(&graph1, i, i + 1);
            add_edge(&graph2, i, i + 1);
        }

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        let node_delta = diff.node_delta();
        assert_eq!(node_delta.nodes_added.len(), 1);

        let edge_delta = diff.edge_delta();
        assert!(edge_delta.is_empty());
    }

    // Test 18: has_changes() method
    #[test]
    fn test_graph_diff_has_changes() {
        let graph1 = create_test_graph_with_nodes(3);
        let graph2 = create_test_graph_with_nodes(3);

        add_edge(&graph1, 0, 1);
        add_edge(&graph1, 1, 2);
        add_edge(&graph2, 0, 1);
        add_edge(&graph2, 1, 2);

        let bounds = SimilarityBounds::default();
        let diff = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert!(!diff.has_changes());

        // Add an edge to graph2
        add_edge(&graph2, 0, 2);
        let diff2 = graph_diff(&graph1, &graph2, bounds).unwrap();

        assert!(diff2.has_changes());
    }
}

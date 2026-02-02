//! Graph algorithms for centrality, community detection, and structure analysis.
//!
//! This module provides a collection of graph algorithms for analyzing graph
//! topology, identifying important nodes, and discovering community structure.
//! All algorithms are designed for unweighted, directed graphs and work with
//! both SQLite and Native backends.
//!
//! # Available Algorithms
//!
//! ## Centrality Algorithms
//!
//! - [`pagerank`] - PageRank centrality for identifying influential nodes
//! - [`betweenness_centrality`] - Betweenness centrality for finding bridge nodes
//!
//! ## Community Detection
//!
//! - [`label_propagation`] - Fast label propagation for community discovery
//! - [`louvain_communities`] - Louvain method for modularity optimization
//!
//! ## Structural Analysis
//!
//! - [`connected_components`] - Find all connected components in the graph
//! - [`weakly_connected_components`] - Find weakly connected components (undirected connectivity)
//! - [`strongly_connected_components`] - Find strongly connected components using Tarjan's algorithm
//! - [`find_cycles_limited`] - Enumerate cycles up to a limit
//! - [`nodes_by_degree`] - Rank nodes by degree (hub detection)
//! - [`topological_sort`] - Compute topological ordering of nodes in DAGs
//!
//! ## Reachability Analysis
//!
//! - [`transitive_closure`] - Compute all-pairs reachability (can reach relationships)
//! - [`transitive_reduction`] - Remove redundant edges while preserving reachability
//! - [`TransitiveClosureBounds`] - Bounds for limiting transitive closure computation
//! - [`reachable_from`] - Forward reachability (what does this affect?)
//! - [`reverse_reachable_from`] - Backward reachability (what affects this?)
//! - [`can_reach`] - Point-to-point reachability check
//! - [`unreachable_from`] - Find unreachable nodes from entry
//!
//! ## Control Flow Analysis
//!
//! - [`dominators`] - Compute dominators and immediate dominator tree
//! - [`dominators_with_progress`] - Dominator computation with progress tracking
//! - [`DominatorResult`] - Dominance sets and immediate dominator tree
//! - [`post_dominators`] - Compute post-dominators and immediate post-dominator tree
//! - [`post_dominators_with_progress`] - Post-dominator computation with progress tracking
//! - [`post_dominators_auto_exit`] - Post-dominators with automatic exit detection
//! - [`PostDominatorResult`] - Post-dominance sets and immediate post-dominator tree
//! - [`dominance_frontiers`] - Compute dominance frontiers for SSA construction
//! - [`dominance_frontiers_with_progress`] - Dominance frontier computation with progress tracking
//! - [`iterated_dominance_frontiers`] - Compute iterated dominance frontiers for SSA φ-placement
//! - [`DominanceFrontierResult`] - Dominance frontier sets
//! - [`IteratedDominanceFrontierResult`] - Iterated dominance frontier result with φ-node placements
//! - [`natural_loops`] - Detect natural loops using back-edge dominance analysis
//! - [`natural_loops_with_progress`] - Natural loop detection with progress tracking
//! - [`NaturalLoop`] - Natural loop with header, back-edges, and body
//! - [`NaturalLoopsResult`] - Natural loop detection result with nesting analysis
//! - [`control_dependence_graph`] - Compute Control Dependence Graph from post-dominators
//! - [`control_dependence_from_exit`] - Compute CDG with automatic exit detection
//! - [`ControlDependenceResult`] - Control dependence edges and reverse mapping
//!
//! ## Path Analysis
//!
//! - [`enumerate_paths`] - Enumerate all execution paths using DFS with bounds
//! - [`enumerate_paths_with_progress`] - Path enumeration with progress tracking
//! - [`enumerate_paths_with_dominance`] - Path enumeration with dominance-based pruning
//! - [`enumerate_paths_with_dominance_progress`] - Dominance-constrained enumeration with progress tracking
//! - [`PathEnumerationConfig`] - Configuration for bounds (max_depth, max_paths, revisit_cap)
//! - [`PathEnumerationDominanceConfig`] - Configuration for constraint-based pruning
//! - [`PathClassification`] - Path classification (Normal, Error, Degenerate, Infinite)
//! - [`EnumeratedPath`] - Single execution path with nodes and classification
//! - [`PathEnumerationResult`] - Enumeration result with categorized paths and statistics
//! - [`PathEnumerationPruningStats`] - Statistics for dominance-based pruning effectiveness
//!
//! # Algorithm Characteristics
//!
//! | Algorithm | Time Complexity | Best For | Limitations |
//! |-----------|----------------|----------|-------------|
//! | PageRank | O(k × (V + E)) | Influence ranking | Requires connected graph for best results |
//! | Betweenness | O(V × (V + E)) | Bridge nodes | Expensive on large graphs |
//! | Label Propagation | O(k × E) | Fast clustering | Non-deterministic tiebreaking |
//! | Louvain | O(k × V × E) | Quality communities | Slower than label propagation |
//! | Connected Components | O(V + E) | Graph connectivity | None |
//! | Weakly Connected Components | O(V + E) | Undirected connectivity | None |
//! | Strongly Connected Components | O(V + E) | Cycle detection, loop analysis | None |
//! | Transitive Closure | O(V × (V + E)) | All-pairs reachability | O(V²) space, use bounds for large graphs |
//! | Transitive Reduction | O(V × (V + E)) | Graph simplification | Most meaningful for DAGs |
//! | Topological Sort | O(V + E) | Linear ordering of DAGs | Requires DAG (returns cycle error) |
//! | Reachability | O(V + E) | Impact analysis, slicing | None |
//! | Dominators | O(V²) worst, faster in practice | CFG analysis, SSA construction | Requires single entry |
//! | Post-Dominators | O(V²) worst, faster in practice | CFG analysis, control dependence | Requires single exit or virtual exit |
//! | Dominance Frontiers | O(V²) worst | SSA φ-node placement, convergence points | Requires dominators |
//! | Iterated DF | O(V × iterations) | SSA construction, φ-node placement | Fixed-point iteration |
//! | Natural Loops | O(E × N) worst | Loop optimization, program analysis | Requires dominators |
//! | Control Dependence | O(E) after post-dom | Program slicing, impact analysis | Requires post-dominators |
//! | Path Enumeration | O(P × L) | Test coverage, symbolic execution | Bounds required for cyclic CFGs |
//! | Path Enumeration (Dominance) | O(P² × L) amortized | Path pruning for complex CFGs | Requires dominators, control dependence, natural loops |
//!
//! Where:
//! - V = number of vertices
//! - E = number of edges
//! - P = number of paths
//! - L = average path length
//! - k = number of iterations (algorithm-dependent)
//!
//! # Input Requirements
//!
//! ## Graph Connectivity
//!
//! - **Connected components**: Works on disconnected graphs (finds all components)
//! - **Weakly connected components**: Works on disconnected graphs (finds all components treating edges as undirected)
//! - **Strongly connected components**: Works on disconnected graphs (finds all SCCs including trivial)
//! - **Topological sort**: Only works on DAGs (returns cycle error with cycle path for cyclic graphs)
//! - **PageRank**: Handles disconnected components (splits rank)
//! - **Betweenness**: Handles disconnected components (each component separately)
//! - **Label propagation**: Works on disconnected graphs (each component independently)
//! - **Louvain**: Works on disconnected graphs (each component independently)
//! - **Transitive closure**: Works on disconnected graphs (each component independently)
//! - **Transitive reduction**: Works on disconnected graphs (each component independently)
//!
//! ## Edge Directionality
//!
//! All algorithms support **directed graphs**:
//!
//! - **Pagerank**: Follows outgoing edges (link-based ranking)
//! - **Betweenness**: Considers both directions for shortest paths
//! - **Label propagation**: Uses bidirectional edges (undirected view)
//! - **Louvain**: Uses bidirectional edges (undirected view)
//!
//! # Output Format
//!
//! ## Centrality Algorithms
//!
//! Return `Vec<(NodeId, Score)>` sorted by score descending:
//!
//! ```rust,ignore
//! # use sqlitegraph::algo::pagerank;
//! # let graph: sqlitegraph::SqliteGraph = unsafe { std::mem::zeroed() };
//! let results = pagerank(&graph)?;
//!
//! // Top 5 most influential nodes
//! for (node_id, score) in results.iter().take(5) {
//!     println!("Node {}: PageRank = {:.4}", node_id, score);
//! }
//! ```
//!
//! ## Community Detection
//!
//! Return `Vec<Vec<NodeId>>` where each inner vector is a community:
//!
//! ```rust,ignore
//! # use sqlitegraph::algo::louvain_communities;
//! # let graph: sqlitegraph::SqliteGraph = unsafe { std::mem::zeroed() };
//! let communities = louvain_communities(&graph)?;
//!
//! println!("Found {} communities", communities.len());
//! for (i, community) in communities.iter().enumerate() {
//!     println!("Community {}: {} nodes", i, community.len());
//! }
//! ```
//!
//! # Usage Patterns
//!
//! ## When to Use PageRank
//!
//! - **Identify influential nodes** in citation networks
//! - **Rank web pages** or documents by link structure
//! - **Find key entities** in knowledge graphs
//! - **Recommendation systems** based on graph structure
//!
//! ## When to Use Betweenness Centrality
//!
//! - **Find bridge nodes** connecting communities
//! - **Identify bottlenecks** in communication networks
//! - **Detect control points** in flow networks
//! - **Analyze information flow** in social networks
//!
//! ## When to Use Label Propagation
//!
//! - **Fast community detection** on large graphs
//! - **Exploratory analysis** where speed matters more than quality
//! - **Baseline comparison** for other clustering methods
//! - **Incremental clustering** where results update frequently
//!
//! ## When to Use Louvain
//!
//! - **High-quality community detection** where modularity matters
//! - **Hierarchical clustering** to reveal multi-scale structure
//! - **Research applications** requiring reproducible results
//! - **Final clustering** when offline computation is acceptable
//!
//! # Progress Tracking
//!
//! Long-running algorithms provide `_with_progress` variants:
//!
//! ```rust,ignore
//! use sqlitegraph::{
//!     algo::pagerank_with_progress,
//!     progress::ConsoleProgress
//! };
//!
//! let progress = ConsoleProgress::new();
//! let results = pagerank_with_progress(&graph, progress)?;
//! // Output: PageRank iteration 10/100...
//! ```
//!
//! Progress tracking is available for:
//! - [`pagerank_with_progress`]
//! - [`betweenness_centrality_with_progress`]
//! - [`louvain_communities_with_progress`]
//! - [`weakly_connected_components_with_progress`]
//! - [`transitive_closure_with_progress`]
//! - [`transitive_reduction_with_progress`]
//! - [`reachable_from_with_progress`]
//! - [`reverse_reachable_from_with_progress`]
//! - [`dominators_with_progress`]
//! - [`post_dominators_with_progress`]
//! - [`dominance_frontiers_with_progress`]
//! - [`natural_loops_with_progress`]
//! - [`enumerate_paths_with_progress`]
//! - [`enumerate_paths_with_dominance_progress`]
//!
//! Use [`NoProgress`] for zero-overhead progress tracking (default).

// Re-export all public API functions for convenience
// Users can use `crate::algo::pagerank` instead of `crate::algo::centrality::pagerank`

// Centrality algorithms
pub use centrality::{
    betweenness_centrality, betweenness_centrality_with_progress, pagerank, pagerank_with_progress,
};

// Community detection algorithms
pub use community::{label_propagation, louvain_communities, louvain_communities_with_progress};

// Structural analysis algorithms
pub use structure::{connected_components, find_cycles_limited, nodes_by_degree};
pub use scc::{strongly_connected_components, SccResult};
pub use wcc::{weakly_connected_components, weakly_connected_components_with_progress};
pub use topological_sort::{topological_sort, TopoError};

// Reachability analysis algorithms
pub use transitive_closure::{transitive_closure, transitive_closure_with_progress, TransitiveClosureBounds};
pub use transitive_reduction::{transitive_reduction, transitive_reduction_with_progress};
pub use reachability::{
    can_reach,
    reachable_from,
    reachable_from_with_progress,
    reverse_reachable_from,
    reverse_reachable_from_with_progress,
    unreachable_from,
};

// Control flow analysis algorithms
pub use dominance_frontiers::{
    dominance_frontiers, dominance_frontiers_with_progress, iterated_dominance_frontiers,
    DominanceFrontierResult, IteratedDominanceFrontierResult,
};
pub use dominators::{dominators, dominators_with_progress, DominatorResult};
pub use natural_loops::{natural_loops, natural_loops_with_progress, NaturalLoop, NaturalLoopsResult};
pub use post_dominators::{
    post_dominators, post_dominators_auto_exit, post_dominators_with_progress, PostDominatorResult,
};
pub use control_dependence::{
    control_dependence_graph, control_dependence_from_exit, ControlDependenceResult,
};

// Path analysis algorithms
pub use path_enumeration::{
    enumerate_paths, enumerate_paths_with_progress, enumerate_paths_with_dominance,
    enumerate_paths_with_dominance_progress, EnumeratedPath, PathClassification,
    PathEnumerationConfig, PathEnumerationDominanceConfig, PathEnumerationPruningStats,
    PathEnumerationResult,
};

// Module declarations
mod centrality;
mod community;
mod control_dependence;
mod dominance_frontiers;
mod dominators;
mod natural_loops;
mod path_enumeration;
mod post_dominators;
mod reachability;
mod scc;
mod structure;
mod topological_sort;
mod transitive_closure;
mod transitive_reduction;
mod wcc;

// Test module
#[cfg(test)]
mod tests;

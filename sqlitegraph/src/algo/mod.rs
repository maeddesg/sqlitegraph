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
//!
//! ## Reachability Analysis
//!
//! - [`transitive_closure`] - Compute all-pairs reachability (can reach relationships)
//! - [`TransitiveClosureBounds`] - Bounds for limiting transitive closure computation
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
//!
//! Where:
//! - V = number of vertices
//! - E = number of edges
//! - k = number of iterations (algorithm-dependent)
//!
//! # Input Requirements
//!
//! ## Graph Connectivity
//!
//! - **Connected components**: Works on disconnected graphs (finds all components)
//! - **Weakly connected components**: Works on disconnected graphs (finds all components treating edges as undirected)
//! - **Strongly connected components**: Works on disconnected graphs (finds all SCCs including trivial)
//! - **PageRank**: Handles disconnected components (splits rank)
//! - **Betweenness**: Handles disconnected components (each component separately)
//! - **Label propagation**: Works on disconnected graphs (each component independently)
//! - **Louvain**: Works on disconnected graphs (each component independently)
//! - **Transitive closure**: Works on disconnected graphs (each component independently)
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

// Reachability analysis algorithms
pub use transitive_closure::{transitive_closure, transitive_closure_with_progress, TransitiveClosureBounds};

// Module declarations
mod centrality;
mod community;
mod scc;
mod structure;
mod transitive_closure;
mod wcc;

// Test module
#[cfg(test)]
mod tests;

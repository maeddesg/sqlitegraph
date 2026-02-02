//! Graph cut and partitioning algorithms.
//!
//! This module provides algorithms for computing minimum cuts in directed graphs,
//! enabling fault tolerance analysis, security boundary analysis, and critical node
//! identification for distributed systems design.
//!
//! # Available Algorithms
//!
//! - [`min_st_cut`] - Minimum s-t edge cut (smallest set of edges whose removal disconnects source from target)
//! - [`min_st_cut_with_progress`] - Minimum s-t edge cut with progress tracking
//! - [`min_vertex_cut`] - Minimum vertex cut (smallest set of vertices whose removal disconnects source from target)
//! - [`min_vertex_cut_with_progress`] - Minimum vertex cut with progress tracking
//!
//! # When to Use Cut Algorithms
//!
//! ## Minimum s-t Edge Cut (`min_st_cut`)
//!
//! - **Fault Tolerance Analysis**: Identify minimum edges whose removal disconnects components
//! - **Security Boundary Analysis**: Find minimal attack surface between components
//! - **Network Bottleneck Identification**: Discover critical edges for network flow
//! - **Partitioning**: Evaluate cut quality for graph partitioning
//!
//! ## Minimum Vertex Cut (`min_vertex_cut`)
//!
//! - **Critical Node Identification**: Find single points of failure
//! - **Redundancy Planning**: Determine minimum nodes needed for resilience
//! - **Separation Analysis**: Identify vertices that separate graph components
//! - **Dominance Analysis**: Find vertices that control connectivity
//!
//! # Algorithm
//!
//! ## Minimum s-t Edge Cut
//!
//! Uses the **max-flow min-cut theorem** with Edmonds-Karp algorithm:
//! 1. Build flow network with unit capacities (each edge capacity = 1)
//! 2. Run Edmonds-Karp (BFS-based Ford-Fulkerson) to find max flow
//! 3. Find minimum cut from residual graph: nodes reachable from source form source_side
//! 4. Cut edges are edges from source_side to sink_side with zero residual capacity
//!
//! The max-flow min-cut theorem states that the value of the maximum flow equals
//! the capacity of the minimum cut. In unit-capacity networks, this gives us the
//! minimum number of edges whose removal disconnects source from sink.
//!
//! ## Minimum Vertex Cut
//!
//! Uses **vertex splitting transformation** to convert vertex cut to edge cut:
//! 1. For each vertex x (where x != source and x != sink), create x_in and x_out nodes
//! 2. Add edge (x_in, x_out) with capacity 1 (limits vertex to being used once)
//! 3. For each original edge (u, v), add edges (u_out, v_in) with capacity 1
//! 4. Run max-flow on transformed graph
//! 5. Vertices corresponding to saturated (x_in, x_out) edges form the minimum vertex cut
//!
//! # Complexity
//!
//! - **Time (Edge Cut)**: O(|V| * |E|²) for Edmonds-Karp where V = vertices, E = edges
//! - **Time (Vertex Cut)**: O(|V| * |E|²) but with ~2V vertices in transformed graph
//! - **Space**: O(|V| + |E|) for residual graph and BFS queue
//!
//! Where:
//! - V = number of vertices
//! - E = number of edges
//!
//! # Example
//!
//! ```rust,ignore
//! use sqlitegraph::{SqliteGraph, algo::{min_st_cut, min_vertex_cut}};
//!
//! let graph = SqliteGraph::open_in_memory()?;
//! // ... build graph ...
//!
//! // Find minimum edge cut between node 1 and node 5
//! let edge_cut = min_st_cut(&graph, 1, 5)?;
//! println!("Need to remove {} edges to disconnect", edge_cut.cut_size);
//! println!("Cut edges: {:?}", edge_cut.cut_edges);
//!
//! // Find minimum vertex cut
//! let vertex_cut = min_vertex_cut(&graph, 1, 5)?;
//! println!("Need to remove {} vertices to disconnect", vertex_cut.cut_size);
//! println!("Separator: {:?}", vertex_cut.separator);
//! ```

use std::collections::{HashMap, VecDeque};

use ahash::AHashSet;

use crate::errors::SqliteGraphError;
use crate::graph::SqliteGraph;
use crate::progress::ProgressCallback;

// ============================================================================
// Result Types
// ============================================================================

/// Result of minimum s-t edge cut computation.
///
/// Represents the minimum set of edges whose removal disconnects the source
/// from the target in a directed graph.
///
/// # Fields
///
/// - `source_side`: Nodes reachable from source in the residual graph (after max-flow)
/// - `sink_side`: All other nodes (complement of source_side)
/// - `cut_edges`: Edges crossing from source_side to sink_side (the minimum cut)
/// - `cut_size`: Number of edges in the minimum cut (equals max flow value)
///
/// # Example
///
/// ```rust,ignore
/// # use sqlitegraph::algo::min_st_cut;
/// # use sqlitegraph::SqliteGraph;
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph ...
/// let result = min_st_cut(&graph, 1, 5)?;
///
/// println!("Source side: {:?}", result.source_side);
/// println!("Sink side: {:?}", result.sink_side);
/// println!("Cut edges (remove these to disconnect): {:?}", result.cut_edges);
/// println!("Cut size: {} (min edges to remove)", result.cut_size);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinCutResult {
    /// Source side of the cut (nodes reachable from source in residual graph)
    pub source_side: AHashSet<i64>,
    /// Sink side of the cut (all other nodes)
    pub sink_side: AHashSet<i64>,
    /// Edges crossing the cut (from source_side to sink_side)
    pub cut_edges: Vec<(i64, i64)>,
    /// Number of edges in the minimum cut
    pub cut_size: usize,
}

/// Result of minimum vertex cut computation.
///
/// Represents the minimum set of vertices whose removal disconnects the source
/// from the target in a directed graph.
///
/// # Fields
///
/// - `separator`: Vertices whose removal disconnects source from target
/// - `source_side`: Nodes that can reach separator without using separator nodes
/// - `sink_side`: Nodes that can be reached from source after passing through separator
/// - `cut_size`: Number of vertices in the minimum vertex cut
///
/// # Example
///
/// ```rust,ignore
/// # use sqlitegraph::algo::min_vertex_cut;
/// # use sqlitegraph::SqliteGraph;
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph ...
/// let result = min_vertex_cut(&graph, 1, 5)?;
///
/// println!("Separator (remove these vertices): {:?}", result.separator);
/// println!("Source side: {:?}", result.source_side);
/// println!("Sink side: {:?}", result.sink_side);
/// println!("Cut size: {} (min vertices to remove)", result.cut_size);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinVertexCutResult {
    /// Set of vertices whose removal disconnects source from target
    pub separator: AHashSet<i64>,
    /// Source side (can reach separator without using separator nodes)
    pub source_side: AHashSet<i64>,
    /// Sink side (can be reached from separator)
    pub sink_side: AHashSet<i64>,
    /// Number of vertices in the minimum vertex cut
    pub cut_size: usize,
}

// ============================================================================
// Internal Flow Network Types
// ============================================================================

/// Flow network edge with capacity and flow tracking.
#[derive(Debug, Clone)]
struct FlowEdge {
    to: i64,
    capacity: usize,
    flow: usize,
}

impl FlowEdge {
    fn new(to: i64, capacity: usize) -> Self {
        Self {
            to,
            capacity,
            flow: 0,
        }
    }

    /// Residual capacity (can still send this much flow)
    fn residual(&self) -> usize {
        self.capacity - self.flow
    }

    /// Add flow to edge (returns excess if capacity exceeded)
    fn add_flow(&mut self, amount: usize) -> usize {
        let can_add = self.residual().min(amount);
        self.flow += can_add;
        amount - can_add
    }
}

/// Flow network for max-flow computation.
struct FlowNetwork {
    /// Adjacency list: node -> list of outgoing edges
    adjacency: HashMap<i64, Vec<FlowEdge>>,
    /// Map from (from, to, index) to reverse edge index for residual updates
    /// Key: (from, to), Value: reverse_edge_index in from's adjacency list
    reverse_edge: HashMap<(i64, i64), usize>,
}

impl FlowNetwork {
    /// Create a new empty flow network.
    fn new() -> Self {
        Self {
            adjacency: HashMap::new(),
            reverse_edge: HashMap::new(),
        }
    }

    /// Add an edge with given capacity (also adds reverse edge with 0 capacity).
    fn add_edge(&mut self, from: i64, to: i64, capacity: usize) {
        // Skip self-loops in flow network
        if from == to {
            return;
        }

        // Forward edge index
        let forward_idx = self.adjacency.entry(from).or_insert_with(Vec::new).len();
        // Reverse edge index
        let reverse_idx = self.adjacency.entry(to).or_insert_with(Vec::new).len();

        // Add forward edge
        self.adjacency.entry(from).or_insert_with(Vec::new).push(FlowEdge::new(to, capacity));
        // Add reverse edge (for residual graph)
        self.adjacency.entry(to).or_insert_with(Vec::new).push(FlowEdge::new(from, 0));

        // Track reverse edges for updates
        self.reverse_edge.insert((from, to), reverse_idx);
        self.reverse_edge.insert((to, from), forward_idx);
    }

    /// Get all neighbors of a node.
    fn neighbors(&self, node: i64) -> &[FlowEdge] {
        self.adjacency.get(&node).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get all nodes in the network.
    fn nodes(&self) -> AHashSet<i64> {
        self.adjacency.keys().copied().collect()
    }

    /// Find nodes reachable from source using edges with positive residual capacity.
    fn reachable_residual(&self, source: i64) -> AHashSet<i64> {
        let mut visited = AHashSet::new();
        let mut queue = VecDeque::new();

        visited.insert(source);
        queue.push_back(source);

        while let Some(node) = queue.pop_front() {
            for edge in self.neighbors(node) {
                if edge.residual() > 0 && visited.insert(edge.to) {
                    queue.push_back(edge.to);
                }
            }
        }

        visited
    }

    /// Find min cut edges after max-flow computation.
    fn find_cut_edges(&self, source_side: &AHashSet<i64>) -> Vec<(i64, i64)> {
        let mut cut_edges = Vec::new();

        for &from in source_side {
            for edge in self.neighbors(from) {
                // Edge crosses from source_side to sink_side if:
                // - From node is in source_side
                // - To node is NOT in source_side (in sink_side)
                // - Edge has zero residual capacity (saturated)
                if !source_side.contains(&edge.to) && edge.residual() == 0 {
                    cut_edges.push((from, edge.to));
                }
            }
        }

        cut_edges
    }
}

// ============================================================================
// Edmonds-Karp Max-Flow Algorithm
// ============================================================================

/// Run Edmonds-Karp max-flow algorithm from source to sink.
///
/// Returns (max_flow_value, flow_network) where the flow_network contains
/// the residual capacities after max-flow computation.
///
/// # Algorithm
/// 1. Initialize all flows to 0
/// 2. While there exists an augmenting path from source to sink in residual graph:
///    a. Find path using BFS (guarantees shortest path)
///    b. Compute bottleneck capacity along path
///    c. Augment flow along path (update forward/reverse edges)
/// 3. Return max flow value
///
/// # Complexity
/// - Time: O(V * E²) where V = vertices, E = edges
/// - Space: O(V + E) for residual graph and BFS queue
fn edmonds_karp(
    mut network: FlowNetwork,
    source: i64,
    sink: i64,
) -> (usize, FlowNetwork) {
    let mut max_flow = 0;

    // Find augmenting paths while they exist
    while let Some(path) = bfs_augmenting_path(&network, source, sink) {
        // Find bottleneck capacity along path
        let bottleneck = find_bottleneck(&network, &path);

        // Augment flow along path
        augment_flow(&mut network, &path, bottleneck);

        max_flow += bottleneck;
    }

    (max_flow, network)
}

/// Find augmenting path using BFS (Edmonds-Karp).
///
/// Returns Some(path) if path exists, None if sink not reachable.
/// Path is a list of node IDs from source to sink.
fn bfs_augmenting_path(network: &FlowNetwork, source: i64, sink: i64) -> Option<Vec<i64>> {
    let mut parent: HashMap<i64, (i64, usize)> = HashMap::new();
    let mut queue = VecDeque::new();

    queue.push_back(source);
    parent.insert(source, (source, 0)); // Special marker for source

    while let Some(node) = queue.pop_front() {
        if node == sink {
            // Reconstruct path
            let mut path = vec![sink];
            let mut current = sink;

            while current != source {
                let (prev_node, _edge_idx) = *parent.get(&current)?;
                path.push(prev_node);
                current = prev_node;
            }

            path.reverse();
            return Some(path);
        }

        // Explore neighbors with positive residual capacity
        for (edge_idx, edge) in network.neighbors(node).iter().enumerate() {
            if edge.residual() > 0 && !parent.contains_key(&edge.to) {
                parent.insert(edge.to, (node, edge_idx));
                queue.push_back(edge.to);
            }
        }
    }

    None // No augmenting path found
}

/// Find bottleneck capacity along an augmenting path.
fn find_bottleneck(network: &FlowNetwork, path: &[i64]) -> usize {
    let mut bottleneck = usize::MAX;

    for i in 0..path.len().saturating_sub(1) {
        let from = path[i];
        let to = path[i + 1];

        for edge in network.neighbors(from) {
            if edge.to == to {
                bottleneck = bottleneck.min(edge.residual());
                break;
            }
        }
    }

    bottleneck
}

/// Augment flow along a path by the given amount.
fn augment_flow(network: &mut FlowNetwork, path: &[i64], amount: usize) {
    for i in 0..path.len().saturating_sub(1) {
        let from = path[i];
        let to = path[i + 1];

        // Update forward edge
        if let Some(forward_edges) = network.adjacency.get_mut(&from) {
            for edge in forward_edges.iter_mut() {
                if edge.to == to {
                    edge.flow += amount;
                    break;
                }
            }
        }

        // Update reverse edge
        if let Some(reverse_edges) = network.adjacency.get_mut(&to) {
            for edge in reverse_edges.iter_mut() {
                if edge.to == from {
                    // Reduce flow on reverse edge (equivalent to increasing capacity)
                    edge.flow = edge.flow.saturating_sub(amount);
                    break;
                }
            }
        }
    }
}

// ============================================================================
// Build Flow Network from Graph
// ============================================================================

/// Build unit-capacity flow network from graph for min-cut computation.
///
/// Creates a flow network where each edge has capacity 1 (unweighted graph).
/// Self-loops are filtered out as they don't affect s-t connectivity.
fn build_flow_network(graph: &SqliteGraph, source: i64, sink: i64) -> FlowNetwork {
    let mut network = FlowNetwork::new();

    // Collect all nodes that might be in paths
    let mut nodes_to_visit = vec![source];
    let mut visited = AHashSet::new();
    visited.insert(source);

    // BFS to find all nodes reachable from source
    while let Some(node) = nodes_to_visit.pop() {
        if let Ok(neighbors) = graph.fetch_outgoing(node) {
            for &neighbor in &neighbors {
                // Add edge to flow network (unit capacity)
                network.add_edge(node, neighbor, 1);

                if visited.insert(neighbor) {
                    nodes_to_visit.push(neighbor);
                }
            }
        }
    }

    network
}

// ============================================================================
// Public API: Minimum s-t Edge Cut
// ============================================================================

/// Compute minimum s-t edge cut using max-flow min-cut theorem.
///
/// Returns the smallest set of edges whose removal disconnects `source` from `sink`.
/// Uses Edmonds-Karp algorithm for max-flow computation with unit capacities.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `source` - The source node ID
/// * `sink` - The sink (target) node ID
///
/// # Returns
/// `MinCutResult` containing source_side, sink_side, cut_edges, and cut_size.
///
/// # Errors
/// - Returns `SqliteGraphError::NotFound` if source or sink doesn't exist
/// - Returns error if graph traversal fails
///
/// # Complexity
/// - **Time**: O(|V| * |E|²) for Edmonds-Karp where V = vertices, E = edges
/// - **Space**: O(|V| + |E|) for residual graph and BFS queue
///
/// # Edge Cases
/// - **Source == Sink**: Returns empty cut (trivially connected)
/// - **Disconnected nodes**: Returns empty cut (no path = zero cut capacity)
/// - **Single edge graph**: Returns cut containing that single edge
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::min_st_cut};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph: 1 -> 2 -> 3 -> 4 -> 5 ...
///
/// let result = min_st_cut(&graph, 1, 5)?;
/// println!("Min cut size: {}", result.cut_size);
/// println!("Cut edges: {:?}", result.cut_edges);
/// ```
pub fn min_st_cut(
    graph: &SqliteGraph,
    source: i64,
    sink: i64,
) -> Result<MinCutResult, SqliteGraphError> {
    // Handle trivial case: source equals sink
    if source == sink {
        return Ok(MinCutResult {
            source_side: {
                let mut set = AHashSet::new();
                set.insert(source);
                set
            },
            sink_side: AHashSet::new(),
            cut_edges: vec![],
            cut_size: 0,
        });
    }

    // Build flow network
    let network = build_flow_network(graph, source, sink);

    // Check if sink is reachable from source
    if network.nodes().contains(&source) && !network.nodes().contains(&sink) {
        // Sink not in network means no path exists
        return Ok(MinCutResult {
            source_side: network.nodes(),
            sink_side: AHashSet::new(),
            cut_edges: vec![],
            cut_size: 0,
        });
    }

    // Run Edmonds-Karp max-flow
    let (max_flow, residual_network) = edmonds_karp(network, source, sink);

    // Extract cut from residual graph
    let source_side = residual_network.reachable_residual(source);
    let all_nodes = residual_network.nodes();
    let sink_side = all_nodes.difference(&source_side).copied().collect();
    let cut_edges = residual_network.find_cut_edges(&source_side);

    Ok(MinCutResult {
        source_side,
        sink_side,
        cut_edges,
        cut_size: max_flow,
    })
}

/// Compute minimum s-t edge cut with progress tracking.
///
/// Same algorithm as [`min_st_cut`] but reports progress during execution.
/// Useful for long-running operations on large graphs.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `source` - The source node ID
/// * `sink` - The sink (target) node ID
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// `MinCutResult` containing source_side, sink_side, cut_edges, and cut_size.
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Current BFS iteration
/// - `total`: None (unknown total iterations for Edmonds-Karp)
/// - `message`: "Min cut: iteration {current}, flow so far: {flow}"
///
/// Progress is reported after each augmenting path is found.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::min_st_cut_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let result = min_st_cut_with_progress(&graph, 1, 5, &progress)?;
/// // Output: Min cut: iteration 1, flow so far: 1...
/// // Output: Min cut: iteration 2, flow so far: 2...
/// ```
pub fn min_st_cut_with_progress<F>(
    graph: &SqliteGraph,
    source: i64,
    sink: i64,
    progress: &F,
) -> Result<MinCutResult, SqliteGraphError>
where
    F: ProgressCallback,
{
    // Handle trivial case: source equals sink
    if source == sink {
        return Ok(MinCutResult {
            source_side: {
                let mut set = AHashSet::new();
                set.insert(source);
                set
            },
            sink_side: AHashSet::new(),
            cut_edges: vec![],
            cut_size: 0,
        });
    }

    // Build flow network
    let network = build_flow_network(graph, source, sink);

    // Check if sink is reachable from source
    if network.nodes().contains(&source) && !network.nodes().contains(&sink) {
        return Ok(MinCutResult {
            source_side: network.nodes(),
            sink_side: AHashSet::new(),
            cut_edges: vec![],
            cut_size: 0,
        });
    }

    // Run Edmonds-Karp with progress tracking
    let mut current_network = network;
    let mut max_flow = 0;
    let mut iteration = 0;

    while let Some(path) = bfs_augmenting_path(&current_network, source, sink) {
        iteration += 1;

        let bottleneck = find_bottleneck(&current_network, &path);
        augment_flow(&mut current_network, &path, bottleneck);
        max_flow += bottleneck;

        // Report progress
        progress.on_progress(
            iteration,
            None,
            &format!("Min cut: iteration {}, flow so far: {}", iteration, max_flow),
        );
    }

    // Report completion
    progress.on_complete();

    // Extract cut from residual graph
    let source_side = current_network.reachable_residual(source);
    let all_nodes = current_network.nodes();
    let sink_side = all_nodes.difference(&source_side).copied().collect();
    let cut_edges = current_network.find_cut_edges(&source_side);

    Ok(MinCutResult {
        source_side,
        sink_side,
        cut_edges,
        cut_size: max_flow,
    })
}

// ============================================================================
// Vertex Splitting Transformation
// ============================================================================

/// Node encoding for vertex splitting transformation.
///
/// For each original vertex x (except source and sink), we create:
/// - x_in: Encoded as x * 2 (even numbers)
/// - x_out: Encoded as x * 2 + 1 (odd numbers)
///
/// Source and sink are not split, so they map to themselves.
///
/// Example:
/// - Original node 5 (not source/sink): 5_in = 10, 5_out = 11
/// - Source node 1: maps to 1 (unchanged)
/// - Sink node 10: maps to 10 (unchanged)
struct VertexSplitTransform {
    source: i64,
    sink: i64,
}

impl VertexSplitTransform {
    fn new(source: i64, sink: i64) -> Self {
        Self { source, sink }
    }

    /// Get the "in" node for a vertex (x_in).
    fn node_in(&self, x: i64) -> i64 {
        if x == self.source || x == self.sink {
            x // Source and sink are not split
        } else {
            x * 2
        }
    }

    /// Get the "out" node for a vertex (x_out).
    fn node_out(&self, x: i64) -> i64 {
        if x == self.source || x == self.sink {
            x // Source and sink are not split
        } else {
            x * 2 + 1
        }
    }

    /// Check if a split node represents the original node.
    fn is_original_node(&self, node_id: i64, original: i64) -> bool {
        if original == self.source || original == self.sink {
            node_id == original
        } else {
            node_id == original * 2 || node_id == original * 2 + 1
        }
    }

    /// Map a split node back to its original node ID.
    fn to_original(&self, node_id: i64) -> i64 {
        if node_id == self.source || node_id == self.sink {
            node_id
        } else if node_id % 2 == 0 {
            node_id / 2  // x_in -> x
        } else {
            (node_id - 1) / 2  // x_out -> x
        }
    }

    /// Check if an edge is the internal (x_in, x_out) edge for a vertex.
    fn is_internal_edge(&self, from: i64, to: i64) -> Option<i64> {
        // Internal edges are (x*2, x*2+1) for non-source/sink nodes
        if from % 2 == 0 && to == from + 1 {
            let original = from / 2;
            if original != self.source && original != self.sink {
                return Some(original);
            }
        }
        None
    }
}

/// Build vertex-splitting transformed flow network.
///
/// For each vertex x (except source and sink):
/// - Create x_in and x_out nodes
/// - Add edge (x_in, x_out) with capacity 1
///
/// For each original edge (u, v):
/// - Add edge (u_out, v_in) with capacity 1
fn build_vertex_split_network(
    graph: &SqliteGraph,
    source: i64,
    sink: i64,
) -> (FlowNetwork, VertexSplitTransform) {
    let transform = VertexSplitTransform::new(source, sink);
    let mut network = FlowNetwork::new();

    // Collect nodes reachable from source
    let mut nodes_to_visit = vec![source];
    let mut visited = AHashSet::new();
    visited.insert(source);

    while let Some(node) = nodes_to_visit.pop() {
        if let Ok(neighbors) = graph.fetch_outgoing(node) {
            for &neighbor in &neighbors {
                let node_out = transform.node_out(node);
                let neighbor_in = transform.node_in(neighbor);

                // Add edge (u_out, v_in) for original edge (u, v)
                network.add_edge(node_out, neighbor_in, 1);

                // Add internal edge (v_in, v_out) if not already added
                let neighbor_out = transform.node_out(neighbor);
                if neighbor != source && neighbor != sink {
                    network.add_edge(neighbor_in, neighbor_out, 1);
                }

                // Also add internal edge for current node if needed
                let node_in = transform.node_in(node);
                if node != source && node != sink {
                    network.add_edge(node_in, node_out, 1);
                }

                if visited.insert(neighbor) {
                    nodes_to_visit.push(neighbor);
                }
            }
        }
    }

    // Ensure source and sink have their internal edges
    let source_in = transform.node_in(source);
    let source_out = transform.node_out(source);
    if source_in != source_out {
        network.add_edge(source_in, source_out, 1);
    }

    (network, transform)
}

// ============================================================================
// Public API: Minimum Vertex Cut
// ============================================================================

/// Compute minimum vertex cut using vertex splitting transformation.
///
/// Returns the smallest set of vertices whose removal disconnects `source` from `sink`.
/// Uses vertex splitting to convert vertex cut to edge cut, then applies max-flow.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `source` - The source node ID
/// * `target` - The target node ID
///
/// # Returns
/// `MinVertexCutResult` containing separator, source_side, sink_side, and cut_size.
///
/// # Algorithm
/// 1. Transform graph using vertex splitting: each vertex x becomes (x_in, x_out)
/// 2. Add edge (x_in, x_out) with capacity 1 for each vertex
/// 3. For each original edge (u, v), add edge (u_out, v_in) with capacity 1
/// 4. Run max-flow on transformed graph
/// 5. Extract separator: vertices where (x_in, x_out) edge is saturated
///
/// # Complexity
/// - **Time**: O(|V| * |E|²) for Edmonds-Karp with ~2V vertices
/// - **Space**: O(|V| + |E|) for transformed graph and residual graph
///
/// # Edge Cases
/// - **Source == Target**: Returns empty separator (trivially connected)
/// - **Direct edge source->target**: Returns empty separator (no intermediate vertices)
/// - **Disconnected nodes**: Returns empty separator
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::min_vertex_cut};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph ...
///
/// let result = min_vertex_cut(&graph, 1, 5)?;
/// println!("Remove these vertices to disconnect: {:?}", result.separator);
/// println!("Separator size: {}", result.cut_size);
/// ```
pub fn min_vertex_cut(
    graph: &SqliteGraph,
    source: i64,
    target: i64,
) -> Result<MinVertexCutResult, SqliteGraphError> {
    // Handle trivial case: source equals target
    if source == target {
        return Ok(MinVertexCutResult {
            separator: AHashSet::new(),
            source_side: {
                let mut set = AHashSet::new();
                set.insert(source);
                set
            },
            sink_side: AHashSet::new(),
            cut_size: 0,
        });
    }

    // Build vertex-splitting transformed network
    let (network, transform) = build_vertex_split_network(graph, source, target);

    let source_out = transform.node_out(source);
    let target_in = transform.node_in(target);

    // Check if target is reachable
    if !network.nodes().contains(&target_in) {
        return Ok(MinVertexCutResult {
            separator: AHashSet::new(),
            source_side: {
                let mut set = AHashSet::new();
                set.insert(source);
                set
            },
            sink_side: AHashSet::new(),
            cut_size: 0,
        });
    }

    // Run max-flow on transformed graph
    let (max_flow, residual_network) = edmonds_karp(network, source_out, target_in);

    // Extract separator: vertices where internal edge is saturated
    let mut separator = AHashSet::new();
    for node in residual_network.nodes() {
        for edge in residual_network.neighbors(node) {
            if let Some(original) = transform.is_internal_edge(node, edge.to) {
                // Internal edge is saturated if residual capacity is 0
                if edge.residual() == 0 {
                    separator.insert(original);
                }
            }
        }
    }

    // Compute source_side and sink_side from residual graph
    let source_side_transformed = residual_network.reachable_residual(source_out);
    let mut source_side = AHashSet::new();
    for node in source_side_transformed {
        source_side.insert(transform.to_original(node));
    }

    let all_nodes_transformed = residual_network.nodes();
    let mut sink_side = AHashSet::new();
    for node in all_nodes_transformed {
        let original = transform.to_original(node);
        if !source_side.contains(&original) {
            sink_side.insert(original);
        }
    }

    Ok(MinVertexCutResult {
        separator,
        source_side,
        sink_side,
        cut_size: max_flow,
    })
}

/// Compute minimum vertex cut with progress tracking.
///
/// Same algorithm as [`min_vertex_cut`] but reports progress during execution.
/// Useful for long-running operations on large graphs.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `source` - The source node ID
/// * `target` - The target node ID
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// `MinVertexCutResult` containing separator, source_side, sink_side, and cut_size.
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Current BFS iteration
/// - `total`: None (unknown total iterations for Edmonds-Karp)
/// - `message`: "Vertex cut: iteration {current}, flow so far: {flow}"
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::min_vertex_cut_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let result = min_vertex_cut_with_progress(&graph, 1, 5, &progress)?;
/// ```
pub fn min_vertex_cut_with_progress<F>(
    graph: &SqliteGraph,
    source: i64,
    target: i64,
    progress: &F,
) -> Result<MinVertexCutResult, SqliteGraphError>
where
    F: ProgressCallback,
{
    // Handle trivial case: source equals target
    if source == target {
        return Ok(MinVertexCutResult {
            separator: AHashSet::new(),
            source_side: {
                let mut set = AHashSet::new();
                set.insert(source);
                set
            },
            sink_side: AHashSet::new(),
            cut_size: 0,
        });
    }

    // Build vertex-splitting transformed network
    let (network, transform) = build_vertex_split_network(graph, source, target);

    let source_out = transform.node_out(source);
    let target_in = transform.node_in(target);

    // Check if target is reachable
    if !network.nodes().contains(&target_in) {
        return Ok(MinVertexCutResult {
            separator: AHashSet::new(),
            source_side: {
                let mut set = AHashSet::new();
                set.insert(source);
                set
            },
            sink_side: AHashSet::new(),
            cut_size: 0,
        });
    }

    // Run Edmonds-Karp with progress tracking
    let mut current_network = network;
    let mut max_flow = 0;
    let mut iteration = 0;

    while let Some(path) = bfs_augmenting_path(&current_network, source_out, target_in) {
        iteration += 1;

        let bottleneck = find_bottleneck(&current_network, &path);
        augment_flow(&mut current_network, &path, bottleneck);
        max_flow += bottleneck;

        // Report progress
        progress.on_progress(
            iteration,
            None,
            &format!("Vertex cut: iteration {}, flow so far: {}", iteration, max_flow),
        );
    }

    // Report completion
    progress.on_complete();

    // Extract separator: vertices where internal edge is saturated
    let mut separator = AHashSet::new();
    for node in current_network.nodes() {
        for edge in current_network.neighbors(node) {
            if let Some(original) = transform.is_internal_edge(node, edge.to) {
                if edge.residual() == 0 {
                    separator.insert(original);
                }
            }
        }
    }

    // Compute source_side and sink_side from residual graph
    let source_side_transformed = current_network.reachable_residual(source_out);
    let mut source_side = AHashSet::new();
    for node in source_side_transformed {
        source_side.insert(transform.to_original(node));
    }

    let all_nodes_transformed = current_network.nodes();
    let mut sink_side = AHashSet::new();
    for node in all_nodes_transformed {
        let original = transform.to_original(node);
        if !source_side.contains(&original) {
            sink_side.insert(original);
        }
    }

    Ok(MinVertexCutResult {
        separator,
        source_side,
        sink_side,
        cut_size: max_flow,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphEdge, GraphEntity};

    /// Helper: Create linear chain graph: s -> a -> b -> t
    fn create_linear_chain() -> (SqliteGraph, i64, i64) {
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

        (graph, entity_ids[0], entity_ids[3])
    }

    /// Helper: Create diamond graph: s -> a, s -> b, a -> t, b -> t
    fn create_diamond() -> (SqliteGraph, i64, i64) {
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

        (graph, entity_ids[0], entity_ids[3])
    }

    /// Helper: Create parallel paths: s -> a -> t, s -> b -> t, s -> c -> t
    fn create_parallel_paths() -> (SqliteGraph, i64, i64) {
        let graph = SqliteGraph::open_in_memory().expect("Failed to create graph");

        // Create 5 nodes: s, a, b, c, t
        for i in 0..5 {
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

        // Create parallel paths: s(0) -> a(1) -> t(4), s -> b(2) -> t, s -> c(3) -> t
        let edges = vec![(0, 1), (1, 4), (0, 2), (2, 4), (0, 3), (3, 4)];
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

        (graph, entity_ids[0], entity_ids[4])
    }

    /// Helper: Create single edge graph: s -> t
    fn create_single_edge() -> (SqliteGraph, i64, i64) {
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

        // Create single edge
        let edge = GraphEdge {
            id: 0,
            from_id: entity_ids[0],
            to_id: entity_ids[1],
            edge_type: "next".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge).expect("Failed to insert edge");

        (graph, entity_ids[0], entity_ids[1])
    }

    /// Helper: Create disconnected graph: s -> a, b -> t
    fn create_disconnected() -> (SqliteGraph, i64, i64) {
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

        // Create disconnected components: 0 -> 1 and 2 -> 3
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

        (graph, entity_ids[0], entity_ids[3])
    }

    // Tests for min_st_cut

    #[test]
    fn test_min_st_cut_linear_chain() {
        // Scenario: Linear chain s -> a -> b -> t
        // Expected: cut_size = 1 (any single edge disconnects)
        let (graph, source, sink) = create_linear_chain();

        let result = min_st_cut(&graph, source, sink).expect("Failed to compute min cut");

        assert_eq!(result.cut_size, 1, "Linear chain should have cut size 1");
        assert_eq!(result.cut_edges.len(), 1, "Should have 1 cut edge");
        assert!(
            result.source_side.contains(&source),
            "Source side should contain source"
        );
        assert!(
            result.sink_side.contains(&sink),
            "Sink side should contain sink"
        );
    }

    #[test]
    fn test_min_st_cut_diamond() {
        // Scenario: Diamond s -> a, s -> b, a -> t, b -> t
        // Expected: cut_size = 2 (must cut both a->t and b->t, or both s->a and s->b)
        let (graph, source, sink) = create_diamond();

        let result = min_st_cut(&graph, source, sink).expect("Failed to compute min cut");

        assert_eq!(result.cut_size, 2, "Diamond should have cut size 2");
        assert_eq!(result.cut_edges.len(), 2, "Should have 2 cut edges");
    }

    #[test]
    fn test_min_st_cut_parallel_paths() {
        // Scenario: Three parallel paths s -> a -> t, s -> b -> t, s -> c -> t
        // Expected: cut_size = 3 (must cut all three edges into t)
        let (graph, source, sink) = create_parallel_paths();

        let result = min_st_cut(&graph, source, sink).expect("Failed to compute min cut");

        assert_eq!(
            result.cut_size,
            3,
            "Parallel paths should have cut size 3"
        );
        assert_eq!(result.cut_edges.len(), 3, "Should have 3 cut edges");
    }

    #[test]
    fn test_min_st_cut_single_edge() {
        // Scenario: Single edge s -> t
        // Expected: cut_size = 1, cut = {(s, t)}
        let (graph, source, sink) = create_single_edge();

        let result = min_st_cut(&graph, source, sink).expect("Failed to compute min cut");

        assert_eq!(result.cut_size, 1, "Single edge should have cut size 1");
        assert_eq!(result.cut_edges.len(), 1, "Should have 1 cut edge");
        assert_eq!(
            result.cut_edges[0],
            (source, sink),
            "Cut edge should be (source, sink)"
        );
    }

    #[test]
    fn test_min_st_cut_source_equals_target() {
        // Scenario: source == target
        // Expected: Empty cut result
        let (graph, source, _) = create_single_edge();

        let result = min_st_cut(&graph, source, source).expect("Failed to compute min cut");

        assert_eq!(result.cut_size, 0, "Source==target should have cut size 0");
        assert!(result.cut_edges.is_empty(), "Cut edges should be empty");
        assert!(result.source_side.contains(&source), "Source side contains source");
        assert!(result.sink_side.is_empty(), "Sink side should be empty");
    }

    #[test]
    fn test_min_st_cut_with_progress_matches() {
        // Scenario: Progress variant matches non-progress variant
        // Expected: Same results
        use crate::progress::NoProgress;

        let (graph, source, sink) = create_diamond();

        let progress = NoProgress;
        let result_with =
            min_st_cut_with_progress(&graph, source, sink, &progress).expect("Failed");
        let result_without = min_st_cut(&graph, source, sink).expect("Failed");

        assert_eq!(
            result_with.cut_size,
            result_without.cut_size,
            "Cut size should match"
        );
        assert_eq!(
            result_with.cut_edges.len(),
            result_without.cut_edges.len(),
            "Cut edges count should match"
        );
    }

    // Tests for min_vertex_cut

    #[test]
    fn test_min_vertex_cut_bridge_node() {
        // Scenario: s -> a -> t
        // Expected: vertex_cut_size = 1, separator = {a}
        let (graph, source, sink) = create_linear_chain();

        let result = min_vertex_cut(&graph, source, sink).expect("Failed to compute vertex cut");

        assert_eq!(
            result.cut_size,
            1,
            "Bridge node should have vertex cut size 1"
        );
        assert_eq!(result.separator.len(), 1, "Should have 1 separator vertex");
    }

    #[test]
    fn test_min_vertex_cut_two_parallel_paths() {
        // Scenario: s -> a -> t, s -> b -> t
        // Expected: vertex_cut_size = 2, separator = {a, b}
        let (graph, source, sink) = create_diamond();

        let result = min_vertex_cut(&graph, source, sink).expect("Failed to compute vertex cut");

        assert_eq!(
            result.cut_size,
            2,
            "Two parallel paths should have vertex cut size 2"
        );
        assert_eq!(result.separator.len(), 2, "Should have 2 separator vertices");
    }

    #[test]
    fn test_min_vertex_cut_direct_edge() {
        // Scenario: Direct edge s -> t
        // Expected: vertex_cut_size = 0 (no intermediate vertices)
        let (graph, source, sink) = create_single_edge();

        let result = min_vertex_cut(&graph, source, sink).expect("Failed to compute vertex cut");

        assert_eq!(
            result.cut_size,
            0,
            "Direct edge should have vertex cut size 0"
        );
        assert!(
            result.separator.is_empty(),
            "Separator should be empty for direct edge"
        );
    }

    #[test]
    fn test_min_vertex_cut_source_equals_target() {
        // Scenario: source == target
        // Expected: Empty separator result
        let (graph, source, _) = create_single_edge();

        let result =
            min_vertex_cut(&graph, source, source).expect("Failed to compute vertex cut");

        assert_eq!(result.cut_size, 0, "Source==target should have cut size 0");
        assert!(result.separator.is_empty(), "Separator should be empty");
        assert!(result.source_side.contains(&source), "Source side contains source");
    }

    #[test]
    fn test_min_vertex_cut_with_progress_matches() {
        // Scenario: Progress variant matches non-progress variant
        // Expected: Same results
        use crate::progress::NoProgress;

        let (graph, source, sink) = create_diamond();

        let progress = NoProgress;
        let result_with =
            min_vertex_cut_with_progress(&graph, source, sink, &progress).expect("Failed");
        let result_without = min_vertex_cut(&graph, source, sink).expect("Failed");

        assert_eq!(
            result_with.cut_size,
            result_without.cut_size,
            "Cut size should match"
        );
        assert_eq!(
            result_with.separator.len(),
            result_without.separator.len(),
            "Separator size should match"
        );
    }

    #[test]
    fn test_min_vertex_cut_three_parallel_paths() {
        // Scenario: Three parallel paths s -> a -> t, s -> b -> t, s -> c -> t
        // Expected: vertex_cut_size = 3, separator = {a, b, c}
        let (graph, source, sink) = create_parallel_paths();

        let result = min_vertex_cut(&graph, source, sink).expect("Failed to compute vertex cut");

        assert_eq!(
            result.cut_size,
            3,
            "Three parallel paths should have vertex cut size 3"
        );
        assert_eq!(result.separator.len(), 3, "Should have 3 separator vertices");
    }
}

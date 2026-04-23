# Adding a Graph Algorithm

**Last Updated:** 2026-02-12  
**Version:** v2.0.0

This guide explains how to add a new graph algorithm to SQLiteGraph.

---

## Overview

Graph algorithms are located in `src/algorithms/`. They work with any backend that implements `GraphBackend` trait (SQLite, V3, or V2).

---

## Current Algorithm Structure

Algorithms are organized by category:

```
src/algorithms/
├── mod.rs              # Module exports
├── traversal.rs        # BFS, DFS, topological sort
├── shortest_path.rs    # Dijkstra, A*, Bellman-Ford
├── centrality.rs       # Degree, betweenness, closeness, PageRank
├── community.rs        # Connected components, Louvain
├── path.rs             # Hamiltonian path detection
├── bipartite.rs        # Bipartite graph checks
├── cycles.rs           # Cycle detection
├── flow.rs             # Max flow algorithms
└── tree.rs             # MST, tree algorithms
```

---

## Step-by-Step Guide

### Step 1: Define Algorithm Interface

Create or update `src/algorithms/your_algorithm.rs`:

```rust
use crate::graph::{GraphBackend, NodeId, EdgeId};
use crate::algorithms::AlgorithmResult;
use serde_json::Value;
use std::collections::HashMap;

/// Configuration for your algorithm
#[derive(Debug, Clone)]
pub struct YourAlgorithmConfig {
    pub max_iterations: usize,
    pub tolerance: f64,
}

impl Default for YourAlgorithmConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-6,
        }
    }
}

/// Result of your algorithm
#[derive(Debug, Clone)]
pub struct YourAlgorithmResult {
    pub scores: HashMap<NodeId, f64>,
    pub iterations: usize,
    pub converged: bool,
}

impl AlgorithmResult for YourAlgorithmResult {
    fn to_json(&self) -> Value {
        serde_json::json!({
            "scores": self.scores,
            "iterations": self.iterations,
            "converged": self.converged,
        })
    }
}

/// Run your algorithm on the graph
pub fn your_algorithm<B: GraphBackend>(
    backend: &B,
    config: &YourAlgorithmConfig,
) -> Result<YourAlgorithmResult, AlgorithmError> {
    // Implementation
}
```

### Step 2: Implement Algorithm

Complete the implementation:

```rust
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, thiserror::Error)]
pub enum AlgorithmError {
    #[error("Graph is empty")]
    EmptyGraph,
    #[error("Convergence failed after {0} iterations")]
    ConvergenceFailed(usize),
    #[error(transparent)]
    BackendError(#[from] crate::error::SqliteGraphError),
}

pub fn your_algorithm<B: GraphBackend>(
    backend: &B,
    config: &YourAlgorithmConfig,
) -> Result<YourAlgorithmResult, AlgorithmError> {
    // Get all nodes
    let nodes: Vec<NodeId> = backend.find_nodes(None, None, None, None)?
        .into_iter()
        .map(|n| n.id)
        .collect();
    
    if nodes.is_empty() {
        return Err(AlgorithmError::EmptyGraph);
    }
    
    // Initialize scores
    let mut scores: HashMap<NodeId, f64> = nodes.iter()
        .map(|&id| (id, 1.0 / nodes.len() as f64))
        .collect();
    
    let mut iterations = 0;
    let mut converged = false;
    
    // Iterative algorithm
    while iterations < config.max_iterations && !converged {
        let mut new_scores = HashMap::new();
        let mut max_change = 0.0;
        
        for &node in &nodes {
            let neighbors = backend.get_neighbors(node, None)?;
            let score = if neighbors.is_empty() {
                scores[&node]  // No neighbors, keep current score
            } else {
                // Calculate new score based on neighbors
                neighbors.iter()
                    .filter_map(|n| scores.get(n))
                    .sum::<f64>() / neighbors.len() as f64
            };
            
            let old_score = scores.get(&node).copied().unwrap_or(0.0);
            max_change = max_change.max((score - old_score).abs());
            new_scores.insert(node, score);
        }
        
        scores = new_scores;
        iterations += 1;
        
        if max_change < config.tolerance {
            converged = true;
        }
    }
    
    if !converged {
        return Err(AlgorithmError::ConvergenceFailed(iterations));
    }
    
    Ok(YourAlgorithmResult {
        scores,
        iterations,
        converged,
    })
}
```

### Step 3: Export from Module

Update `src/algorithms/mod.rs`:

```rust
pub mod your_algorithm;

pub use your_algorithm::{
    your_algorithm,
    YourAlgorithmConfig,
    YourAlgorithmResult,
    AlgorithmError,
};
```

### Step 4: Add to Backend Trait

If algorithm should be accessible via backend API, update `GraphBackend` trait:

```rust
// In src/backend/mod.rs or appropriate trait definition
pub trait GraphBackend {
    // ... existing methods
    
    /// Run your algorithm
    fn run_your_algorithm(
        &self,
        config: &YourAlgorithmConfig,
    ) -> Result<YourAlgorithmResult, AlgorithmError> {
        your_algorithm(self, config)
    }
}
```

### Step 5: Add Tests

Add to `src/algorithms/your_algorithm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::sqlite::SqliteGraphBackend;
    use crate::backend::v3::V3Backend;
    
    fn create_test_graph() -> SqliteGraphBackend {
        let backend = SqliteGraphBackend::create_in_memory().unwrap();
        
        // Create test graph structure
        let n1 = backend.insert_node(
            NodeSpec::new("A", "node").with_name("Node A")
        ).unwrap();
        let n2 = backend.insert_node(
            NodeSpec::new("B", "node").with_name("Node B")
        ).unwrap();
        let n3 = backend.insert_node(
            NodeSpec::new("C", "node").with_name("Node C")
        ).unwrap();
        
        backend.insert_edge(EdgeSpec::new(n1, n2, "connects")).unwrap();
        backend.insert_edge(EdgeSpec::new(n2, n3, "connects")).unwrap();
        
        backend
    }
    
    #[test]
    fn test_your_algorithm_basic() {
        let backend = create_test_graph();
        let config = YourAlgorithmConfig::default();
        
        let result = your_algorithm(&backend, &config).unwrap();
        
        assert!(result.converged);
        assert!(result.iterations > 0);
        assert_eq!(result.scores.len(), 3);
        
        // Verify scores sum to approximately 1.0
        let sum: f64 = result.scores.values().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }
    
    #[test]
    fn test_your_algorithm_empty_graph() {
        let backend = SqliteGraphBackend::create_in_memory().unwrap();
        let config = YourAlgorithmConfig::default();
        
        let result = your_algorithm(&backend, &config);
        assert!(matches!(result, Err(AlgorithmError::EmptyGraph)));
    }
    
    #[test]
    fn test_your_algorithm_v3_backend() {
        let temp = tempfile::tempdir().unwrap();
        let backend = V3Backend::create(temp.path().join("test.graph")).unwrap();
        
        // Add nodes and edges
        let n1 = backend.insert_node(
            NodeSpec::new("A", "node").with_name("Node A")
        ).unwrap();
        let n2 = backend.insert_node(
            NodeSpec::new("B", "node").with_name("Node B")
        ).unwrap();
        backend.insert_edge(EdgeSpec::new(n1, n2, "connects")).unwrap();
        
        let config = YourAlgorithmConfig::default();
        let result = your_algorithm(&backend, &config).unwrap();
        
        assert!(result.converged);
        assert_eq!(result.scores.len(), 2);
    }
    
    #[test]
    fn test_your_algorithm_convergence_config() {
        let backend = create_test_graph();
        
        // Test with very strict tolerance - might not converge
        let strict_config = YourAlgorithmConfig {
            max_iterations: 5,
            tolerance: 1e-12,
        };
        
        let result = your_algorithm(&backend, &strict_config);
        assert!(matches!(result, Err(AlgorithmError::ConvergenceFailed(5))));
    }
}
```

---

## Algorithm Patterns

### Pattern 1: Traversal-Based

For BFS, DFS, topological sort:

```rust
pub fn bfs_traversal<B: GraphBackend>(
    backend: &B,
    start: NodeId,
    max_depth: Option<usize>,
) -> Result<Vec<(NodeId, usize)>, AlgorithmError> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut result = Vec::new();
    
    queue.push_back((start, 0));
    visited.insert(start);
    
    while let Some((node, depth)) = queue.pop_front() {
        if let Some(max) = max_depth {
            if depth > max {
                continue;
            }
        }
        
        result.push((node, depth));
        
        for neighbor in backend.get_neighbors(node, None)? {
            if visited.insert(neighbor) {
                queue.push_back((neighbor, depth + 1));
            }
        }
    }
    
    Ok(result)
}
```

### Pattern 2: Iterative Convergence

For PageRank, centrality measures:

```rust
pub fn iterative_algorithm<B: GraphBackend>(
    backend: &B,
    config: &Config,
) -> Result<Scores, AlgorithmError> {
    let mut scores = initialize_scores(backend)?;
    
    for iteration in 0..config.max_iterations {
        let new_scores = compute_iteration(backend, &scores)?;
        
        if has_converged(&scores, &new_scores, config.tolerance) {
            return Ok(Scores { values: new_scores, iterations: iteration + 1 });
        }
        
        scores = new_scores;
    }
    
    Err(AlgorithmError::ConvergenceFailed(config.max_iterations))
}
```

### Pattern 3: Path Finding

For Dijkstra, A*:

```rust
use std::collections::BinaryHeap;
use std::cmp::Ordering;

#[derive(Copy, Clone, Debug, PartialEq)]
struct State {
    cost: f64,
    node: NodeId,
}

impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.partial_cmp(&self.cost).unwrap()
    }
}

pub fn dijkstra<B: GraphBackend>(
    backend: &B,
    start: NodeId,
    goal: NodeId,
) -> Result<Option<(Vec<NodeId>, f64)>, AlgorithmError> {
    let mut dist: HashMap<NodeId, f64> = HashMap::new();
    let mut prev: HashMap<NodeId, NodeId> = HashMap::new();
    let mut heap = BinaryHeap::new();
    
    dist.insert(start, 0.0);
    heap.push(State { cost: 0.0, node: start });
    
    while let Some(State { cost, node }) = heap.pop() {
        if node == goal {
            // Reconstruct path
            let mut path = vec![goal];
            let mut current = goal;
            while let Some(&prev_node) = prev.get(&current) {
                path.push(prev_node);
                current = prev_node;
            }
            path.reverse();
            return Ok(Some((path, cost)));
        }
        
        if cost > *dist.get(&node).unwrap_or(&f64::INFINITY) {
            continue;
        }
        
        for edge in backend.get_edges_from(node)? {
            let next = edge.target;
            let weight = edge.weight.unwrap_or(1.0);
            let next_cost = cost + weight;
            
            if next_cost < *dist.get(&next).unwrap_or(&f64::INFINITY) {
                heap.push(State { cost: next_cost, node: next });
                dist.insert(next, next_cost);
                prev.insert(next, node);
            }
        }
    }
    
    Ok(None) // No path found
}
```

---

## Testing Algorithms

```bash
# Run all algorithm tests
cargo test --lib algorithms

# Run tests for a specific algorithm
cargo test --lib algorithms::your_algorithm::tests

# Run with V3 backend
cargo test --lib --features native-v3 algorithms

# Run with SQLite backend
cargo test --lib algorithms
```

---

## Performance Considerations

1. **Cache neighbor lookups** - Multiple calls to `get_neighbors` are expensive
2. **Batch operations** - Use `find_nodes` with filters instead of iterating all nodes
3. **Early termination** - Return as soon as goal is reached (path finding)
4. **Parallelization** - Consider `rayon` for independent node processing
5. **Memory usage** - Don't store full adjacency matrix for large graphs

---

## See Also

- [Architecture](../../ARCHITECTURE.md) - Algorithm system design
- Source: `src/algorithms/`
- [35+ Algorithms Available](../../README.md) - Existing implementations

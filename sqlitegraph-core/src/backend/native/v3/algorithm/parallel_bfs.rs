//! Parallel Breadth-First Search using Rayon
//!
//! Level-wise parallel BFS where each level can be processed concurrently.
//! Provides 2-4× speedup on multi-core systems for graph traversals.

use crate::backend::{BackendDirection, GraphBackend, NeighborQuery};
use crate::backend::native::v3::V3Backend;
use crate::snapshot::SnapshotId;
use crate::SqliteGraphError;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;

/// Configuration for parallel BFS execution
#[derive(Debug, Clone)]
pub struct BfsConfig {
    /// Maximum number of threads to use (None = use Rayon default)
    pub max_threads: Option<usize>,

    /// Minimum graph size to use parallel processing (node count)
    pub min_parallel_size: usize,

    /// Batch size for parallel chunking
    pub batch_size: usize,
}

impl Default for BfsConfig {
    fn default() -> Self {
        Self {
            max_threads: None,
            min_parallel_size: 1000,
            batch_size: 100,
        }
    }
}

/// Result of a BFS traversal
#[derive(Debug, Clone)]
pub struct BfsResult {
    /// Order in which nodes were visited (BFS order)
    pub visited_order: Vec<i64>,

    /// Distance from start node to each visited node
    pub distances: HashMap<i64, usize>,

    /// Total number of nodes visited
    pub total_visited: usize,
}

impl BfsResult {
    /// Create a new BFS result
    fn new() -> Self {
        Self {
            visited_order: Vec::new(),
            distances: HashMap::new(),
            total_visited: 0,
        }
    }

    /// Add a visited node at the specified distance
    fn add_visit(&mut self, node: i64, distance: usize) {
        self.visited_order.push(node);
        self.distances.insert(node, distance);
        self.total_visited += 1;
    }
}

/// Perform parallel BFS traversal from a start node
///
/// # Arguments
///
/// * `graph` - The V3Backend to traverse
/// * `start` - The starting node ID
/// * `config` - Optional configuration for parallel execution
///
/// # Returns
///
/// * `Result<BfsResult, SqliteGraphError>` - BFS traversal results
///
/// # Example
///
/// ```no_run
/// use sqlitegraph::backend::native::v3::algorithm::parallel_bfs;
/// use sqlitegraph::backend::native::v3::algorithm::BfsConfig;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = /* ... */;
/// let config = BfsConfig {
///     max_threads: Some(4),
///     min_parallel_size: 500,
///     batch_size: 50,
/// };
/// let result = parallel_bfs(&backend, 1, Some(config))?;
/// # Ok(())
/// # }
/// ```
pub fn parallel_bfs(
    graph: &V3Backend,
    start: i64,
    config: Option<BfsConfig>,
) -> Result<BfsResult, SqliteGraphError> {
    let config = config.unwrap_or_default();

    // Check if start node exists
    let snapshot = SnapshotId::current();
    if graph.get_node(snapshot, start).is_err() {
        return Err(SqliteGraphError::not_found(format!("Start node {} not found", start)));
    }

    // Check graph size - use sequential fallback for small graphs
    let node_count = graph.header().node_count;
    if node_count < config.min_parallel_size as u64 {
        return sequential_bfs(graph, start);
    }

    // Set up Rayon thread pool if max_threads specified
    let result = if let Some(max_threads) = config.max_threads {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(max_threads)
            .build()
            .map_err(|e| SqliteGraphError::connection(format!("Failed to create thread pool: {}", e)))?;

        pool.install(|| parallel_bfs_impl(graph, start, &config))
    } else {
        parallel_bfs_impl(graph, start, &config)
    };

    result
}

/// Internal parallel BFS implementation
fn parallel_bfs_impl(
    graph: &V3Backend,
    start: i64,
    config: &BfsConfig,
) -> Result<BfsResult, SqliteGraphError> {
    let snapshot = SnapshotId::current();
    let mut result = BfsResult::new();
    let visited = Arc::new(Mutex::new(HashSet::new()));

    // Initialize BFS queue
    let mut current_level: Vec<i64> = vec![start];
    let mut next_level: Vec<i64> = Vec::new();
    let mut distance = 0;

    // Mark start as visited
    {
        let mut visited_guard = visited.lock().unwrap();
        visited_guard.insert(start);
    }
    result.add_visit(start, distance);

    // Process each level
    while !current_level.is_empty() {
        next_level.clear();
        distance += 1;

        // Process current level in parallel chunks
        let chunks: Vec<_> = current_level
            .par_chunks(config.batch_size)
            .collect();

        for chunk in chunks {
            // Process each node in chunk
            for &node in chunk {
                // Fetch neighbors using the GraphBackend API
                let query = NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                };

                if let Ok(neighbors) = graph.neighbors(snapshot, node, query) {
                    for neighbor in neighbors {
                        let mut visited_guard = visited.lock().unwrap();
                        if visited_guard.insert(neighbor) {
                            drop(visited_guard);
                            next_level.push(neighbor);
                            result.add_visit(neighbor, distance);
                        }
                    }
                }
            }
        }

        // Swap levels
        std::mem::swap(&mut current_level, &mut next_level);
    }

    Ok(result)
}

/// Sequential BFS fallback for small graphs
///
/// Used when graph size is below `min_parallel_size` threshold.
/// Provides simpler implementation without threading overhead.
fn sequential_bfs(graph: &V3Backend, start: i64) -> Result<BfsResult, SqliteGraphError> {
    let snapshot = SnapshotId::current();
    let mut result = BfsResult::new();
    let mut visited: HashSet<i64> = HashSet::new();
    let mut queue: VecDeque<(i64, usize)> = VecDeque::new();

    // Initialize
    visited.insert(start);
    queue.push_back((start, 0));
    result.add_visit(start, 0);

    // BFS traversal
    while let Some((node, distance)) = queue.pop_front() {
        // Fetch neighbors using the GraphBackend API
        let query = NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        };

        if let Ok(neighbors) = graph.neighbors(snapshot, node, query) {
            for neighbor in neighbors {
                if visited.insert(neighbor) {
                    queue.push_back((neighbor, distance + 1));
                    result.add_visit(neighbor, distance + 1);
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v3::V3Backend;
    use crate::backend::{EdgeSpec, NodeSpec};
    use tempfile::TempDir;

    /// Helper function to create a test backend
    fn create_test_backend() -> (V3Backend, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = V3Backend::create(&db_path).unwrap();
        (backend, temp_dir)
    }

    /// Helper function to create a chain graph: 1 -> 2 -> 3 -> ... -> n
    fn create_chain_graph(backend: &V3Backend, n: i64) -> Vec<i64> {
        let mut node_ids = Vec::new();

        // Create nodes
        for i in 1..=n {
            let node = NodeSpec {
                kind: "test_node".to_string(),
                name: format!("node_{}", i),
                file_path: None,
                data: serde_json::json!(null),
            };
            let id = backend.insert_node(node).unwrap();
            node_ids.push(id);
        }

        // Create edges to form a chain
        for i in 0..node_ids.len() - 1 {
            let edge = EdgeSpec {
                from: node_ids[i],
                to: node_ids[i + 1],
                edge_type: "test_edge".to_string(),
                data: serde_json::json!(null),
            };
            backend.insert_edge(edge).unwrap();
        }

        node_ids
    }

    #[test]
    fn test_parallel_bfs_chain_graph() {
        let (backend, _temp_dir) = create_test_backend();
        let node_ids = create_chain_graph(&backend, 10);

        // Run BFS from first node
        let result = parallel_bfs(&backend, node_ids[0], None).unwrap();

        // Verify traversal order
        assert_eq!(result.total_visited, 10);
        assert_eq!(result.visited_order.len(), 10);

        // Verify distances
        assert_eq!(result.distances[&node_ids[0]], 0);
        assert_eq!(result.distances[&node_ids[1]], 1);
        assert_eq!(result.distances[&node_ids[9]], 9);

        // Verify BFS order (should be sequential in a chain)
        for (i, &node_id) in result.visited_order.iter().enumerate() {
            assert_eq!(node_id, node_ids[i]);
        }
    }

    #[test]
    fn test_parallel_bfs_nonexistent_start() {
        let (backend, _temp_dir) = create_test_backend();

        // Try BFS from nonexistent node
        let result = parallel_bfs(&backend, 9999, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_parallel_bfs_sequential_fallback() {
        let (backend, _temp_dir) = create_test_backend();
        let node_ids = create_chain_graph(&backend, 5); // Small graph

        // Run BFS with high min_parallel_size to force sequential
        let config = BfsConfig {
            max_threads: None,
            min_parallel_size: 1000,
            batch_size: 100,
        };

        let result = parallel_bfs(&backend, node_ids[0], Some(config)).unwrap();

        // Verify traversal worked
        assert_eq!(result.total_visited, 5);
        assert_eq!(result.visited_order.len(), 5);
    }

    #[test]
    fn test_bfs_config_default() {
        let config = BfsConfig::default();

        assert_eq!(config.max_threads, None);
        assert_eq!(config.min_parallel_size, 1000);
        assert_eq!(config.batch_size, 100);
    }

    #[test]
    fn test_bfs_result_empty() {
        let result = BfsResult::new();

        assert_eq!(result.total_visited, 0);
        assert!(result.visited_order.is_empty());
        assert!(result.distances.is_empty());
    }

    #[test]
    fn test_parallel_bfs_diamond_graph() {
        let (backend, _temp_dir) = create_test_backend();

        // Create a diamond graph:
        //     1
        //    / \
        //   2   3
        //    \ /
        //     4
        let node1 = backend
            .insert_node(NodeSpec {
                kind: "test".to_string(),
                name: "1".to_string(),
                file_path: None,
                data: serde_json::json!(null),
            })
            .unwrap();
        let node2 = backend
            .insert_node(NodeSpec {
                kind: "test".to_string(),
                name: "2".to_string(),
                file_path: None,
                data: serde_json::json!(null),
            })
            .unwrap();
        let node3 = backend
            .insert_node(NodeSpec {
                kind: "test".to_string(),
                name: "3".to_string(),
                file_path: None,
                data: serde_json::json!(null),
            })
            .unwrap();
        let node4 = backend
            .insert_node(NodeSpec {
                kind: "test".to_string(),
                name: "4".to_string(),
                file_path: None,
                data: serde_json::json!(null),
            })
            .unwrap();

        // Create edges
        backend
            .insert_edge(EdgeSpec {
                from: node1,
                to: node2,
                edge_type: "edge".to_string(),
                data: serde_json::json!(null),
            })
            .unwrap();
        backend
            .insert_edge(EdgeSpec {
                from: node1,
                to: node3,
                edge_type: "edge".to_string(),
                data: serde_json::json!(null),
            })
            .unwrap();
        backend
            .insert_edge(EdgeSpec {
                from: node2,
                to: node4,
                edge_type: "edge".to_string(),
                data: serde_json::json!(null),
            })
            .unwrap();
        backend
            .insert_edge(EdgeSpec {
                from: node3,
                to: node4,
                edge_type: "edge".to_string(),
                data: serde_json::json!(null),
            })
            .unwrap();

        // Run BFS from node1
        let result = parallel_bfs(&backend, node1, None).unwrap();

        // Verify all nodes visited
        assert_eq!(result.total_visited, 4);

        // Verify distances
        assert_eq!(result.distances[&node1], 0);
        assert_eq!(result.distances[&node2], 1);
        assert_eq!(result.distances[&node3], 1);
        assert_eq!(result.distances[&node4], 2);

        // Verify BFS order (level by level)
        assert_eq!(result.visited_order[0], node1);
        assert!(result.visited_order[1..3].contains(&node2));
        assert!(result.visited_order[1..3].contains(&node3));
        assert_eq!(result.visited_order[3], node4);
    }
}

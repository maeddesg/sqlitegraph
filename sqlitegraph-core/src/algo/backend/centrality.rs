//! Centrality algorithms using GraphBackend trait
//!
//! Backend-agnostic implementations of PageRank and Betweenness centrality.

use std::collections::VecDeque;

use ahash::AHashMap;

use crate::backend::GraphBackend;
use crate::errors::SqliteGraphError;

/// Computes PageRank scores for all nodes in the graph.
///
/// Backend-agnostic version that works with any GraphBackend implementation.
/// Uses only `entity_ids()` and `fetch_outgoing()` trait methods.
///
/// # Arguments
/// * `graph` - Any backend implementing GraphBackend trait
/// * `damping` - Damping factor (typically 0.85)
/// * `iterations` - Number of power iteration iterations
///
/// # Returns
/// Vector of (node_id, score) tuples sorted by score descending.
///
/// # Complexity
/// Time: O(k * |E|) where k = iterations
/// Space: O(|V|) for score storage
pub fn pagerank(
    graph: &dyn GraphBackend,
    damping: f64,
    iterations: usize,
) -> Result<Vec<(i64, f64)>, SqliteGraphError> {
    let all_ids = graph.all_entity_ids()?;
    let n = all_ids.len();

    if n == 0 {
        return Ok(Vec::new());
    }

    // Initialize all nodes with equal score
    let mut scores: AHashMap<i64, f64> = all_ids.iter().map(|&id| (id, 1.0 / n as f64)).collect();

    // Pre-compute outgoing counts for all nodes
    let mut outgoing_counts: AHashMap<i64, usize> = AHashMap::new();
    for &id in &all_ids {
        let count = graph.fetch_outgoing(id)?.len();
        outgoing_counts.insert(id, count);
    }

    // Power iteration
    for _ in 0..iterations {
        let mut new_scores: AHashMap<i64, f64> = AHashMap::new();

        // Initialize with teleport probability (1-d)/n
        let base_score = (1.0 - damping) / n as f64;
        for &id in &all_ids {
            new_scores.insert(id, base_score);
        }

        // Track total dangling score to redistribute
        let mut dangling_score = 0.0;

        // Distribute scores from outgoing edges
        for &id in &all_ids {
            let score = scores[&id];
            let out_count = outgoing_counts[&id];

            if out_count == 0 {
                // Dangling node - add score to redistribution pool
                dangling_score += score;
            } else {
                // Distribute score evenly to all outgoing neighbors
                let share = score / out_count as f64;
                for &neighbor in &graph.fetch_outgoing(id)? {
                    *new_scores.get_mut(&neighbor).unwrap() += damping * share;
                }
            }
        }

        // Redistribute dangling score equally to all nodes
        let dangling_share = damping * dangling_score / n as f64;
        for (_, score) in new_scores.iter_mut() {
            *score += dangling_share;
        }

        scores = new_scores;
    }

    // Convert to sorted vector
    let mut result: Vec<(i64, f64)> = scores.into_iter().collect();
    result.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    Ok(result)
}

/// Computes betweenness centrality for all nodes in the graph.
///
/// Backend-agnostic version using Brandes' algorithm.
/// Uses only `all_entity_ids()` and `fetch_outgoing()` trait methods.
///
/// # Arguments
/// * `graph` - Any backend implementing GraphBackend trait
///
/// # Returns
/// Vector of (node_id, centrality) tuples sorted by centrality descending.
///
/// # Complexity
/// Time: O(|V| * |E|) for unweighted graphs (Brandes' algorithm)
/// Space: O(|V| + |E|)
pub fn betweenness_centrality(
    graph: &dyn GraphBackend,
) -> Result<Vec<(i64, f64)>, SqliteGraphError> {
    let all_ids = graph.all_entity_ids()?;
    let n = all_ids.len();

    if n == 0 {
        return Ok(Vec::new());
    }

    // Initialize centrality scores
    let mut centrality: AHashMap<i64, f64> = all_ids.iter().map(|&id| (id, 0.0)).collect();

    // Brandes' algorithm: for each node as source
    for &s in &all_ids {
        // BFS from s
        let mut dist: AHashMap<i64, i64> = AHashMap::new();
        let mut sigma: AHashMap<i64, f64> = AHashMap::new(); // number of shortest paths
        let mut predecessors: AHashMap<i64, Vec<i64>> = AHashMap::new();

        // Initialize source
        dist.insert(s, 0);
        sigma.insert(s, 1.0);

        let mut queue = VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            for &w in &graph.fetch_outgoing(v)? {
                // First time discovering w
                if !dist.contains_key(&w) {
                    dist.insert(w, dist[&v] + 1);
                    queue.push_back(w);
                }

                // Found another shortest path to w through v
                if dist.get(&w) == Some(&(dist[&v] + 1)) {
                    *sigma.entry(w).or_insert(0.0) += sigma[&v];
                    predecessors.entry(w).or_insert_with(Vec::new).push(v);
                }
            }
        }

        // Accumulate centrality (dependency propagation)
        let mut delta: AHashMap<i64, f64> = all_ids.iter().map(|&id| (id, 0.0)).collect();

        // Process nodes in reverse order of distance from s
        let mut nodes: Vec<i64> = dist.keys().copied().collect();
        nodes.sort_by_key(|&id| std::cmp::Reverse(dist[&id]));

        for w in nodes {
            if w == s {
                continue;
            }

            for &v in predecessors.get(&w).unwrap_or(&vec![]) {
                let contribution = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                *delta.get_mut(&v).unwrap() += contribution;
            }

            if w != s {
                *centrality.get_mut(&w).unwrap() += delta[&w];
            }
        }
    }

    // Convert to sorted vector
    let mut result: Vec<(i64, f64)> = centrality.into_iter().collect();
    result.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    Ok(result)
}

#[cfg(all(test, feature = "native-v3"))]
mod tests {
    use super::*;
    use crate::backend::native::v3::V3Backend;
    use crate::backend::{EdgeSpec, NodeSpec};
    use tempfile::TempDir;

    fn create_test_backend() -> (V3Backend, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        let backend = V3Backend::create(&db_path).unwrap();
        (backend, temp_dir)
    }

    fn build_chain(backend: &V3Backend) -> Vec<i64> {
        let mut nodes = Vec::new();
        for _ in 0..4 {
            let id = backend
                .insert_node(NodeSpec {
                    kind: "Node".to_string(),
                    name: "node".to_string(),
                    file_path: None,
                    data: serde_json::json!({}),
                })
                .unwrap();
            nodes.push(id);
        }
        for i in 0..nodes.len() - 1 {
            backend
                .insert_edge(EdgeSpec {
                    from: nodes[i],
                    to: nodes[i + 1],
                    edge_type: "links".to_string(),
                    data: serde_json::json!({}),
                })
                .unwrap();
        }
        nodes
    }

    #[test]
    fn test_backend_pagerank_chain() {
        let (backend, _temp) = create_test_backend();
        let nodes = build_chain(&backend);

        let scores = pagerank(&backend, 0.85, 20).unwrap();

        assert_eq!(scores.len(), 4);

        // Verify scores sum to ~1.0
        let total: f64 = scores.iter().map(|(_, s)| s).sum();
        assert!((total - 1.0).abs() < 0.01);

        // End node should have higher score than start
        let scores_map: std::collections::HashMap<i64, f64> = scores.into_iter().collect();
        assert!(scores_map[&nodes[3]] > scores_map[&nodes[0]]);
    }

    #[test]
    fn test_backend_betweenness_chain() {
        let (backend, _temp) = create_test_backend();
        build_chain(&backend);

        let centrality = betweenness_centrality(&backend).unwrap();

        assert_eq!(centrality.len(), 4);

        // In a chain, middle nodes should have higher betweenness
        // (they lie on more shortest paths)
    }

    #[test]
    fn test_backend_pagerank_empty() {
        let (backend, _temp) = create_test_backend();

        let scores = pagerank(&backend, 0.85, 20).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_backend_betweenness_empty() {
        let (backend, _temp) = create_test_backend();

        let centrality = betweenness_centrality(&backend).unwrap();
        assert!(centrality.is_empty());
    }
}

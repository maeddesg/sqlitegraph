//! V2-specific dataset generator for stress testing and performance benchmarks.
//!
//! Provides deterministic graph generation with specific patterns for V2 testing:
//! - Multi-edge scenarios (multiple edges between same node pairs)
//! - Bidirectional-heavy patterns
//! - Power-law topology with hubs
//! - Erdos-Renyi sparse graphs

use rand::seq::SliceRandom;
use rand::{RngCore, SeedableRng};
use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};
use std::collections::HashMap;
use tempfile::TempDir;

/// V2 graph generation modes for stress testing
#[derive(Debug, Clone, Copy)]
pub enum V2GraphMode {
    /// Sparse graph ~4*N edges (Erdos-Renyi-like)
    Sparse,
    /// Power-law topology: few hubs + many low-degree nodes
    PowerLaw,
    /// High multi-edge: many repeated edges between same pairs
    MultiEdge,
    /// Bidirectional-heavy: edges in both directions
    Bidirectional,
    /// Mixed realistic scenario combining multiple patterns
    Mixed,
}

/// Specification for V2 test graph generation
#[derive(Debug, Clone)]
pub struct V2GraphSpec {
    pub node_count: usize,
    pub edge_count: usize,
    pub mode: V2GraphMode,
    pub seed: u64,
    pub multi_edge_factor: usize, // Average edges per unique pair
    pub bidirectional_ratio: f64, // Fraction of edges that should have reverse
}

impl Default for V2GraphSpec {
    fn default() -> Self {
        Self {
            node_count: 10000,
            edge_count: 40000,
            mode: V2GraphMode::Mixed,
            seed: 0xC0FFEE, // Fixed seed for reproducibility
            multi_edge_factor: 3,
            bidirectional_ratio: 0.3,
        }
    }
}

impl V2GraphSpec {
    /// Create a new V2 graph specification
    pub fn new(node_count: usize, edge_count: usize, mode: V2GraphMode) -> Self {
        Self {
            node_count,
            edge_count,
            mode,
            ..Default::default()
        }
    }

    /// Set multi-edge factor for MultiEdge mode
    pub fn with_multi_edge_factor(mut self, factor: usize) -> Self {
        self.multi_edge_factor = factor;
        self
    }

    /// Set bidirectional ratio
    pub fn with_bidirectional_ratio(mut self, ratio: f64) -> Self {
        self.bidirectional_ratio = ratio.clamp(0.0, 1.0);
        self
    }

    /// Create stress test specification
    pub fn stress_test() -> Self {
        Self {
            node_count: 100000,
            edge_count: 1000000,
            mode: V2GraphMode::Mixed,
            seed: 0xDEADBEEF,
            multi_edge_factor: 5,
            bidirectional_ratio: 0.4,
        }
    }
}

/// Result of V2 graph generation
#[derive(Debug)]
pub struct V2GraphResult {
    pub node_ids: Vec<i64>,
    pub edge_count: usize,
    pub temp_dir: TempDir,
    pub db_path: std::path::PathBuf,
    pub file_size_bytes: u64,
    pub generation_time_ms: u64,
    pub node_degrees: HashMap<i64, (usize, usize)>, // (outgoing, incoming)
    pub bytes_per_node: f64,
    pub bytes_per_edge: f64,
    pub growth_efficiency: f64, // bytes_per_entity ratio
}

/// Generate a V2 test graph with deterministic patterns
pub fn generate_v2_graph(spec: &V2GraphSpec) -> V2GraphResult {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("v2_test.v2");

    // Use native backend for V2 testing
    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).expect("Failed to create V2 test graph");

    let start_time = std::time::Instant::now();
    let mut rng = rand::rngs::StdRng::seed_from_u64(spec.seed);

    // Generate nodes
    let mut node_ids = Vec::with_capacity(spec.node_count);
    for i in 0..spec.node_count {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "TestNode".to_string(),
                name: format!("node_{}", i),
                file_path: None,
                data: serde_json::json!({
                    "id": i,
                    "mode": format!("{:?}", spec.mode),
                    "seed": spec.seed,
                }),
            })
            .expect("Failed to insert node");
        node_ids.push(node_id);
    }

    // Generate edges based on mode
    let edge_count = generate_v2_edges(&graph, &node_ids, spec, &mut rng);

    let generation_time = start_time.elapsed();

    // Get file size
    let file_size_bytes = std::fs::metadata(&db_path)
        .expect("Failed to get file size")
        .len();

    // Calculate node degrees (sample for large graphs)
    let node_degrees = if spec.node_count <= 10000 {
        calculate_all_degrees(&graph, &node_ids)
    } else {
        calculate_sample_degrees(&graph, &node_ids, 1000, &mut rng)
    };

    let bytes_per_node = file_size_bytes as f64 / spec.node_count as f64;
    let bytes_per_edge = file_size_bytes as f64 / edge_count as f64;
    let total_entities = (spec.node_count + edge_count) as f64;
    let growth_efficiency = file_size_bytes as f64 / total_entities;

    V2GraphResult {
        node_ids,
        edge_count,
        temp_dir,
        db_path,
        file_size_bytes,
        generation_time_ms: generation_time.as_millis() as u64,
        node_degrees,
        bytes_per_node,
        bytes_per_edge,
        growth_efficiency,
    }
}

/// Generate edges based on V2-specific patterns
fn generate_v2_edges(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    spec: &V2GraphSpec,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    if node_ids.is_empty() {
        return 0;
    }

    match spec.mode {
        V2GraphMode::Sparse => generate_sparse_edges(graph, node_ids, spec, rng),
        V2GraphMode::PowerLaw => generate_powerlaw_edges(graph, node_ids, spec, rng),
        V2GraphMode::MultiEdge => generate_multiedge_edges(graph, node_ids, spec, rng),
        V2GraphMode::Bidirectional => generate_bidirectional_edges(graph, node_ids, spec, rng),
        V2GraphMode::Mixed => generate_mixed_edges(graph, node_ids, spec, rng),
    }
}

/// Generate sparse Erdos-Renyi-like graph (~4*N edges)
fn generate_sparse_edges(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    spec: &V2GraphSpec,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;
    let target_edges = spec.edge_count.min(node_ids.len() * 4); // ~4*N max

    for _ in 0..target_edges {
        let from_idx = rng.next_u32() as usize % node_ids.len();
        let mut to_idx = rng.next_u32() as usize % node_ids.len();

        // Avoid self-loops
        if to_idx == from_idx {
            to_idx = (to_idx + 1) % node_ids.len();
        }

        let edge_data = serde_json::json!({
            "type": "sparse",
            "created_at": "v2_stress_test",
        });

        graph
            .insert_edge(EdgeSpec {
                from: node_ids[from_idx],
                to: node_ids[to_idx],
                edge_type: "sparse_edge".to_string(),
                data: edge_data,
            })
            .expect("Failed to insert sparse edge");
        edge_count += 1;
    }

    edge_count
}

/// Generate power-law graph with hub nodes
fn generate_powerlaw_edges(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    spec: &V2GraphSpec,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;

    // Create hub nodes (top 5% of nodes get 60% of edges)
    let hub_count = (node_ids.len() / 20).max(1);
    let hub_edges = (spec.edge_count as f64 * 0.6) as usize;
    let regular_edges = spec.edge_count - hub_edges;

    // Generate hub edges (concentrated on few nodes)
    for _ in 0..hub_edges {
        let hub_idx = rng.next_u32() as usize % hub_count;
        let target_idx = rng.next_u32() as usize % node_ids.len();

        if hub_idx != target_idx {
            let edge_data = serde_json::json!({
                "type": "hub_edge",
                "hub": true,
            });

            graph
                .insert_edge(EdgeSpec {
                    from: node_ids[hub_idx],
                    to: node_ids[target_idx],
                    edge_type: "hub_connection".to_string(),
                    data: edge_data,
                })
                .expect("Failed to insert hub edge");
            edge_count += 1;
        }
    }

    // Generate regular sparse edges for remaining nodes
    for _ in 0..regular_edges {
        let from_idx = hub_count + (rng.next_u32() as usize % (node_ids.len() - hub_count));
        let to_idx = rng.next_u32() as usize % node_ids.len();

        if from_idx != to_idx {
            let edge_data = serde_json::json!({
                "type": "regular_edge",
                "hub": false,
            });

            graph
                .insert_edge(EdgeSpec {
                    from: node_ids[from_idx],
                    to: node_ids[to_idx],
                    edge_type: "regular_connection".to_string(),
                    data: edge_data,
                })
                .expect("Failed to insert regular edge");
            edge_count += 1;
        }
    }

    edge_count
}

/// Generate high multi-edge graph (multiple edges between same pairs)
fn generate_multiedge_edges(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    spec: &V2GraphSpec,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;
    let unique_pairs = spec.edge_count / spec.multi_edge_factor.max(1);

    for i in 0..unique_pairs {
        let from_idx = i % node_ids.len();
        let to_idx = (i + 1 + rng.next_u32() as usize % (node_ids.len() - 1)) % node_ids.len();

        // Insert multiple edges between the same pair
        for multi_idx in 0..spec.multi_edge_factor {
            let edge_data = serde_json::json!({
                "type": "multiedge",
                "pair_index": i,
                "multi_index": multi_idx,
                "total_multi": spec.multi_edge_factor,
            });

            graph
                .insert_edge(EdgeSpec {
                    from: node_ids[from_idx],
                    to: node_ids[to_idx],
                    edge_type: format!("multi_edge_{}", multi_idx),
                    data: edge_data,
                })
                .expect("Failed to insert multi-edge");
            edge_count += 1;
        }
    }

    edge_count
}

/// Generate bidirectional-heavy graph
fn generate_bidirectional_edges(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    spec: &V2GraphSpec,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;
    let base_pairs = spec.edge_count / 2; // Each pair creates 2 edges

    for i in 0..base_pairs {
        let from_idx = i % node_ids.len();
        let to_idx = (i + 1) % node_ids.len();

        let edge_data = serde_json::json!({
            "type": "bidirectional",
            "pair_index": i,
            "direction": "forward",
        });

        // Forward edge
        graph
            .insert_edge(EdgeSpec {
                from: node_ids[from_idx],
                to: node_ids[to_idx],
                edge_type: "bidirectional_forward".to_string(),
                data: edge_data.clone(),
            })
            .expect("Failed to insert forward edge");
        edge_count += 1;

        // Reverse edge
        let reverse_data = serde_json::json!({
            "type": "bidirectional",
            "pair_index": i,
            "direction": "reverse",
        });

        graph
            .insert_edge(EdgeSpec {
                from: node_ids[to_idx],
                to: node_ids[from_idx],
                edge_type: "bidirectional_reverse".to_string(),
                data: reverse_data,
            })
            .expect("Failed to insert reverse edge");
        edge_count += 1;
    }

    edge_count
}

/// Generate mixed realistic scenario
fn generate_mixed_edges(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    spec: &V2GraphSpec,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;

    // Allocate edges across patterns:
    // 40% sparse (base connectivity)
    // 30% power-law (hubs)
    // 20% bidirectional (mutual relationships)
    // 10% multi-edge (parallel connections)

    let sparse_edges = (spec.edge_count as f64 * 0.4) as usize;
    let powerlaw_edges = (spec.edge_count as f64 * 0.3) as usize;
    let bidirectional_edges = (spec.edge_count as f64 * 0.2) as usize / 2; // Each creates 2 edges
    let multiedge_edges = (spec.edge_count as f64 * 0.1) as usize;

    // Generate each pattern
    edge_count += generate_sparse_pattern(graph, node_ids, sparse_edges, rng);
    edge_count += generate_powerlaw_pattern(graph, node_ids, powerlaw_edges, rng);
    edge_count += generate_bidirectional_pattern(graph, node_ids, bidirectional_edges, rng);
    edge_count += generate_multiedge_pattern(graph, node_ids, multiedge_edges, 3, rng);

    edge_count
}

/// Helper: generate sparse pattern portion
fn generate_sparse_pattern(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    count: usize,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;
    for _ in 0..count {
        let from_idx = rng.next_u32() as usize % node_ids.len();
        let to_idx = rng.next_u32() as usize % node_ids.len();

        if from_idx != to_idx {
            graph
                .insert_edge(EdgeSpec {
                    from: node_ids[from_idx],
                    to: node_ids[to_idx],
                    edge_type: "mixed_sparse".to_string(),
                    data: serde_json::json!({"pattern": "sparse"}),
                })
                .expect("Failed to insert mixed sparse edge");
            edge_count += 1;
        }
    }
    edge_count
}

/// Helper: generate power-law pattern portion
fn generate_powerlaw_pattern(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    count: usize,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;
    let hub_count = (node_ids.len() / 10).max(1);

    for _ in 0..count {
        let hub_idx = rng.next_u32() as usize % hub_count;
        let target_idx = rng.next_u32() as usize % node_ids.len();

        if hub_idx != target_idx {
            graph
                .insert_edge(EdgeSpec {
                    from: node_ids[hub_idx],
                    to: node_ids[target_idx],
                    edge_type: "mixed_powerlaw".to_string(),
                    data: serde_json::json!({"pattern": "powerlaw"}),
                })
                .expect("Failed to insert mixed power-law edge");
            edge_count += 1;
        }
    }
    edge_count
}

/// Helper: generate bidirectional pattern portion
fn generate_bidirectional_pattern(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    pair_count: usize,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;

    for i in 0..pair_count {
        let from_idx = i % node_ids.len();
        let to_idx = (i + 1) % node_ids.len();

        // Forward edge
        graph
            .insert_edge(EdgeSpec {
                from: node_ids[from_idx],
                to: node_ids[to_idx],
                edge_type: "mixed_bidirectional_forward".to_string(),
                data: serde_json::json!({"pattern": "bidirectional", "direction": "forward"}),
            })
            .expect("Failed to insert mixed forward edge");
        edge_count += 1;

        // Reverse edge
        graph
            .insert_edge(EdgeSpec {
                from: node_ids[to_idx],
                to: node_ids[from_idx],
                edge_type: "mixed_bidirectional_reverse".to_string(),
                data: serde_json::json!({"pattern": "bidirectional", "direction": "reverse"}),
            })
            .expect("Failed to insert mixed reverse edge");
        edge_count += 1;
    }
    edge_count
}

/// Helper: generate multi-edge pattern portion
fn generate_multiedge_pattern(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    pair_count: usize,
    multi_factor: usize,
    rng: &mut rand::rngs::StdRng,
) -> usize {
    let mut edge_count = 0;

    for i in 0..pair_count {
        let from_idx = i % node_ids.len();
        let to_idx = (i + 1) % node_ids.len();

        for multi_idx in 0..multi_factor {
            graph
                .insert_edge(EdgeSpec {
                    from: node_ids[from_idx],
                    to: node_ids[to_idx],
                    edge_type: format!("mixed_multiedge_{}", multi_idx),
                    data: serde_json::json!({
                        "pattern": "multiedge",
                        "multi_index": multi_idx,
                    }),
                })
                .expect("Failed to insert mixed multi-edge");
            edge_count += 1;
        }
    }
    edge_count
}

/// Calculate all node degrees (for smaller graphs)
fn calculate_all_degrees(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
) -> HashMap<i64, (usize, usize)> {
    let mut degrees = HashMap::new();

    for &node_id in node_ids {
        // Get outgoing neighbors count
        let outgoing = graph
            .neighbors(
                node_id,
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .unwrap_or_default()
            .len();

        // Get incoming neighbors count
        let incoming = graph
            .neighbors(
                node_id,
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Incoming,
                    edge_type: None,
                },
            )
            .unwrap_or_default()
            .len();

        degrees.insert(node_id, (outgoing, incoming));
    }

    degrees
}

/// Calculate sample node degrees (for larger graphs)
fn calculate_sample_degrees(
    graph: &Box<dyn sqlitegraph::GraphBackend>,
    node_ids: &[i64],
    sample_size: usize,
    rng: &mut rand::rngs::StdRng,
) -> HashMap<i64, (usize, usize)> {
    let mut sample_indices: Vec<usize> = (0..node_ids.len()).collect();
    sample_indices.partial_shuffle(rng, sample_size);

    let mut degrees = HashMap::new();

    for &idx in &sample_indices[..sample_size.min(sample_indices.len())] {
        let node_id = node_ids[idx];

        // Get outgoing neighbors count
        let outgoing = graph
            .neighbors(
                node_id,
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Outgoing,
                    edge_type: None,
                },
            )
            .unwrap_or_default()
            .len();

        // Get incoming neighbors count
        let incoming = graph
            .neighbors(
                node_id,
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Incoming,
                    edge_type: None,
                },
            )
            .unwrap_or_default()
            .len();

        degrees.insert(node_id, (outgoing, incoming));
    }

    degrees
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v2_graph_spec_creation() {
        let spec = V2GraphSpec::new(1000, 4000, V2GraphMode::Sparse)
            .with_multi_edge_factor(5)
            .with_bidirectional_ratio(0.5);

        assert_eq!(spec.node_count, 1000);
        assert_eq!(spec.edge_count, 4000);
        assert_eq!(spec.multi_edge_factor, 5);
        assert_eq!(spec.bidirectional_ratio, 0.5);
    }

    #[test]
    fn test_v2_graph_generation() {
        let spec = V2GraphSpec {
            node_count: 100,
            edge_count: 400,
            mode: V2GraphMode::Sparse,
            seed: 0x12345678,
            ..Default::default()
        };

        let result = generate_v2_graph(&spec);

        assert_eq!(result.node_ids.len(), 100);
        assert_eq!(result.edge_count, 400);
        assert!(result.file_size_bytes > 0);
        assert!(result.generation_time_ms > 0);
    }
}

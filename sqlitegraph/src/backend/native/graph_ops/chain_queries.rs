//! Chain traversal and pattern matching operations

use crate::backend::native::NativeBackendError;
use crate::backend::native::adjacency::{AdjacencyHelpers, Direction};
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::*;
use crate::backend::{BackendDirection, ChainStep, PatternMatch, PatternQuery};

// Per-traversal cache for eliminating redundant I/O during chain traversal
use super::{TraversalCache, get_neighbors_cached, TraversalCacheStats};

/// Native chain query implementation
pub fn native_chain_query(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    chain: &[ChainStep],
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    let mut current_nodes = vec![start];
    let mut result = current_nodes.clone();

    // Per-traversal cache - evaporates when function returns
    let mut cache: TraversalCache = TraversalCache::new();
    let mut stats = TraversalCacheStats::default();

    for step in chain {
        let mut next_nodes = Vec::new();
        let direction = match step.direction {
            BackendDirection::Outgoing => Direction::Outgoing,
            BackendDirection::Incoming => Direction::Incoming,
        };

        for &node in &current_nodes {
            let neighbors = if let Some(edge_type) = &step.edge_type {
                let edge_type_ref = edge_type.as_str();
                match direction {
                    Direction::Outgoing => AdjacencyHelpers::get_outgoing_neighbors_filtered(
                        graph_file,
                        node,
                        &[edge_type_ref],
                    )?,
                    Direction::Incoming => AdjacencyHelpers::get_incoming_neighbors_filtered(
                        graph_file,
                        node,
                        &[edge_type_ref],
                    )?,
                }
            } else {
                // Unfiltered path: use cache
                get_neighbors_cached(
                    graph_file,
                    node,
                    direction,
                    &mut cache,
                    &mut stats,
                )?
            };

            next_nodes.extend(neighbors);
        }

        if next_nodes.is_empty() {
            return Ok(vec![]); // Chain broken
        }

        current_nodes = next_nodes;
        result.extend(current_nodes.clone());
    }

    #[cfg(debug_assertions)]
    {
        if stats.hits + stats.misses > 0 {
            let hit_rate = stats.hit_rate();
            log::debug!(
                "Chain query cache stats: hits={}, misses={}, hit_rate={:.2}%",
                stats.hits,
                stats.misses,
                hit_rate * 100.0
            );
        }
    }

    Ok(result)
}

/// Native pattern search implementation (basic version)
pub fn native_pattern_search(
    _graph_file: &mut GraphFile,
    _start: NativeNodeId,
    _pattern: &PatternQuery,
) -> Result<Vec<PatternMatch>, NativeBackendError> {
    // This is a simplified implementation
    // In a full implementation, this would use the pattern engine
    // For now, return empty matches as the pattern engine is complex
    Ok(vec![])
}

//! Chain traversal and pattern matching operations

use crate::backend::native::NativeBackendError;
use crate::backend::native::adjacency::{AdjacencyHelpers, Direction};
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::*;
use crate::backend::{BackendDirection, ChainStep, PatternMatch, PatternQuery};

// Per-traversal context for 3-tier lookup (Phase 31)
use super::{TraversalContext, get_neighbors_optimized};

/// Native chain query implementation
pub fn native_chain_query(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    chain: &[ChainStep],
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    let mut current_nodes = vec![start];
    let mut result = current_nodes.clone();

    // Per-traversal context with 3-tier lookup (Phase 31)
    let mut ctx = TraversalContext::new();

    for step in chain {
        let mut next_nodes = Vec::new();
        let direction = match step.direction {
            BackendDirection::Outgoing => Direction::Outgoing,
            BackendDirection::Incoming => Direction::Incoming,
        };

        for &node in &current_nodes {
            let neighbors = if let Some(edge_type) = &step.edge_type {
                // Edge-type filtered path: use AdjacencyHelpers directly
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
                // Unfiltered path: use 3-tier optimized lookup (Phase 31)

                // Get degree for pattern detection (before direction matching)
                let degree = match direction {
                    Direction::Outgoing => AdjacencyHelpers::outgoing_degree(graph_file, node)?,
                    Direction::Incoming => AdjacencyHelpers::incoming_degree(graph_file, node)?,
                };
                let _pattern = ctx.detector.observe(node, degree);

                // Trigger prefetch if linear confirmed
                if ctx.detector.is_linear_confirmed() && !ctx.buffer.contains(node) {
                    ctx.buffer.prefetch_clusters_from(graph_file, node)?;
                }

                get_neighbors_optimized(graph_file, node, direction, &mut ctx)?
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
        let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
        if total_lookups > 0 {
            log::debug!(
                "Chain query optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                ctx.buffer_hits,
                ctx.buffer_misses,
                ctx.stats.hits,
                ctx.stats.misses,
                ctx.combined_hit_rate() * 100.0
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

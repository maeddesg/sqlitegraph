//! K-hop neighbor exploration operations

use crate::backend::native::NativeBackendError;
use crate::backend::native::adjacency::{AdjacencyHelpers, Direction};
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::*;

// Per-traversal context for 3-tier lookup (Phase 31)
use super::{TraversalContext, get_neighbors_optimized};

/// Native k-hop implementation
pub fn native_k_hop(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
    direction: Direction,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut current_level = vec![start];
    visited.insert(start);
    let mut result = Vec::new();

    // Per-traversal context with 3-tier lookup (Phase 31)
    let mut ctx = TraversalContext::new();

    for _ in 0..depth {
        let mut next_level = Vec::new();

        for node in current_level {
            // Get degree for pattern detection (direction-aware)
            let degree = match direction {
                Direction::Outgoing => AdjacencyHelpers::outgoing_degree(graph_file, node)?,
                Direction::Incoming => AdjacencyHelpers::incoming_degree(graph_file, node)?,
            };
            let _pattern = ctx.detector.observe(node, degree);

            // Trigger prefetch if linear confirmed
            if ctx.detector.is_linear_confirmed() && !ctx.buffer.contains(node) {
                ctx.buffer.prefetch_clusters_from(graph_file, node)?;
            }

            let neighbors = get_neighbors_optimized(
                graph_file,
                node,
                direction,
                &mut ctx,
            )?;

            for neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    next_level.push(neighbor);
                    result.push(neighbor);
                }
            }
        }

        current_level = next_level;
        if current_level.is_empty() {
            break;
        }
    }

    #[cfg(debug_assertions)]
    {
        let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
        if total_lookups > 0 {
            log::debug!(
                "K-hop optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                ctx.buffer_hits, ctx.buffer_misses, ctx.stats.hits, ctx.stats.misses,
                ctx.combined_hit_rate() * 100.0
            );
        }
    }

    Ok(result)
}

/// Native k-hop implementation with edge type filtering
pub fn native_k_hop_filtered(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
    direction: Direction,
    allowed_edge_types: &[&str],
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut current_level = vec![start];
    visited.insert(start);
    let mut result = Vec::new();

    for _ in 0..depth {
        let mut next_level = Vec::new();

        for node in current_level {
            let neighbors = match direction {
                Direction::Outgoing => AdjacencyHelpers::get_outgoing_neighbors_filtered(
                    graph_file,
                    node,
                    allowed_edge_types,
                )?,
                Direction::Incoming => AdjacencyHelpers::get_incoming_neighbors_filtered(
                    graph_file,
                    node,
                    allowed_edge_types,
                )?,
            };

            for neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    next_level.push(neighbor);
                    result.push(neighbor);
                }
            }
        }

        current_level = next_level;
        if current_level.is_empty() {
            break;
        }
    }

    Ok(result)
}

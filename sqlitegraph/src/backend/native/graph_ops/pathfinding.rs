//! Shortest path algorithms using BFS

use crate::backend::native::NativeBackendError;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::*;

// Per-traversal context for 3-tier lookup (Phase 31)
use super::{TraversalContext, get_neighbors_optimized};

// Import AdjacencyHelpers for degree check (pattern detection)
use crate::backend::native::adjacency::AdjacencyHelpers;

/// Native shortest path implementation using BFS
pub fn native_shortest_path(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    end: NativeNodeId,
) -> Result<Option<Vec<NativeNodeId>>, NativeBackendError> {
    if start == end {
        return Ok(Some(vec![start]));
    }

    // Per-traversal context with 3-tier lookup (Phase 31)
    let mut ctx = TraversalContext::new();

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut parent: std::collections::HashMap<NativeNodeId, NativeNodeId> =
        std::collections::HashMap::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(current_node) = queue.pop_front() {
        if current_node == end {
            // Reconstruct path
            let mut path = vec![end];
            let mut current = end;

            while let Some(&p) = parent.get(&current) {
                path.push(p);
                current = p;
            }

            path.reverse();

            #[cfg(debug_assertions)]
            {
                let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
                if total_lookups > 0 {
                    log::debug!(
                        "Shortest path optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                        ctx.buffer_hits, ctx.buffer_misses, ctx.stats.hits, ctx.stats.misses,
                        ctx.combined_hit_rate() * 100.0
                    );
                }
            }

            return Ok(Some(path));
        }

        // Get degree for pattern detection
        let degree = AdjacencyHelpers::outgoing_degree(graph_file, current_node)?;
        let _pattern = ctx.detector.observe(current_node, degree);

        // Trigger prefetch if linear confirmed
        if ctx.detector.is_linear_confirmed() && !ctx.buffer.contains(current_node) {
            ctx.buffer.prefetch_clusters_from(graph_file, current_node)?;
        }

        let neighbors = get_neighbors_optimized(
            graph_file,
            current_node,
            crate::backend::native::adjacency::Direction::Outgoing,
            &mut ctx,
        )?;
        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                parent.insert(neighbor, current_node);
                queue.push_back(neighbor);
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
        if total_lookups > 0 {
            log::debug!(
                "Shortest path optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                ctx.buffer_hits, ctx.buffer_misses, ctx.stats.hits, ctx.stats.misses,
                ctx.combined_hit_rate() * 100.0
            );
        }
    }

    Ok(None)
}

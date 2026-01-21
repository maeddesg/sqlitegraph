//! Multiple BFS implementations with CPU-specific optimizations

use crate::backend::native::NativeBackendError;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::optimizations;
use crate::backend::native::types::*;

// Per-traversal cache for eliminating redundant I/O during BFS
// Now uses unified TraversalContext with 3-tier lookup (Phase 31)
use super::{TraversalContext, get_neighbors_optimized};
use crate::backend::native::adjacency::AdjacencyHelpers;

/// Generic scalar BFS implementation (baseline for all CPUs/large graphs)
pub fn bfs_generic_scalar(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();

    // Per-traversal context with 3-tier lookup (L1: buffer, L2: cache, L3: storage)
    let mut ctx = TraversalContext::new();

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        // Get degree for pattern detection
        let degree = AdjacencyHelpers::outgoing_degree(graph_file, current_node)?;

        // Extract cluster metadata for sequential read optimization (Phase 37-05)
        let (cluster_offset, cluster_size) = match graph_file.read_node_at(current_node) {
            Ok(node_record) => (
                node_record.outgoing_cluster_offset,
                node_record.outgoing_cluster_size,
            ),
            Err(_) => (0, 0), // Fallback if node read fails
        };

        // Observe for pattern detection with cluster metadata
        let _pattern = ctx.detector.observe_with_cluster(current_node, degree, cluster_offset, cluster_size);

        // Trigger prefetch if linear confirmed and node not in buffer
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
                result.push(neighbor);
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
        if total_lookups > 0 {
            log::debug!(
                "BFS optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                ctx.buffer_hits, ctx.buffer_misses, ctx.stats.hits, ctx.stats.misses,
                ctx.combined_hit_rate() * 100.0
            );
        }
    }

    Ok(result)
}

/// Optimized BFS with pointer table (medium graphs, SIMD-capable CPUs)
pub fn bfs_pointer_table_optimized(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();

    // Per-traversal context with 3-tier lookup (L1: buffer, L2: cache, L3: storage)
    let mut ctx = TraversalContext::new();

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        // Get degree for pattern detection
        let degree = AdjacencyHelpers::outgoing_degree(graph_file, current_node)?;

        // Extract cluster metadata for sequential read optimization (Phase 37-05)
        let (cluster_offset, cluster_size) = match graph_file.read_node_at(current_node) {
            Ok(node_record) => (
                node_record.outgoing_cluster_offset,
                node_record.outgoing_cluster_size,
            ),
            Err(_) => (0, 0), // Fallback if node read fails
        };

        let _pattern = ctx.detector.observe_with_cluster(current_node, degree, cluster_offset, cluster_size);

        // Trigger prefetch if linear confirmed
        if ctx.detector.is_linear_confirmed() && !ctx.buffer.contains(current_node) {
            ctx.buffer.prefetch_from(graph_file, current_node)?;
        }

        // Use pointer table for fast adjacency lookup
        let neighbors =
            if let Some(offsets) = optimizations::get_outgoing_edge_offsets(current_node) {
                // Fast path: use pointer table to avoid edge scanning
                let mut neighbor_ids = Vec::with_capacity(offsets.len());
                for &offset in &offsets {
                    if let Ok(edge_record) = graph_file.read_edge_at_offset(offset) {
                        neighbor_ids.push(edge_record.to_id);
                    }
                }
                neighbor_ids
            } else {
                // Fallback to 3-tier lookup (L1: buffer, L2: cache, L3: storage)
                get_neighbors_optimized(
                    graph_file,
                    current_node,
                    crate::backend::native::adjacency::Direction::Outgoing,
                    &mut ctx,
                )?
            };

        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                result.push(neighbor);
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
        if total_lookups > 0 {
            log::debug!(
                "BFS pointer table optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                ctx.buffer_hits, ctx.buffer_misses, ctx.stats.hits, ctx.stats.misses,
                ctx.combined_hit_rate() * 100.0
            );
        }
    }

    Ok(result)
}

/// Fully optimized BFS with pointer table and hot cache (small graphs, high-end CPUs)
pub fn bfs_fully_optimized(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();

    // Per-traversal context with 3-tier lookup (L1: buffer, L2: cache, L3: storage)
    let mut ctx = TraversalContext::new();

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        // Get degree for pattern detection
        let degree = AdjacencyHelpers::outgoing_degree(graph_file, current_node)?;

        // Extract cluster metadata for sequential read optimization (Phase 37-05)
        let (cluster_offset, cluster_size) = match graph_file.read_node_at(current_node) {
            Ok(node_record) => (
                node_record.outgoing_cluster_offset,
                node_record.outgoing_cluster_size,
            ),
            Err(_) => (0, 0), // Fallback if node read fails
        };

        let _pattern = ctx.detector.observe_with_cluster(current_node, degree, cluster_offset, cluster_size);

        // Trigger prefetch if linear confirmed
        if ctx.detector.is_linear_confirmed() && !ctx.buffer.contains(current_node) {
            ctx.buffer.prefetch_from(graph_file, current_node)?;
        }

        // Use both pointer table and hot cache for maximum performance
        let neighbors =
            if let Some(offsets) = optimizations::get_outgoing_edge_offsets(current_node) {
                let mut neighbor_ids = Vec::with_capacity(offsets.len());

                // Check hot cache for node metadata first
                if let Some(_hot_metadata) = optimizations::get_node_hot(current_node) {
                    // Hot cache hit - use optimized path
                    for &offset in &offsets {
                        if let Ok(edge_record) = graph_file.read_edge_at_offset(offset) {
                            neighbor_ids.push(edge_record.to_id);
                        }
                    }
                } else {
                    // Cold cache path - still use pointer table but extract hot metadata
                    for &offset in &offsets {
                        if let Ok(edge_record) = graph_file.read_edge_at_offset(offset) {
                            neighbor_ids.push(edge_record.to_id);
                        }
                    }

                    // Extract and cache hot metadata for future use
                    if let Ok(node_record) = graph_file.read_node_at(current_node) {
                        let hot_metadata = optimizations::extract_node_hot(&node_record);
                        optimizations::put_node_hot(current_node, hot_metadata);
                    }
                }

                neighbor_ids
            } else {
                // Fallback to 3-tier lookup (L1: buffer, L2: cache, L3: storage)
                get_neighbors_optimized(
                    graph_file,
                    current_node,
                    crate::backend::native::adjacency::Direction::Outgoing,
                    &mut ctx,
                )?
            };

        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                result.push(neighbor);
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let total_lookups = ctx.buffer_hits + ctx.buffer_misses + ctx.stats.hits + ctx.stats.misses;
        if total_lookups > 0 {
            log::debug!(
                "BFS fully optimized stats: buffer_hits={}, buffer_misses={}, cache_hits={}, cache_misses={}, combined_hit_rate={:.2}%",
                ctx.buffer_hits, ctx.buffer_misses, ctx.stats.hits, ctx.stats.misses,
                ctx.combined_hit_rate() * 100.0
            );
        }
    }

    Ok(result)
}

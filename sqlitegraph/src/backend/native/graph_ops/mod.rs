//! Core operations and algorithms for native graph backend.
//!
//! ## INLINE HINT STRATEGY (Phase 13 Step 5)
//!
//! ### Tier A: Tiny Hot Path Functions (#[inline(always)])
//! - `estimate_graph_size_category()`: Simple match statement for BFS dispatch
//! - `select_bfs_strategy()`: Strategy selection logic called for every BFS
//!
//! ### Tier B: Small Helper Functions (#[inline] or compiler-driven)
//! - Public API functions: Moderate complexity but good inline candidates
//!
//! ### Tier C: Large Functions (no inline hints)
//! - BFS implementations: Large algorithms left to compiler discretion
//! - `native_shortest_path()`, `native_k_hop()`: Complex algorithms
//!
//! ## Modular Architecture
//!
//! - `strategy.rs`: CPU profiling and graph size categorization
//! - `bfs_implementations.rs`: Multiple BFS implementations with optimizations
//! - `pathfinding.rs`: Shortest path algorithms using BFS
//! - `k_hop.rs`: K-hop neighbor exploration operations
//! - `chain_queries.rs`: Chain traversal and pattern matching
//! - `cache.rs`: Per-traversal adjacency cache for eliminating redundant I/O
//! - `tests.rs`: Comprehensive test suite

use super::graph_file::GraphFile;
use super::types::CpuProfile;
use super::types::*;

// Module declarations
mod bfs_implementations;
mod cache;
mod chain_queries;
mod k_hop;
mod pathfinding;
mod strategy;
mod traversal_context;

// Re-export all public functionality
pub use bfs_implementations::*;
pub use cache::{TraversalCache, TraversalCacheStats, get_neighbors_cached, get_neighbors_optimized};
pub use chain_queries::*;
pub use k_hop::*;
pub use pathfinding::*;
pub use strategy::*;

// Re-export LinearDetector from adjacency for Phase 31 traversal integration
pub use crate::backend::native::adjacency::{LinearDetector, TraversalPattern};

// Re-export TraversalContext for Phase 31 traversal integration
pub use traversal_context::TraversalContext;

/// Native BFS implementation using adjacency helpers
pub fn native_bfs(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    // Default to Auto CPU profile for backwards compatibility
    native_bfs_with_cpu_profile(graph_file, start, depth, CpuProfile::Auto)
}

/// Native BFS implementation with explicit CPU profile
pub fn native_bfs_with_cpu_profile(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
    cpu_profile: CpuProfile,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    // Get node count from header for graph size estimation
    let node_count = graph_file.persistent_header().node_count as usize;

    // Select optimal strategy based on CPU profile and graph size
    let strategy = select_bfs_strategy(cpu_profile, node_count);

    match strategy {
        "simd512_optimized" | "avx2_optimized" => bfs_fully_optimized(graph_file, start, depth),
        "simd512_pointer_table" | "avx2_pointer_table" => {
            bfs_pointer_table_optimized(graph_file, start, depth)
        }
        _ => bfs_generic_scalar(graph_file, start, depth),
    }
}

/// Native BFS implementation with telemetry export for diagnostic analysis
///
/// This function runs BFS and returns both the visited nodes and a JSON telemetry
/// string containing performance metrics from TraversalContext. Used for Phase 37-04
/// gap analysis to identify bottlenecks in Chain(500) traversal.
///
/// Returns:
/// - Ok((visited_nodes, telemetry_json)) on success
/// - Err(NativeBackendError) on failure
pub fn native_bfs_with_telemetry(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<(Vec<NativeNodeId>, String), NativeBackendError> {
    use std::time::Instant;

    if depth == 0 {
        // For zero depth, return minimal telemetry
        let ctx = TraversalContext::new();
        return Ok((vec![start], ctx.export_telemetry()));
    }

    // Get node count for strategy selection
    let node_count = graph_file.persistent_header().node_count as usize;
    let strategy = select_bfs_strategy(CpuProfile::Auto, node_count);

    // Run BFS with timing
    let start_time = Instant::now();

    let result = match strategy {
        "simd512_optimized" | "avx2_optimized" => {
            bfs_fully_optimized_with_telemetry(graph_file, start, depth)?
        }
        "simd512_pointer_table" | "avx2_pointer_table" => {
            bfs_pointer_table_optimized_with_telemetry(graph_file, start, depth)?
        }
        _ => bfs_generic_scalar_with_telemetry(graph_file, start, depth)?,
    };

    let elapsed = start_time.elapsed();

    // Update telemetry with total time
    let mut telemetry: serde_json::Value = serde_json::from_str(&result.1)
        .unwrap_or_else(|_| serde_json::json!({}));
    telemetry["time_total_ms"] = serde_json::json!(elapsed.as_secs_f64() * 1000.0);

    Ok((result.0, telemetry.to_string()))
}

// Helper functions for telemetry-enabled BFS
// These are wrappers around the existing BFS implementations that export telemetry

fn bfs_generic_scalar_with_telemetry(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<(Vec<NativeNodeId>, String), NativeBackendError> {
    if depth == 0 {
        return Ok((vec![start], TraversalContext::new().export_telemetry()));
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();
    let mut ctx = TraversalContext::new();

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        let degree = crate::backend::native::adjacency::AdjacencyHelpers::outgoing_degree(graph_file, current_node)?;

        // Extract cluster metadata for sequential read optimization (Phase 37-05)
        let (cluster_offset, cluster_size) = match graph_file.read_node_at(current_node) {
            Ok(node_record) => (
                node_record.outgoing_cluster_offset,
                node_record.outgoing_cluster_size,
            ),
            Err(_) => (0, 0), // Fallback if node read fails
        };

        let _pattern = ctx.detector.observe_with_cluster(current_node, degree, cluster_offset, cluster_size);

        // Populate node_id -> cluster_index mapping for sequential cluster extraction (Phase 35)
        let cluster_index = ctx.detector.cluster_offsets().len().saturating_sub(1);
        ctx.node_cluster_index.insert(current_node, cluster_index);

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

    Ok((result, ctx.export_telemetry()))
}

fn bfs_pointer_table_optimized_with_telemetry(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<(Vec<NativeNodeId>, String), NativeBackendError> {
    // For now, delegate to generic scalar implementation
    // Full telemetry for pointer table variant can be added later if needed
    bfs_generic_scalar_with_telemetry(graph_file, start, depth)
}

fn bfs_fully_optimized_with_telemetry(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<(Vec<NativeNodeId>, String), NativeBackendError> {
    // For now, delegate to generic scalar implementation
    // Full telemetry for fully optimized variant can be added later if needed
    bfs_generic_scalar_with_telemetry(graph_file, start, depth)
}

#[cfg(test)]
mod tests;

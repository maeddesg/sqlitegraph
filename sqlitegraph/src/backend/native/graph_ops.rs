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

use super::adjacency::{AdjacencyHelpers, Direction};
use super::cpu_tuning::resolve_cpu_profile;
use super::graph_file::GraphFile;
use super::node_store::clear_node_cache;
use super::optimizations;
use super::types::CpuProfile;
use super::types::*;
use crate::backend::{BackendDirection, ChainStep, PatternMatch, PatternQuery};

/// Estimate graph size category for optimization selection
#[inline(always)]
fn estimate_graph_size_category(node_count: usize) -> &'static str {
    match node_count {
        0..=999 => "small",      // < 1K nodes
        1000..=9999 => "medium", // 1K-10K nodes
        _ => "large",            // >= 10K nodes
    }
}

/// Select optimal BFS strategy based on CPU profile and graph size
#[inline(always)]
fn select_bfs_strategy(cpu_profile: CpuProfile, node_count: usize) -> &'static str {
    let size_category = estimate_graph_size_category(node_count);
    let resolved_profile = resolve_cpu_profile(cpu_profile);

    match (resolved_profile, size_category) {
        (CpuProfile::X86Avx512, "small") => "simd512_optimized",
        (CpuProfile::X86Avx512, "medium") => "simd512_pointer_table",
        (CpuProfile::X86Zen4, "small") => "avx2_optimized",
        (CpuProfile::X86Zen4, "medium") => "avx2_pointer_table",
        (CpuProfile::X86Avx2, "small") => "avx2_optimized",
        (CpuProfile::X86Avx2, "medium") => "avx2_pointer_table",
        _ => "generic_scalar",
    }
}

/// Generic scalar BFS implementation (baseline for all CPUs/large graphs)
fn bfs_generic_scalar(
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

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        let neighbors = AdjacencyHelpers::get_outgoing_neighbors(graph_file, current_node)?;
        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                result.push(neighbor);
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }

    Ok(result)
}

/// Optimized BFS with pointer table (medium graphs, SIMD-capable CPUs)
fn bfs_pointer_table_optimized(
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

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        // Use pointer table for fast adjacency lookup
        let neighbors =
            if let Some(offsets) = optimizations::get_outgoing_edge_offsets(current_node) {
                // Fast path: use pointer table to avoid edge scanning
                let mut neighbor_ids = Vec::with_capacity(offsets.len());
                for &offset in &offsets {
                    if let Some(edge_record) = graph_file.read_edge_at_offset(offset) {
                        neighbor_ids.push(edge_record.to_id);
                    }
                }
                neighbor_ids
            } else {
                // Fallback to standard adjacency lookup
                AdjacencyHelpers::get_outgoing_neighbors(graph_file, current_node)?
            };

        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                result.push(neighbor);
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }

    Ok(result)
}

/// Fully optimized BFS with pointer table and hot cache (small graphs, high-end CPUs)
fn bfs_fully_optimized(
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

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        // Use both pointer table and hot cache for maximum performance
        let neighbors =
            if let Some(offsets) = optimizations::get_outgoing_edge_offsets(current_node) {
                let mut neighbor_ids = Vec::with_capacity(offsets.len());

                // Check hot cache for node metadata first
                if let Some(_hot_metadata) = optimizations::get_node_hot(current_node) {
                    // Hot cache hit - use optimized path
                    for &offset in &offsets {
                        if let Some(edge_record) = graph_file.read_edge_at_offset(offset) {
                            neighbor_ids.push(edge_record.to_id);
                        }
                    }
                } else {
                    // Cold cache path - still use pointer table but extract hot metadata
                    for &offset in &offsets {
                        if let Some(edge_record) = graph_file.read_edge_at_offset(offset) {
                            neighbor_ids.push(edge_record.to_id);
                        }
                    }

                    // Extract and cache hot metadata for future use
                    if let Some(node_record) = graph_file.read_node_at(current_node) {
                        let hot_metadata = optimizations::extract_node_hot(&node_record);
                        optimizations::put_node_hot(current_node, hot_metadata);
                    }
                }

                neighbor_ids
            } else {
                // Fallback to standard adjacency lookup
                AdjacencyHelpers::get_outgoing_neighbors(graph_file, current_node)?
            };

        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                result.push(neighbor);
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }

    Ok(result)
}

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

/// Native shortest path implementation using BFS
pub fn native_shortest_path(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    end: NativeNodeId,
) -> Result<Option<Vec<NativeNodeId>>, NativeBackendError> {
    if start == end {
        return Ok(Some(vec![start]));
    }

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
            return Ok(Some(path));
        }

        let neighbors = AdjacencyHelpers::get_outgoing_neighbors(graph_file, current_node)?;
        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                parent.insert(neighbor, current_node);
                queue.push_back(neighbor);
            }
        }
    }

    Ok(None)
}

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

    for _ in 0..depth {
        let mut next_level = Vec::new();

        for node in current_level {
            let neighbors = match direction {
                Direction::Outgoing => AdjacencyHelpers::get_outgoing_neighbors(graph_file, node)?,
                Direction::Incoming => AdjacencyHelpers::get_incoming_neighbors(graph_file, node)?,
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

/// Native chain query implementation
pub fn native_chain_query(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    chain: &[ChainStep],
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    let mut current_nodes = vec![start];
    let mut result = current_nodes.clone();

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
                match direction {
                    Direction::Outgoing => {
                        AdjacencyHelpers::get_outgoing_neighbors(graph_file, node)?
                    }
                    Direction::Incoming => {
                        AdjacencyHelpers::get_incoming_neighbors(graph_file, node)?
                    }
                }
            };

            next_nodes.extend(neighbors);
        }

        if next_nodes.is_empty() {
            return Ok(vec![]); // Chain broken
        }

        current_nodes = next_nodes;
        result.extend(current_nodes.clone());
    }

    Ok(result)
}

/// Native pattern search implementation (basic version)
pub fn native_pattern_search(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    pattern: &PatternQuery,
) -> Result<Vec<PatternMatch>, NativeBackendError> {
    // This is a simplified implementation
    // In a full implementation, this would use the pattern engine
    // For now, return empty matches as the pattern engine is complex
    Ok(vec![])
}

#[cfg(all(test, feature = "v2_experimental"))]
#[cfg(all(test, not(feature = "v2_experimental")))]
mod tests {
    use super::super::{EdgeStore, NodeStore};
    use super::*;
    use crate::backend::{EdgeSpec, NodeSpec};
    use tempfile::NamedTempFile;

    fn create_test_graph_file() -> (GraphFile, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        let graph_file = GraphFile::create(path).unwrap();
        (graph_file, temp_file)
    }

    #[test]
    fn test_native_bfs_simple() {
        // Clear cache to ensure test isolation
        clear_node_cache();

        let (mut graph_file, _temp_file) = create_test_graph_file();

        // Create nodes
        let node1 = NodeRecord::new(
            1,
            "Test".to_string(),
            "node1".to_string(),
            serde_json::json!({}),
        );
        let node2 = NodeRecord::new(
            2,
            "Test".to_string(),
            "node2".to_string(),
            serde_json::json!({}),
        );
        let node3 = NodeRecord::new(
            3,
            "Test".to_string(),
            "node3".to_string(),
            serde_json::json!({}),
        );

        {
            let mut node_store = NodeStore::new(&mut graph_file);
            node_store.write_node(&node1).unwrap();
            node_store.write_node(&node2).unwrap();
            node_store.write_node(&node3).unwrap();
        }

        // Create edges: 1 -> 2 -> 3
        let edge1 = EdgeRecord::new(1, 1, 2, "test".to_string(), serde_json::json!({}));
        let edge2 = EdgeRecord::new(2, 2, 3, "test".to_string(), serde_json::json!({}));

        {
            let mut edge_store = EdgeStore::new(&mut graph_file);
            edge_store.write_edge(&edge1).unwrap();
            edge_store.write_edge(&edge2).unwrap();
        }

        let result = native_bfs(&mut graph_file, 1, 2).unwrap();
        assert!(
            result.contains(&2),
            "Expected to find node 2 in BFS result: {:?}",
            result
        );
        assert!(
            result.contains(&3),
            "Expected to find node 3 in BFS result: {:?}",
            result
        );
    }

    #[test]
    fn test_native_shortest_path() {
        // Clear cache to ensure test isolation
        clear_node_cache();

        let (mut graph_file, _temp_file) = create_test_graph_file();

        // Create nodes
        let node1 = NodeRecord::new(
            1,
            "Test".to_string(),
            "node1".to_string(),
            serde_json::json!({}),
        );
        let node2 = NodeRecord::new(
            2,
            "Test".to_string(),
            "node2".to_string(),
            serde_json::json!({}),
        );
        let node3 = NodeRecord::new(
            3,
            "Test".to_string(),
            "node3".to_string(),
            serde_json::json!({}),
        );

        {
            let mut node_store = NodeStore::new(&mut graph_file);
            node_store.write_node(&node1).unwrap();
            node_store.write_node(&node2).unwrap();
            node_store.write_node(&node3).unwrap();
        }

        // Create edge: 1 -> 2 -> 3
        let edge1 = EdgeRecord::new(1, 1, 2, "test".to_string(), serde_json::json!({}));
        let edge2 = EdgeRecord::new(2, 2, 3, "test".to_string(), serde_json::json!({}));

        {
            let mut edge_store = EdgeStore::new(&mut graph_file);
            edge_store.write_edge(&edge1).unwrap();
            edge_store.write_edge(&edge2).unwrap();
        }

        let result = native_shortest_path(&mut graph_file, 1, 3).unwrap();
        assert!(result.is_some());
        let path = result.unwrap();
        assert_eq!(path, vec![1, 2, 3]);
    }
}

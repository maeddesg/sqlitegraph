//! Shortest path algorithms using BFS

use crate::backend::native::NativeBackendError;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::*;

// Per-traversal cache for eliminating redundant I/O during shortest path search
use super::{TraversalCache, get_neighbors_cached, TraversalCacheStats};

/// Native shortest path implementation using BFS
pub fn native_shortest_path(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    end: NativeNodeId,
) -> Result<Option<Vec<NativeNodeId>>, NativeBackendError> {
    if start == end {
        return Ok(Some(vec![start]));
    }

    // Per-traversal cache - evaporates when function returns
    let mut cache: TraversalCache = TraversalCache::new();
    let mut stats = TraversalCacheStats::default();

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
                if stats.hits + stats.misses > 0 {
                    let hit_rate = stats.hit_rate();
                    log::debug!(
                        "Shortest path cache stats: hits={}, misses={}, hit_rate={:.2}%",
                        stats.hits,
                        stats.misses,
                        hit_rate * 100.0
                    );
                }
            }

            return Ok(Some(path));
        }

        let neighbors = get_neighbors_cached(
            graph_file,
            current_node,
            crate::backend::native::adjacency::Direction::Outgoing,
            &mut cache,
            &mut stats,
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
        if stats.hits + stats.misses > 0 {
            let hit_rate = stats.hit_rate();
            log::debug!(
                "Shortest path cache stats: hits={}, misses={}, hit_rate={:.2}%",
                stats.hits,
                stats.misses,
                hit_rate * 100.0
            );
        }
    }

    Ok(None)
}

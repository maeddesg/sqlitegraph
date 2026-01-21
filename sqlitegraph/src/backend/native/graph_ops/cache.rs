//! Per-traversal cache for graph operations.
//!
//! This module provides a simple, efficient cache for node adjacency data during
//! graph traversal operations (BFS, k-hop, shortest path, etc.).
//!
//! # Design: Per-Traversal Cache
//!
//! The cache is scoped to each traversal operation and evaporates when the
//! traversal function returns. This design:
//!
//! - **Preserves MVCC isolation**: No cross-transaction cache staleness
//! - **Avoids invalidation complexity**: Cache dies with the function
//! - **Eliminates redundant I/O**: Nodes visited multiple times during traversal
//!   are cached in memory
//! - **Zero thread-safety overhead**: Single-threaded traversal, no atomics needed
//!
//! # Cache Key Design
//!
//! The cache key combines `(node_id, direction)` to handle cases where a node
//! has different outgoing vs incoming neighbors. This allows the same traversal
//! to efficiently query both directions without cache pollution.
//!
//! # Usage Pattern
//!
//! ```rust
//! use crate::backend::native::graph_ops::cache::{TraversalCache, TraversalCacheStats, get_neighbors_cached};
//! use crate::backend::native::adjacency::Direction;
//!
//! fn bfs_with_cache(graph_file: &mut GraphFile, start: NativeNodeId, depth: u32) -> NativeResult<Vec<NativeNodeId>> {
//!     let mut cache = TraversalCache::new();
//!     let mut stats = TraversalCacheStats::default();
//!
//!     // ... traversal loop ...
//!     let neighbors = get_neighbors_cached(
//!         graph_file,
//!         current_node,
//!         Direction::Outgoing,
//!         &mut cache,
//!         &mut stats,
//!     )?;
//!     // ... use neighbors ...
//!
//!     // Cache evaporates here when function returns
//!     Ok(result)
//! }
//! ```

use ahash::AHashMap;
use crate::backend::native::adjacency::{AdjacencyHelpers, Direction};
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::{NativeNodeId, NativeResult};

/// Per-traversal cache key combining node ID and direction.
///
/// Using both node_id and direction as the key ensures that:
/// - Outgoing and incoming neighbors are cached separately
/// - No cache pollution when querying both directions in same traversal
/// - Simple hash-based lookup with ahash for performance
type CacheKey = (NativeNodeId, Direction);

/// Per-traversal cache for node adjacency data.
///
/// Key: (node_id, direction) tuple
/// Value: Vector of neighbor node IDs
///
/// The cache is created at the start of a traversal operation and dropped
/// when the traversal completes. No explicit invalidation is needed.
pub type TraversalCache = AHashMap<CacheKey, Vec<NativeNodeId>>;

/// Cache statistics for tracking hit/miss rates during traversal.
///
/// Used for performance validation (PERF-07) to measure cache effectiveness.
/// Non-atomic counters are safe because traversals are single-threaded.
#[derive(Debug, Default, Clone, Copy)]
pub struct TraversalCacheStats {
    /// Number of cache hits (neighbors served from cache)
    pub hits: u64,
    /// Number of cache misses (neighbors loaded from storage)
    pub misses: u64,
}

impl TraversalCacheStats {
    /// Create new zero-initialized statistics.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate cache hit rate as a fraction (0.0 to 1.0).
    ///
    /// Returns 0.0 if no cache operations were performed.
    #[inline]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Record a cache hit.
    #[inline]
    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    /// Record a cache miss.
    #[inline]
    pub fn record_miss(&mut self) {
        self.misses += 1;
    }
}

/// Get neighbors for a node with per-traversal caching.
///
/// This helper function checks the cache before falling through to
/// `AdjacencyHelpers`, reducing redundant I/O during traversals.
///
/// # Parameters
///
/// - `graph_file`: Mutable borrow of the graph file for I/O operations
/// - `node_id`: The node whose neighbors we want to fetch
/// - `direction`: Whether to get Outgoing or Incoming neighbors
/// - `cache`: Mutable borrow of the per-traversal cache
/// - `stats`: Mutable borrow of statistics tracking (optional)
///
/// # Returns
///
/// An owned `Vec<NativeNodeId>` containing the neighbor node IDs.
///
/// **Note:** Returns an owned `Vec` (not a reference) to avoid borrow
/// checker lifetime issues. The clone is cheap for typical neighbor list sizes.
///
/// # Example
///
/// ```rust
/// let mut cache = TraversalCache::new();
/// let mut stats = TraversalCacheStats::default();
///
/// let neighbors = get_neighbors_cached(
///     graph_file,
///     node_id,
///     Direction::Outgoing,
///     &mut cache,
///     &mut stats,
/// )?;
/// ```
pub fn get_neighbors_cached(
    graph_file: &mut GraphFile,
    node_id: NativeNodeId,
    direction: Direction,
    cache: &mut TraversalCache,
    stats: &mut TraversalCacheStats,
) -> NativeResult<Vec<NativeNodeId>> {
    let cache_key = (node_id, direction);

    // Check cache first - this is the hot path
    if let Some(cached) = cache.get(&cache_key) {
        stats.record_hit();
        // Return cloned Vec to avoid borrow checker issues
        // Clone is cheap for typical neighbor list sizes
        return Ok(cached.clone());
    }

    // Cache miss - load from storage via AdjacencyHelpers
    stats.record_miss();

    let neighbors = match direction {
        Direction::Outgoing => {
            AdjacencyHelpers::get_outgoing_neighbors(graph_file, node_id)?
        }
        Direction::Incoming => {
            AdjacencyHelpers::get_incoming_neighbors(graph_file, node_id)?
        }
    };

    // Insert into cache for future lookups in this traversal
    cache.insert(cache_key, neighbors.clone());

    Ok(neighbors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::types::NativeBackendError;

    #[test]
    fn test_cache_stats_new() {
        let stats = TraversalCacheStats::new();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = TraversalCacheStats::default();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = TraversalCacheStats::new();

        // No operations yet
        assert_eq!(stats.hit_rate(), 0.0);

        // Record some hits and misses
        stats.record_hit();
        stats.record_hit();
        stats.record_miss();
        stats.record_miss();

        // 2 hits out of 4 total = 0.5
        assert!((stats.hit_rate() - 0.5).abs() < f64::EPSILON);

        // All hits
        stats.hits = 10;
        stats.misses = 0;
        assert_eq!(stats.hit_rate(), 1.0);

        // All misses
        stats.hits = 0;
        stats.misses = 10;
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_key_combines_node_and_direction() {
        // Cache keys with same node but different direction are distinct
        let key1: CacheKey = (42, Direction::Outgoing);
        let key2: CacheKey = (42, Direction::Incoming);

        assert_ne!(key1, key2, "Keys should differ by direction");

        // Cache keys with same direction but different node are distinct
        let key3: CacheKey = (42, Direction::Outgoing);
        let key4: CacheKey = (99, Direction::Outgoing);

        assert_ne!(key3, key4, "Keys should differ by node_id");

        // Cache keys with same node and direction are equal
        let key5: CacheKey = (42, Direction::Outgoing);
        let key6: CacheKey = (42, Direction::Outgoing);

        assert_eq!(key5, key6, "Keys should be identical");
    }

    #[test]
    fn test_traversal_cache_type_alias() {
        // TraversalCache is just AHashMap with our key type
        let cache: TraversalCache = TraversalCache::new();
        assert_eq!(cache.len(), 0);

        // Can insert and retrieve
        let mut cache: TraversalCache = AHashMap::new();
        cache.insert((1, Direction::Outgoing), vec![2, 3, 4]);
        cache.insert((1, Direction::Incoming), vec![0]);

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&(1, Direction::Outgoing)), Some(&vec![2, 3, 4]));
        assert_eq!(cache.get(&(1, Direction::Incoming)), Some(&vec![0]));
    }
}

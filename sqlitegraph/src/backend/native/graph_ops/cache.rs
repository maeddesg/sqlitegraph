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
use crate::backend::native::v2::edge_cluster::EdgeCluster;

// Forward declaration for TraversalContext
// TraversalContext is defined in traversal_context.rs (sibling module)
// The module will be declared in mod.rs in Task 3 of this plan.
// Using full crate path to avoid circular dependency issues.
use crate::backend::native::graph_ops::traversal_context::TraversalContext;

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

/// Get neighbors with 3-tier lookup hierarchy for sequential I/O optimization.
///
/// This function implements a tiered lookup strategy that prioritizes faster caches:
///
/// # Lookup Hierarchy
///
/// 1. **L1: SequentialReadBuffer** - Decoded node slots from batched reads (fastest)
///    - Only checked after LinearDetector confirms linear pattern
///    - Extracts neighbors directly from buffered NodeRecordV2
///    - Checks cluster cache before doing file I/O (Phase 32-05)
///    - Records buffer hit/miss for statistics
///
/// 2. **L2: TraversalCache** - Cached neighbor lists (from v1.3)
///    - Stores Vec<NativeNodeId> for (node_id, direction) keys
///    - Records cache hit/miss for statistics
///
/// 3. **L3: AdjacencyHelpers** - Storage I/O (slowest)
///    - Falls through to disk when L1 and L2 miss
///    - Results are inserted into L2 cache for future lookups
///
/// # Parameters
///
/// - `graph_file`: Mutable borrow of the graph file for I/O operations
/// - `node_id`: The node whose neighbors we want to fetch
/// - `direction`: Whether to get Outgoing or Incoming neighbors
/// - `ctx`: Mutable borrow of the unified traversal context
///
/// # Returns
///
/// An owned `Vec<NativeNodeId>` containing the neighbor node IDs.
///
/// # L1 Buffer Extraction with Cluster Cache (Phase 32-04, 32-05)
///
/// When a node is found in the SequentialReadBuffer and has valid cluster metadata,
/// this function first checks if the cluster data is cached. If cached, it deserializes
/// from the cached bytes (no I/O). If not cached, it reads from file.
///
/// The extraction process:
/// 1. Check if node is in buffer (only after linear pattern confirmed)
/// 2. Extract cluster_offset and cluster_size based on direction
/// 3. Check cluster cache: if cached, deserialize from cached bytes (no I/O)
/// 4. If not cached, read cluster data from file
/// 5. Deserialize EdgeCluster and extract neighbors via iter_neighbors()
/// 6. Return neighbors immediately (early return, no L2/L3 fallback)
///
/// If buffer miss or cluster metadata is invalid, fall through to L2/L3.
///
/// # Example
///
/// ```rust
/// use crate::backend::native::graph_ops::{TraversalContext, get_neighbors_optimized};
/// use crate::backend::native::adjacency::Direction;
///
/// let mut ctx = TraversalContext::new();
///
/// // During traversal with pattern detection
/// let degree = AdjacencyHelpers::outgoing_degree(graph_file, node_id)?;
/// ctx.detector.observe(node_id, degree);
///
/// // Trigger prefetch with cluster caching if linear confirmed
/// if ctx.detector.is_linear_confirmed() && !ctx.buffer.contains(node_id) {
///     ctx.buffer.prefetch_clusters_from(graph_file, node_id)?;
/// }
///
/// // Get neighbors via 3-tier lookup
/// let neighbors = get_neighbors_optimized(
///     graph_file,
///     node_id,
///     Direction::Outgoing,
///     &mut ctx,
/// )?;
/// ```
pub fn get_neighbors_optimized(
    graph_file: &mut GraphFile,
    node_id: NativeNodeId,
    direction: Direction,
    ctx: &mut TraversalContext,
) -> NativeResult<Vec<NativeNodeId>> {
    let cache_key = (node_id, direction);

    // L1: Check SequentialReadBuffer first (fastest path)
    // Only check after linear pattern is confirmed
    if ctx.detector.is_linear_confirmed() {
        // Extract cluster metadata from buffer if node exists
        let l1_result = ctx.buffer.get(node_id).map(|node_record| {
            let (cluster_offset, cluster_size) = match direction {
                Direction::Outgoing => (
                    node_record.outgoing_cluster_offset,
                    node_record.outgoing_cluster_size,
                ),
                Direction::Incoming => (
                    node_record.incoming_cluster_offset,
                    node_record.incoming_cluster_size,
                ),
            };
            (cluster_offset, cluster_size)
        });

        if let Some((cluster_offset, cluster_size)) = l1_result {
            ctx.record_buffer_hit();

            // If cluster metadata is valid, extract neighbors from buffer
            if cluster_offset > 0 && cluster_size > 0 {
                // First, check if cluster data is already cached in buffer
                if let Some(cached_cluster_bytes) = ctx.buffer.get_cluster(cluster_offset) {
                    // Cluster cache hit - deserialize from cached bytes (no I/O)
                    if let Ok(cluster) = EdgeCluster::deserialize(cached_cluster_bytes) {
                        let neighbors: Vec<NativeNodeId> =
                            cluster.iter_neighbors().map(|id| id as NativeNodeId).collect();

                        // Insert into L2 cache for future lookups in this traversal
                        ctx.cache.insert(cache_key, neighbors.clone());

                        return Ok(neighbors);
                    }
                    // If deserialization fails, fall through to file I/O below
                }

                // Cluster cache miss - read cluster data from file
                let mut cluster_data = vec![0u8; cluster_size as usize];
                if graph_file.read_bytes(cluster_offset, &mut cluster_data).is_ok() {
                    // Deserialize edge cluster and extract neighbors
                    if let Ok(cluster) = EdgeCluster::deserialize(&cluster_data) {
                        let neighbors: Vec<NativeNodeId> =
                            cluster.iter_neighbors().map(|id| id as NativeNodeId).collect();

                        // Insert into L2 cache for future lookups in this traversal
                        ctx.cache.insert(cache_key, neighbors.clone());

                        return Ok(neighbors);
                    }
                    // If deserialization fails, fall through to L2/L3
                }
                // If read fails, fall through to L2/L3
            }
            // If cluster_offset == 0 or cluster_size == 0, node has no edges in this direction
            // Return empty neighbors immediately
            else {
                ctx.cache.insert(cache_key, Vec::new());
                return Ok(Vec::new());
            }
        } else {
            ctx.record_buffer_miss();
        }
    }

    // L2: Check TraversalCache (v1.3 cache)
    if let Some(cached) = ctx.cache.get(&cache_key) {
        ctx.stats.record_hit();
        return Ok(cached.clone());
    }
    ctx.stats.record_miss();

    // L3: Load from storage via AdjacencyHelpers
    let neighbors = match direction {
        Direction::Outgoing => {
            AdjacencyHelpers::get_outgoing_neighbors(graph_file, node_id)?
        }
        Direction::Incoming => {
            AdjacencyHelpers::get_incoming_neighbors(graph_file, node_id)?
        }
    };

    // Insert into cache for future lookups
    ctx.cache.insert(cache_key, neighbors.clone());

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

//! Per-traversal context for sequential I/O optimization.
//!
//! This module provides a unified context struct that combines the three components
//! needed for sequential I/O optimization during graph traversals:
//!
//! - **LinearDetector**: Pattern detection (Phase 29) - identifies linear chains
//! - **SequentialReadBuffer**: Decoded slot buffer (Phase 30) - caches node records
//! - **TraversalCache**: Neighbor list cache (v1.3) - caches adjacency lists
//!
//! The context is stack-allocated per traversal and evaporates when the traversal
//! function returns, preserving MVCC isolation.

use crate::backend::native::adjacency::{LinearDetector, SequentialReadBuffer};
use crate::backend::native::graph_ops::{TraversalCache, TraversalCacheStats};
use crate::backend::native::types::NativeNodeId;
use ahash::AHashMap;
use serde_json::json;

/// Per-traversal context for optimized I/O
///
/// This struct combines the three components needed for sequential I/O optimization:
/// - LinearDetector: Pattern detection (3-step threshold for linear chains)
/// - SequentialReadBuffer: Decoded slot buffer (8-slot prefetch, 32KB)
/// - TraversalCache: Neighbor list cache (from v1.3)
///
/// The context is stack-allocated per traversal and evaporates when the
/// traversal function returns, preserving MVCC isolation.
///
/// # Fields
///
/// - **detector**: Linear pattern detector (3-step threshold)
/// - **buffer**: Sequential read buffer (8-slot prefetch, 32KB)
/// - **cache**: Traversal cache (from v1.3, neighbor lists)
/// - **stats**: Cache statistics (for debug logging)
/// - **buffer_hits**: Extended statistics: L1 buffer hits
/// - **buffer_misses**: Extended statistics: L1 buffer misses
/// - **cluster_buffer**: Raw bytes from sequential cluster read (Phase 34)
/// - **cluster_buffer_offsets**: Cluster offsets for positioning (Phase 34)
/// - **time_total_ms**: Total traversal time (Phase 37 telemetry)
/// - **nodes_visited**: Number of nodes visited (Phase 37 telemetry)
/// - **overshoot_count**: Cluster buffer overshoot corrections (Phase 37 telemetry)
/// - **undershoot_count**: Cluster buffer undershoot corrections (Phase 37 telemetry)
/// - **cluster_buffer_reallocs**: Buffer reallocation count (Phase 37 telemetry)
/// - **dedupe_time_ms**: Deduplication time (Phase 37 telemetry)
/// - **sort_time_ms**: Sort time (Phase 37 telemetry)
///
/// # Example
///
/// ```rust
/// use crate::backend::native::graph_ops::TraversalContext;
///
/// let mut ctx = TraversalContext::new();
///
/// // During traversal
/// ctx.record_buffer_hit();
/// ctx.record_buffer_miss();
///
/// let hit_rate = ctx.combined_hit_rate();
/// ```
pub struct TraversalContext {
    /// Pattern detection state machine
    pub detector: LinearDetector,

    /// Sequential slot buffer (L1 cache)
    pub buffer: SequentialReadBuffer,

    /// Neighbor list cache (L2 cache)
    pub cache: TraversalCache,

    /// Cache statistics for debugging
    pub stats: TraversalCacheStats,

    /// Buffer hit tracking (extended stats)
    pub buffer_hits: u64,

    /// Buffer miss tracking (extended stats)
    pub buffer_misses: u64,

    /// Raw bytes from sequential cluster read (all clusters in one I/O)
    ///
    /// Stored as Vec<u8> to defer deserialization until neighbor extraction.
    /// Populated by SequentialClusterReader::read_chain_clusters() when
    /// LinearDetector confirms a linear chain with contiguous clusters.
    ///
    /// Phase 34: Sequential Cluster Reader
    pub cluster_buffer: Option<Vec<u8>>,

    /// Cluster offsets corresponding to cluster_buffer (for positioning)
    ///
    /// Copied from detector.cluster_offsets() when sequential read is triggered.
    /// Used by SequentialClusterReader::extract_neighbors() to calculate
    /// byte offsets within the buffer for each cluster.
    ///
    /// Phase 34: Sequential Cluster Reader
    pub cluster_buffer_offsets: Vec<(u64, u32)>,

    /// Node_id -> cluster_index mapping for sequential cluster extraction (Phase 35)
    ///
    /// Maps each observed node_id to its cluster index in the sequential cluster buffer.
    /// When a linear chain with contiguous clusters is confirmed, this mapping enables
    /// extracting neighbors from the buffered cluster bytes without additional I/O.
    ///
    /// The mapping is populated during traversal via observe_with_cluster() and
    /// cleared on fallback via clear_cluster_buffer().
    ///
    /// **Memory:** O(chain_length) entries, one per node in the detected chain.
    /// **Lookup:** O(1) via AHashMap for hot neighbor extraction path.
    pub node_cluster_index: AHashMap<NativeNodeId, usize>,

    /// Total traversal time in milliseconds (Phase 37 telemetry)
    pub time_total_ms: f64,

    /// Number of nodes visited during traversal (Phase 37 telemetry)
    pub nodes_visited: u64,

    /// Cluster buffer overshoot corrections (Phase 37 telemetry)
    pub overshoot_count: u64,

    /// Cluster buffer undershoot corrections (Phase 37 telemetry)
    pub undershoot_count: u64,

    /// Buffer reallocation count (Phase 37 telemetry)
    pub cluster_buffer_reallocs: u64,

    /// Deduplication time in milliseconds (Phase 37 telemetry)
    pub dedupe_time_ms: f64,

    /// Sort time in milliseconds (Phase 37 telemetry)
    pub sort_time_ms: f64,
}

impl TraversalContext {
    /// Create new traversal context with default components
    ///
    /// Initializes all fields with their default values:
    /// - LinearDetector with threshold of 3
    /// - SequentialReadBuffer with 8-slot prefetch window
    /// - Empty TraversalCache
    /// - Zero-initialized statistics
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::graph_ops::TraversalContext;
    ///
    /// let ctx = TraversalContext::new();
    /// assert!(!ctx.detector.is_linear_confirmed());
    /// assert_eq!(ctx.buffer.len(), 0);
    /// assert_eq!(ctx.buffer_hits, 0);
    /// assert_eq!(ctx.buffer_misses, 0);
    /// ```
    pub fn new() -> Self {
        Self {
            detector: LinearDetector::new(),
            buffer: SequentialReadBuffer::new(),
            cache: TraversalCache::new(),
            stats: TraversalCacheStats::new(),
            buffer_hits: 0,
            buffer_misses: 0,
            cluster_buffer: None,
            cluster_buffer_offsets: Vec::new(),
            node_cluster_index: AHashMap::new(),
            time_total_ms: 0.0,
            nodes_visited: 0,
            overshoot_count: 0,
            undershoot_count: 0,
            cluster_buffer_reallocs: 0,
            dedupe_time_ms: 0.0,
            sort_time_ms: 0.0,
        }
    }

    /// Record a buffer hit (L1 cache hit)
    ///
    /// Increments the buffer_hits counter to track successful L1 cache lookups.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_buffer_hit();
    /// assert_eq!(ctx.buffer_hits, 1);
    /// ```
    pub fn record_buffer_hit(&mut self) {
        self.buffer_hits += 1;
    }

    /// Record a buffer miss (L1 cache miss)
    ///
    /// Increments the buffer_misses counter to track failed L1 cache lookups.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_buffer_miss();
    /// assert_eq!(ctx.buffer_misses, 1);
    /// ```
    pub fn record_buffer_miss(&mut self) {
        self.buffer_misses += 1;
    }

    /// Calculate combined hit rate (L1 + L2 cache)
    ///
    /// Returns the combined hit rate across both cache tiers as a fraction
    /// from 0.0 to 1.0. The calculation includes:
    /// - L1 buffer hits (buffer_hits)
    /// - L2 cache hits (stats.hits)
    /// - All misses (buffer_misses + stats.misses)
    ///
    /// Returns 0.0 if no lookups were performed.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    ///
    /// // No operations yet
    /// assert_eq!(ctx.combined_hit_rate(), 0.0);
    ///
    /// // Record some hits and misses
    /// ctx.buffer_hits = 5;
    /// ctx.stats.hits = 3;
    /// ctx.buffer_misses = 2;
    /// ctx.stats.misses = 1;
    ///
    /// // 8 hits out of 11 total = ~0.73
    /// let rate = ctx.combined_hit_rate();
    /// assert!(rate > 0.7 && rate < 0.75);
    /// ```
    pub fn combined_hit_rate(&self) -> f64 {
        let total_hits = self.buffer_hits + self.stats.hits;
        let total_lookups = total_hits + self.buffer_misses + self.stats.misses;
        if total_lookups == 0 {
            0.0
        } else {
            total_hits as f64 / total_lookups as f64
        }
    }

    /// Clear cluster buffer (called on traversal reset or fallback)
    ///
    /// Clears both the raw cluster buffer and offset tracking. Called when:
    /// - Traversal resets via reset()
    /// - Sequential read fails and we fall back to standard path
    /// - Pattern breaks (branching detected) during traversal
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    ///
    /// // Simulate sequential read populating buffer
    /// ctx.cluster_buffer = Some(vec![1, 2, 3, 4]);
    /// ctx.cluster_buffer_offsets = vec![(100, 4), (104, 4)];
    ///
    /// ctx.clear_cluster_buffer();
    ///
    /// assert!(ctx.cluster_buffer.is_none());
    /// assert!(ctx.cluster_buffer_offsets.is_empty());
    /// ```
    pub fn clear_cluster_buffer(&mut self) {
        self.cluster_buffer = None;
        self.cluster_buffer_offsets.clear();
        self.node_cluster_index.clear();
    }

    /// Record a node visit (Phase 37 telemetry)
    ///
    /// Increments the nodes_visited counter to track how many nodes
    /// were processed during traversal.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_node_visit();
    /// ctx.record_node_visit();
    /// assert_eq!(ctx.nodes_visited, 2);
    /// ```
    pub fn record_node_visit(&mut self) {
        self.nodes_visited += 1;
    }

    /// Record a cluster buffer overshoot correction (Phase 37 telemetry)
    ///
    /// Increments the overshoot_count counter when the sequential cluster buffer
    /// had to be adjusted due to size overestimation.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_overshoot();
    /// assert_eq!(ctx.overshoot_count, 1);
    /// ```
    pub fn record_overshoot(&mut self) {
        self.overshoot_count += 1;
    }

    /// Record a cluster buffer undershoot correction (Phase 37 telemetry)
    ///
    /// Increments the undershoot_count counter when the sequential cluster buffer
    /// had to be adjusted due to size underestimation.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_undershoot();
    /// assert_eq!(ctx.undershoot_count, 1);
    /// ```
    pub fn record_undershoot(&mut self) {
        self.undershoot_count += 1;
    }

    /// Record a cluster buffer reallocation (Phase 37 telemetry)
    ///
    /// Increments the cluster_buffer_reallocs counter when the buffer
    /// had to be reallocated during traversal.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_buffer_realloc();
    /// assert_eq!(ctx.cluster_buffer_reallocs, 1);
    /// ```
    pub fn record_buffer_realloc(&mut self) {
        self.cluster_buffer_reallocs += 1;
    }

    /// Get cluster info for a node (Phase 37-05)
    ///
    /// Extracts cluster metadata (offset and size) for the given node_id.
    /// This helper method isolates cluster metadata extraction logic for
    /// clean cluster-aware observation in BFS and other traversals.
    ///
    /// # Arguments
    ///
    /// * `graph_file` - Mutable reference to the graph file for reading node records
    /// * `node_id` - Node ID to extract cluster metadata for
    ///
    /// # Returns
    ///
    /// - `Some((offset, size))` - Cluster offset and size if node record exists
    /// - `None` - If node record read fails (fallback case)
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::graph_ops::TraversalContext;
    ///
    /// // In BFS traversal:
    /// let (cluster_offset, cluster_size) = ctx.get_cluster_info(graph_file, current_node)
    ///     .unwrap_or((0, 0));
    /// let _pattern = ctx.detector.observe_with_cluster(
    ///     current_node, degree, cluster_offset, cluster_size
    /// );
    /// ```
    pub fn get_cluster_info(
        &self,
        graph_file: &mut crate::backend::native::graph_file::GraphFile,
        node_id: NativeNodeId,
    ) -> Option<(u64, u32)> {
        match graph_file.read_node_at(node_id) {
            Ok(node_record) => Some((
                node_record.outgoing_cluster_offset,
                node_record.outgoing_cluster_size,
            )),
            Err(_) => None, // Node read failed, return None
        }
    }

    /// Calculate fragmentation score from cluster offsets (Phase 37 telemetry)
    ///
    /// Fragmentation is the ratio of gap bytes to total bytes in the
    /// cluster offset history. Higher fragmentation indicates more
    /// non-contiguous storage, which reduces sequential I/O effectiveness.
    ///
    /// # Returns
    ///
    /// Fragmentation score as f64 in range [0.0, 1.0], where 0.0 means
    /// perfectly contiguous and 1.0 means completely fragmented.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// // Clusters are contiguous
    /// assert_eq!(ctx.calculate_fragmentation(), 0.0);
    /// ```
    fn calculate_fragmentation(&self) -> f64 {
        let gap_bytes = self.calculate_gap_bytes();
        if gap_bytes == 0 {
            return 0.0;
        }

        // Calculate total bytes spanned by all clusters
        let offsets = self.detector.cluster_offsets();
        if offsets.is_empty() {
            return 0.0;
        }

        let first_offset = offsets[0].0;
        let last_offset = offsets.last().unwrap().0;
        let last_size = offsets.last().unwrap().1;
        let total_span = (last_offset + last_size as u64) - first_offset;

        if total_span == 0 {
            0.0
        } else {
            gap_bytes as f64 / total_span as f64
        }
    }

    /// Calculate total gap bytes between non-contiguous clusters (Phase 37 telemetry)
    ///
    /// Scans the cluster offset history and sums all gaps between clusters
    /// that are not perfectly contiguous.
    ///
    /// # Returns
    ///
    /// Total number of gap bytes.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// // No clusters = no gaps
    /// assert_eq!(ctx.calculate_gap_bytes(), 0);
    /// ```
    fn calculate_gap_bytes(&self) -> u64 {
        let offsets = self.detector.cluster_offsets();
        if offsets.len() < 2 {
            return 0;
        }

        let mut gap_bytes = 0u64;
        for i in 0..offsets.len() - 1 {
            let (current_offset, current_size) = offsets[i];
            let (next_offset, _) = offsets[i + 1];

            let expected_next = current_offset.saturating_add(current_size as u64);
            if next_offset > expected_next {
                gap_bytes += next_offset - expected_next;
            }
        }

        gap_bytes
    }

    /// Export telemetry as JSON string (Phase 37)
    ///
    /// Collects all diagnostic metrics from the traversal context and returns
    /// them as a JSON string for analysis. This includes timing, hit/miss rates,
    /// fragmentation scores, and CPU breakdown.
    ///
    /// # Returns
    ///
    /// JSON string containing all telemetry fields.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut ctx = TraversalContext::new();
    /// ctx.record_node_visit();
    /// ctx.record_buffer_hit();
    ///
    /// let telemetry_json = ctx.export_telemetry();
    /// let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json).unwrap();
    /// assert_eq!(telemetry["nodes_visited"], 1);
    /// assert_eq!(telemetry["cluster_hits"], 1);
    /// ```
    pub fn export_telemetry(&self) -> String {
        let telemetry = json!({
            "time_total_ms": self.time_total_ms,
            "nodes_visited": self.nodes_visited,
            "cluster_hits": self.buffer_hits,
            "cluster_misses": self.buffer_misses,
            "overshoot_count": self.overshoot_count,
            "undershoot_count": self.undershoot_count,
            "cluster_buffer_reallocs": self.cluster_buffer_reallocs,
            "l2_cache_hits": self.stats.hits,
            "l2_cache_misses": self.stats.misses,
            "dedupe_ms": self.dedupe_time_ms,
            "sort_ms": self.sort_time_ms,
            "linear_detection_ms": self.detector.time_linear_detection_ms(),
            "contiguity_validation_ms": self.detector.time_contiguity_validation_ms(),
            "chains_detected": self.detector.chain_count(),
            "average_chain_length": self.detector.average_chain_length(),
            "fragmentation_score": self.calculate_fragmentation(),
            "gap_bytes": self.calculate_gap_bytes(),
            "cluster_offsets_count": self.detector.cluster_offsets().len(),
            "combined_hit_rate": self.combined_hit_rate(),
        });

        telemetry.to_string()
    }
}

impl Default for TraversalContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traversal_context_new() {
        let ctx = TraversalContext::new();
        assert!(!ctx.detector.is_linear_confirmed());
        assert_eq!(ctx.buffer.len(), 0);
        assert!(ctx.cache.is_empty());
        assert_eq!(ctx.buffer_hits, 0);
        assert_eq!(ctx.buffer_misses, 0);
        assert_eq!(ctx.stats.hits, 0);
        assert_eq!(ctx.stats.misses, 0);
        assert!(ctx.cluster_buffer.is_none());
        assert!(ctx.cluster_buffer_offsets.is_empty());
        assert!(ctx.node_cluster_index.is_empty());
        // Phase 37 telemetry fields
        assert_eq!(ctx.time_total_ms, 0.0);
        assert_eq!(ctx.nodes_visited, 0);
        assert_eq!(ctx.overshoot_count, 0);
        assert_eq!(ctx.undershoot_count, 0);
        assert_eq!(ctx.cluster_buffer_reallocs, 0);
        assert_eq!(ctx.dedupe_time_ms, 0.0);
        assert_eq!(ctx.sort_time_ms, 0.0);
    }

    #[test]
    fn test_traversal_context_default() {
        let ctx = TraversalContext::default();
        assert!(!ctx.detector.is_linear_confirmed());
        assert_eq!(ctx.buffer.len(), 0);
        assert_eq!(ctx.buffer_hits, 0);
        assert_eq!(ctx.buffer_misses, 0);
        assert!(ctx.cluster_buffer.is_none());
        assert!(ctx.cluster_buffer_offsets.is_empty());
        assert!(ctx.node_cluster_index.is_empty());
    }

    #[test]
    fn test_record_buffer_hit() {
        let mut ctx = TraversalContext::new();
        assert_eq!(ctx.buffer_hits, 0);

        ctx.record_buffer_hit();
        assert_eq!(ctx.buffer_hits, 1);

        ctx.record_buffer_hit();
        ctx.record_buffer_hit();
        assert_eq!(ctx.buffer_hits, 3);
    }

    #[test]
    fn test_record_buffer_miss() {
        let mut ctx = TraversalContext::new();
        assert_eq!(ctx.buffer_misses, 0);

        ctx.record_buffer_miss();
        assert_eq!(ctx.buffer_misses, 1);

        ctx.record_buffer_miss();
        ctx.record_buffer_miss();
        assert_eq!(ctx.buffer_misses, 3);
    }

    #[test]
    fn test_combined_hit_rate_empty() {
        let ctx = TraversalContext::new();
        assert_eq!(ctx.combined_hit_rate(), 0.0);
    }

    #[test]
    fn test_combined_hit_rate_only_buffer() {
        let mut ctx = TraversalContext::new();
        ctx.buffer_hits = 10;
        ctx.buffer_misses = 5;

        // 10 / 15 = 0.666...
        let rate = ctx.combined_hit_rate();
        assert!((rate - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_combined_hit_rate_only_cache() {
        let mut ctx = TraversalContext::new();
        ctx.stats.hits = 8;
        ctx.stats.misses = 2;

        // 8 / 10 = 0.8
        assert_eq!(ctx.combined_hit_rate(), 0.8);
    }

    #[test]
    fn test_combined_hit_rate_both() {
        let mut ctx = TraversalContext::new();
        ctx.buffer_hits = 5;
        ctx.stats.hits = 3;
        ctx.buffer_misses = 2;
        ctx.stats.misses = 1;

        // 8 / 11
        let rate = ctx.combined_hit_rate();
        assert!((rate - 8.0 / 11.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_combined_hit_rate_perfect() {
        let mut ctx = TraversalContext::new();
        ctx.buffer_hits = 10;
        ctx.stats.hits = 5;
        ctx.buffer_misses = 0;
        ctx.stats.misses = 0;

        assert_eq!(ctx.combined_hit_rate(), 1.0);
    }

    #[test]
    fn test_combined_hit_rate_zero() {
        let mut ctx = TraversalContext::new();
        ctx.buffer_hits = 0;
        ctx.stats.hits = 0;
        ctx.buffer_misses = 10;
        ctx.stats.misses = 5;

        assert_eq!(ctx.combined_hit_rate(), 0.0);
    }

    #[test]
    fn test_traversal_context_components_accessible() {
        let mut ctx = TraversalContext::new();

        // Verify all public fields are accessible
        let _ = &ctx.detector;
        let _ = &ctx.buffer;
        let _ = &mut ctx.cache;
        let _ = &ctx.stats;
        let _ = &ctx.buffer_hits;
        let _ = &ctx.buffer_misses;

        // Modify fields
        ctx.buffer_hits = 100;
        ctx.buffer_misses = 50;
        assert_eq!(ctx.buffer_hits, 100);
        assert_eq!(ctx.buffer_misses, 50);
    }

    #[test]
    fn test_traversal_context_new_has_empty_cluster_buffer() {
        let ctx = TraversalContext::new();
        assert!(ctx.cluster_buffer.is_none());
        assert!(ctx.cluster_buffer_offsets.is_empty());
    }

    #[test]
    fn test_clear_cluster_buffer() {
        let mut ctx = TraversalContext::new();

        // Populate buffer
        ctx.cluster_buffer = Some(vec![1, 2, 3, 4]);
        ctx.cluster_buffer_offsets = vec![(100, 4), (104, 4)];
        ctx.node_cluster_index.insert(1, 0);
        ctx.node_cluster_index.insert(2, 1);

        // Clear buffer
        ctx.clear_cluster_buffer();

        // Verify cleared
        assert!(ctx.cluster_buffer.is_none());
        assert!(ctx.cluster_buffer_offsets.is_empty());
        assert!(ctx.node_cluster_index.is_empty());
    }

    #[test]
    fn test_clear_cluster_buffer_idempotent() {
        let mut ctx = TraversalContext::new();

        // First clear (already empty)
        ctx.clear_cluster_buffer();
        assert!(ctx.cluster_buffer.is_none());
        assert!(ctx.cluster_buffer_offsets.is_empty());
        assert!(ctx.node_cluster_index.is_empty());

        // Populate and clear
        ctx.cluster_buffer = Some(vec![1, 2, 3]);
        ctx.cluster_buffer_offsets = vec![(100, 3)];
        ctx.node_cluster_index.insert(1, 0);
        ctx.clear_cluster_buffer();

        // Second clear (already empty again)
        ctx.clear_cluster_buffer();
        assert!(ctx.cluster_buffer.is_none());
        assert!(ctx.cluster_buffer_offsets.is_empty());
        assert!(ctx.node_cluster_index.is_empty());
    }

    #[test]
    fn test_node_cluster_index_field() {
        let mut ctx = TraversalContext::new();

        // Verify field is empty initially
        assert!(ctx.node_cluster_index.is_empty());

        // Insert mapping
        ctx.node_cluster_index.insert(1, 0);
        assert_eq!(ctx.node_cluster_index.get(&1), Some(&0));
        assert_eq!(ctx.node_cluster_index.len(), 1);

        // Insert another mapping
        ctx.node_cluster_index.insert(2, 1);
        assert_eq!(ctx.node_cluster_index.get(&2), Some(&1));
        assert_eq!(ctx.node_cluster_index.len(), 2);

        // Clear buffer clears the mapping
        ctx.clear_cluster_buffer();
        assert!(ctx.node_cluster_index.is_empty());
    }

    #[test]
    fn test_clear_cluster_buffer_clears_mapping() {
        let mut ctx = TraversalContext::new();

        // Populate node_cluster_index with multiple entries
        ctx.node_cluster_index.insert(1, 0);
        ctx.node_cluster_index.insert(2, 1);
        ctx.node_cluster_index.insert(3, 2);
        ctx.node_cluster_index.insert(4, 3);
        assert_eq!(ctx.node_cluster_index.len(), 4);

        // Call clear_cluster_buffer()
        ctx.clear_cluster_buffer();

        // Verify node_cluster_index is cleared
        assert!(ctx.node_cluster_index.is_empty());
        assert_eq!(ctx.node_cluster_index.len(), 0);
    }

    #[test]
    fn test_get_cluster_info_helper_exists() {
        // Test that get_cluster_info helper method exists and is callable
        let ctx = TraversalContext::new();

        // The helper method should be accessible
        // We can't test the actual functionality without a graph file,
        // but we can verify the method exists by checking it compiles
        // This test passes if the code compiles successfully
        let _method_exists =TraversalContext::get_cluster_info;
        let _ = &ctx; // Use ctx to avoid unused variable warning
        let _ = _method_exists; // Use to avoid unused variable warning
    }
}

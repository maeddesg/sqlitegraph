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
    }

    #[test]
    fn test_traversal_context_default() {
        let ctx = TraversalContext::default();
        assert!(!ctx.detector.is_linear_confirmed());
        assert_eq!(ctx.buffer.len(), 0);
        assert_eq!(ctx.buffer_hits, 0);
        assert_eq!(ctx.buffer_misses, 0);
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
}

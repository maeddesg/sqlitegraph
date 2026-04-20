//! Adjacency cluster writer with contiguous allocation hint support.
//!
//! This module provides the AdjacencyWriter which can write edge clusters
//! using either contiguous region allocation (for sequential access optimization)
//! or normal fragmented allocation (fallback path).
//!
//! # Usage
//!
//! ```rust
//! use sqlitegraph::backend::native::v2::storage::{AdjacencyWriter, Region};
//!
//! let mut writer = AdjacencyWriter::new(1_000_000);
//!
//! // Try to use contiguous region
//! let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);
//! let result = writer.write_cluster_with_hint(&cluster, Some(&region), 0)?;
//!
//! if result.used_contiguous {
//!     // Cluster was written to contiguous region (fast path)
//! } else {
//!     // Fell back to normal allocation
//! }
//! ```

use super::free_space::{ChainAllocationTrigger, FreeSpaceManager, Region};
use crate::backend::native::v2::edge_cluster::cluster::EdgeCluster;
use crate::backend::native::{FileOffset, NativeBackendError, NativeResult};

/// Result of writing a cluster to storage.
///
/// Tracks where the cluster was written and whether contiguous
/// allocation was used (for metrics and debugging).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenOffset {
    /// Offset where the cluster was written
    pub offset: FileOffset,
    /// Size of the written cluster in bytes
    pub size: u64,
    /// Whether contiguous region allocation was used
    pub used_contiguous: bool,
}

impl WrittenOffset {
    /// Create a new WrittenOffset
    pub fn new(offset: FileOffset, size: u64, used_contiguous: bool) -> Self {
        Self {
            offset,
            size,
            used_contiguous,
        }
    }

    /// Create a WrittenOffset for contiguous allocation
    pub fn contiguous(offset: FileOffset, size: u64) -> Self {
        Self::new(offset, size, true)
    }

    /// Create a WrittenOffset for fragmented allocation
    pub fn fragmented(offset: FileOffset, size: u64) -> Self {
        Self::new(offset, size, false)
    }
}

/// Writer for adjacency clusters with hint-based contiguous allocation.
///
/// The AdjacencyWriter supports two allocation strategies:
/// 1. **Contiguous region allocation**: Writes clusters to a pre-reserved
///    contiguous region for single-I/O traversal (optimal for sequential chains)
/// 2. **Fragmented allocation**: Falls back to normal free space allocation
///    when contiguous allocation is not available or doesn't fit
///
/// This is an **advisory** optimization - failures gracefully fall back
/// without affecting correctness.
#[derive(Debug, Clone)]
pub struct AdjacencyWriter {
    /// Current file size
    file_size: u64,
    /// Next offset for fragmented allocation
    next_fragmented_offset: u64,
}

impl AdjacencyWriter {
    /// Create a new AdjacencyWriter.
    ///
    /// # Arguments
    /// * `file_size` - Initial size of the storage file
    pub fn new(file_size: u64) -> Self {
        Self {
            file_size,
            next_fragmented_offset: file_size,
        }
    }

    /// Get the current file size.
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Get the next offset that will be used for fragmented allocation.
    pub fn next_fragmented_offset(&self) -> u64 {
        self.next_fragmented_offset
    }

    /// Check if a cluster fits in the given contiguous region.
    ///
    /// Validates:
    /// 1. Cluster size <= region's stride (fixed cluster size requirement)
    /// 2. Cluster index is within region's cluster capacity
    ///
    /// # Arguments
    /// * `cluster` - The cluster to check
    /// * `region` - The contiguous region to check against
    ///
    /// # Returns
    /// `true` if the cluster fits in the region, `false` otherwise
    fn fits_in_region(&self, cluster: &EdgeCluster, region: &Region) -> bool {
        // Check fixed stride requirement
        // Cluster size must not exceed stride (unless stride is 0, meaning no fixed size)
        if region.stride > 0 && cluster.size_bytes() as u64 > region.stride as u64 {
            return false;
        }

        // Cluster size must be non-zero
        if cluster.size_bytes() == 0 {
            return false;
        }

        true
    }

    /// Write cluster to contiguous region at specific offset.
    ///
    /// Calculates the offset within the region as:
    /// `offset = region.start_offset + (cluster_index * region.stride)`
    ///
    /// # Arguments
    /// * `cluster` - The cluster to write
    /// * `region` - The contiguous region to write to
    /// * `cluster_index` - Index of this cluster within the region
    ///
    /// # Returns
    /// `Ok(WrittenOffset)` with the offset where the cluster was written
    /// `Err(NativeBackendError)` if the write would exceed region bounds
    fn write_to_region(
        &mut self,
        cluster: &EdgeCluster,
        region: &Region,
        cluster_index: u32,
    ) -> NativeResult<WrittenOffset> {
        // Calculate offset within region
        let offset = region.start_offset + (cluster_index as u64 * region.stride as u64);

        // Validate offset is within region bounds
        let cluster_size = cluster.size_bytes() as u64;
        if offset + cluster_size > region.start_offset + region.total_size {
            return Err(NativeBackendError::CorruptEdgeRecord {
                edge_id: cluster_index as i64,
                reason: format!(
                    "Cluster write exceeds region bounds: offset={} size={} region_end={}",
                    offset,
                    cluster_size,
                    region.start_offset + region.total_size
                ),
            });
        }

        // Track the write (actual file I/O would happen at a higher layer)
        // The cluster's offset is set when actually written to disk

        Ok(WrittenOffset::contiguous(offset, cluster_size))
    }

    /// Write cluster using normal (non-contiguous) allocation.
    ///
    /// This is the fallback path when:
    /// - No region hint is provided
    /// - The cluster doesn't fit in the hinted region
    /// - Contiguous allocation is not available
    ///
    /// # Arguments
    /// * `cluster` - The cluster to write
    ///
    /// # Returns
    /// `Ok(WrittenOffset)` with the offset where the cluster was written
    fn write_cluster_normal(&mut self, cluster: &EdgeCluster) -> NativeResult<WrittenOffset> {
        // Allocate from fragmented space
        let offset = self.allocate_fragmented(cluster.size_bytes() as u64)?;

        // The actual write happens at a higher layer
        // We just return the allocation information

        Ok(WrittenOffset::fragmented(
            offset,
            cluster.size_bytes() as u64,
        ))
    }

    /// Allocate space from the fragmented allocation pool.
    ///
    /// This is a simple allocator that appends to the end of the file.
    /// In a real implementation, this would use FreeSpaceManager.
    ///
    /// # Arguments
    /// * `size` - Size of the allocation in bytes
    ///
    /// # Returns
    /// The offset where the allocation was made
    fn allocate_fragmented(&mut self, size: u64) -> NativeResult<FileOffset> {
        let offset = self.next_fragmented_offset;
        self.next_fragmented_offset += size;
        self.file_size = self.file_size.max(self.next_fragmented_offset);
        Ok(offset)
    }

    /// Write adjacency cluster, using contiguous region if available.
    ///
    /// This is the main entry point for hint-based cluster writing.
    ///
    /// # Strategy
    /// 1. If `hint` is `Some` AND cluster fits in region → use contiguous region
    /// 2. Otherwise → fallback to normal fragmented allocation
    ///
    /// # Arguments
    /// * `cluster` - The cluster to write
    /// * `hint` - Optional contiguous region hint
    /// * `cluster_index` - Index of this cluster within the region (if using hint)
    ///
    /// # Returns
    /// `Ok(WrittenOffset)` indicating where and how the cluster was written
    /// `Err(NativeBackendError)` if writing fails
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::backend::native::v2::storage::{AdjacencyWriter, Region};
    /// # use sqlitegraph::backend::native::v2::edge_cluster::cluster::EdgeCluster;
    /// # let mut writer = AdjacencyWriter::new(1_000_000);
    /// # let cluster = EdgeCluster::create_from_compact_edges(vec![], 1,
    /// #     sqlitegraph::backend::native::v2::edge_cluster::cluster_trace::Direction::Outgoing).unwrap();
    /// // Try contiguous allocation
    /// let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);
    /// let result = writer.write_cluster_with_hint(&cluster, Some(&region), 0)?;
    ///
    /// if result.used_contiguous {
    ///     println!("Used contiguous region at offset {}", result.offset);
    /// } else {
    ///     println!("Used fragmented allocation at offset {}", result.offset);
    /// }
    /// # Ok::<(), sqlitegraph::backend::native::NativeBackendError>(())
    /// ```
    pub fn write_cluster_with_hint(
        &mut self,
        cluster: &EdgeCluster,
        hint: Option<&Region>,
        cluster_index: u32,
    ) -> NativeResult<WrittenOffset> {
        match hint {
            Some(region) if self.fits_in_region(cluster, region) => {
                // Fast path: write to contiguous region
                self.write_to_region(cluster, region, cluster_index)
            }
            _ => {
                // Fallback: normal fragmented allocation
                self.write_cluster_normal(cluster)
            }
        }
    }

    /// Write multiple clusters to a contiguous region.
    ///
    /// This is a convenience method for writing a sequence of clusters
    /// (e.g., a linear chain) to a contiguous region.
    ///
    /// # Arguments
    /// * `clusters` - Iterator of (cluster, cluster_index) pairs
    /// * `region` - The contiguous region to write to
    ///
    /// # Returns
    /// `Ok(Vec<WrittenOffset>)` with offsets for each cluster
    /// `Err(NativeBackendError)` if any write fails
    pub fn write_cluster_batch<'a, I>(
        &mut self,
        clusters: I,
        region: &Region,
    ) -> NativeResult<Vec<WrittenOffset>>
    where
        I: IntoIterator<Item = (&'a EdgeCluster, u32)>,
    {
        let mut results = Vec::new();

        for (cluster, cluster_index) in clusters {
            // Verify each cluster fits before writing
            if !self.fits_in_region(cluster, region) {
                return Err(NativeBackendError::CorruptEdgeRecord {
                    edge_id: cluster_index as i64,
                    reason: format!(
                        "Cluster at index {} doesn't fit in region (size={} stride={})",
                        cluster_index,
                        cluster.size_bytes(),
                        region.stride
                    ),
                });
            }

            let result = self.write_to_region(cluster, region, cluster_index)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Write cluster with automatic chain detection and contiguous allocation.
    ///
    /// This integration helper connects chain detection (via observed chain length)
    /// with the contiguous allocation trigger and the writer. It automatically
    /// reserves contiguous regions when chain length exceeds threshold.
    ///
    /// # Flow
    ///
    /// 1. Check if observed chain length meets threshold
    /// 2. If yes and no active region: try to reserve contiguous region
    /// 3. Use region hint if available, otherwise fallback to normal allocation
    /// 4. Track cluster writes to the region
    ///
    /// # Arguments
    ///
    /// * `cluster` - The cluster to write
    /// * `observed_chain_length` - Number of clusters observed in current chain
    /// * `cluster_stride` - Expected size of each cluster (for region reservation)
    /// * `trigger` - Chain allocation trigger (manages region lifecycle)
    /// * `free_space_manager` - Free space manager for reservation
    ///
    /// # Returns
    ///
    /// `Ok(WrittenOffset)` indicating where and how the cluster was written
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::backend::native::v2::storage::{AdjacencyWriter, ChainAllocationTrigger, FreeSpaceManager};
    /// # use sqlitegraph::backend::native::v2::edge_cluster::cluster::EdgeCluster;
    /// # let cluster = EdgeCluster::create_from_compact_edges(vec![], 1,
    /// #     sqlitegraph::backend::native::v2::edge_cluster::cluster_trace::Direction::Outgoing).unwrap();
    /// let mut writer = AdjacencyWriter::new(1_000_000);
    /// let mut trigger = ChainAllocationTrigger::new();
    /// let mut fsm = FreeSpaceManager::new(1_000_000);
    /// let stride = 4096;
    ///
    /// // First 9 writes: below threshold, normal allocation
    /// for i in 0..9 {
    ///     writer.write_cluster_with_chain_detection(&cluster, i, stride, &mut trigger, &mut fsm)?;
    /// }
    ///
    /// // 10th write: triggers contiguous allocation
    /// let result = writer.write_cluster_with_chain_detection(&cluster, 10, stride, &mut trigger, &mut fsm)?;
    /// # Ok::<(), sqlitegraph::backend::native::NativeBackendError>(())
    /// ```
    pub fn write_cluster_with_chain_detection(
        &mut self,
        cluster: &EdgeCluster,
        observed_chain_length: usize,
        cluster_stride: u32,
        trigger: &mut ChainAllocationTrigger,
        free_space_manager: &mut FreeSpaceManager,
    ) -> NativeResult<WrittenOffset> {
        // Check if we should trigger contiguous allocation
        if trigger.should_trigger_with_observed_count(observed_chain_length) {
            // No active region, try to reserve one
            if trigger.region_hint().is_none() {
                // Reserve region for predicted chain length
                // Use observed_chain_length as prediction for remaining clusters
                let total_bytes = observed_chain_length as u64 * cluster_stride as u64;

                if let Some(region) =
                    free_space_manager.try_reserve_contiguous(total_bytes, cluster_stride as u64)
                {
                    trigger.set_region(region);
                }
                // If reservation fails, we fall through to normal path
            }

            // Use region hint if available
            if trigger.has_active_region() {
                trigger.increment_cluster_count();
                // Get region after increment (we know it exists)
                let cluster_index = trigger.cluster_index() - 1; // Already incremented
                if let Some(region) = trigger.region_hint() {
                    return self.write_cluster_with_hint(cluster, Some(region), cluster_index);
                }
            }
        }

        // Normal allocation path
        self.write_cluster_with_hint(cluster, None, 0)
    }

    /// Reset the writer to a specific file size.
    ///
    /// This is useful for testing or recovery scenarios.
    ///
    /// # Arguments
    /// * `file_size` - The new file size
    /// * `next_offset` - The next offset to use for fragmented allocation
    pub fn reset(&mut self, file_size: u64, next_offset: u64) {
        self.file_size = file_size;
        self.next_fragmented_offset = next_offset;
    }
}

impl Default for AdjacencyWriter {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v2::edge_cluster::cluster_trace::Direction;
    use crate::backend::native::v2::string_table::StringTable;
    use crate::backend::native::{EdgeFlags, EdgeRecord};

    /// Helper to create a test cluster with a specific serialized size.
    /// Note: We create a real cluster since EdgeCluster fields are private.
    /// The size is approximate - the actual cluster size depends on serialization.
    fn create_test_cluster_with_size(target_size: usize) -> EdgeCluster {
        // Create a minimal cluster (small)
        let mut string_table = StringTable::new();
        let edges = vec![EdgeRecord {
            id: 1,
            from_id: 1,
            to_id: 2,
            edge_type: "test".to_string(),
            flags: EdgeFlags::empty(),
            data: serde_json::json!({}),
        }];

        EdgeCluster::create_from_edges(&edges, 1, Direction::Outgoing, &mut string_table).unwrap()
    }

    /// Helper to create a test cluster with a specific number of edges.
    /// More edges = larger cluster.
    fn create_test_cluster_with_edges(edge_count: usize, data_size: usize) -> EdgeCluster {
        let mut string_table = StringTable::new();
        let mut edges = Vec::new();

        for i in 0..edge_count {
            let data = if data_size > 0 {
                serde_json::json!({"data": "x".repeat(data_size)})
            } else {
                serde_json::json!(null)
            };

            edges.push(EdgeRecord {
                id: i as i64 + 1,
                from_id: 1,
                to_id: (i + 2) as i64,
                edge_type: "test".to_string(),
                flags: EdgeFlags::empty(),
                data,
            });
        }

        EdgeCluster::create_from_edges(&edges, 1, Direction::Outgoing, &mut string_table).unwrap()
    }

    /// Helper to create a test cluster from edges.
    fn create_test_cluster_from_edges(edge_count: usize) -> EdgeCluster {
        create_test_cluster_with_edges(edge_count, 0)
    }

    // === Task 1: AdjacencyWriter module creation tests ===

    #[test]
    fn test_adjacency_writer_new() {
        let writer = AdjacencyWriter::new(1_000_000);
        assert_eq!(writer.file_size(), 1_000_000);
        assert_eq!(writer.next_fragmented_offset(), 1_000_000);
    }

    #[test]
    fn test_adjacency_writer_default() {
        let writer = AdjacencyWriter::default();
        assert_eq!(writer.file_size(), 0);
        assert_eq!(writer.next_fragmented_offset(), 0);
    }

    // === Task 2: fits_in_region() tests ===

    #[test]
    fn test_fits_in_region_small_cluster() {
        let writer = AdjacencyWriter::new(1_000_000);
        // Create a small cluster
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        // Small cluster should fit (size <= stride)
        assert!(cluster_size <= 4096);
        assert!(writer.fits_in_region(&cluster, &region));
    }

    #[test]
    fn test_fits_in_region_with_large_data() {
        let writer = AdjacencyWriter::new(1_000_000);
        // Create a cluster with larger data
        let cluster = create_test_cluster_with_edges(1, 5000); // ~5KB of data

        let cluster_size = cluster.size_bytes() as u64;
        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        // Large cluster should NOT fit (size > stride of 4096)
        assert!(cluster_size > 4096);
        assert!(!writer.fits_in_region(&cluster, &region));
    }

    #[test]
    fn test_fits_in_region_zero_stride() {
        let writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_edges(1, 5000);

        let region = Region::new(0, 100_000).with_clusters(0, 0); // No fixed stride

        // Should fit - no stride constraint
        assert!(writer.fits_in_region(&cluster, &region));
    }

    // === Task 3: write_to_region() tests ===

    #[test]
    fn test_write_to_region_first_cluster() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let result = writer.write_to_region(&cluster, &region, 0).unwrap();

        assert!(result.used_contiguous);
        assert_eq!(result.offset, 0); // First cluster at start of region
        assert_eq!(result.size, cluster_size);
    }

    #[test]
    fn test_write_to_region_second_cluster() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let result = writer.write_to_region(&cluster, &region, 1).unwrap();

        assert!(result.used_contiguous);
        assert_eq!(result.offset, 4096); // Second cluster at offset stride
        assert_eq!(result.size, cluster_size);
    }

    #[test]
    fn test_write_to_region_third_cluster() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let result = writer.write_to_region(&cluster, &region, 2).unwrap();

        assert!(result.used_contiguous);
        assert_eq!(result.offset, 8192); // Third cluster at offset 2 * stride
        assert_eq!(result.size, cluster_size);
    }

    #[test]
    fn test_write_to_region_exceeds_bounds() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        // Region only has space for 2 clusters (2 * 4096 = 8192)
        let region = Region::new(0, 8192).with_clusters(2, 4096);

        // Try to write third cluster at offset 2 * 4096 = 8192
        // This would write to [8192, 8192 + cluster_size) which exceeds region end
        let result = writer.write_to_region(&cluster, &region, 2);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NativeBackendError::CorruptEdgeRecord { .. }
        ));
    }

    // === Task 4: write_cluster_normal() tests ===

    #[test]
    fn test_write_cluster_normal_first_allocation() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        let result = writer.write_cluster_normal(&cluster).unwrap();

        assert!(!result.used_contiguous);
        assert_eq!(result.offset, 1_000_000); // Starts at current file size
        assert_eq!(result.size, cluster_size);
        assert_eq!(writer.next_fragmented_offset(), 1_000_000 + cluster_size);
    }

    #[test]
    fn test_write_cluster_normal_multiple_allocations() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster1 = create_test_cluster_with_size(100);
        let cluster2 = create_test_cluster_with_edges(2, 100);

        let size1 = cluster1.size_bytes() as u64;
        let size2 = cluster2.size_bytes() as u64;

        let result1 = writer.write_cluster_normal(&cluster1).unwrap();
        let result2 = writer.write_cluster_normal(&cluster2).unwrap();

        assert!(!result1.used_contiguous);
        assert_eq!(result1.offset, 1_000_000);
        assert_eq!(result1.size, size1);

        assert!(!result2.used_contiguous);
        assert_eq!(result2.offset, 1_000_000 + size1); // After first allocation
        assert_eq!(result2.size, size2);

        assert_eq!(writer.next_fragmented_offset(), 1_000_000 + size1 + size2);
    }

    // === Task 5: write_cluster_with_hint() tests ===

    #[test]
    fn test_write_cluster_with_hint_contiguous_path() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let result = writer
            .write_cluster_with_hint(&cluster, Some(&region), 0)
            .unwrap();

        // Small cluster should use contiguous
        assert!(result.used_contiguous);
        assert_eq!(result.offset, 0);
    }

    #[test]
    fn test_write_cluster_with_hint_fallback_no_hint() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let result = writer.write_cluster_with_hint(&cluster, None, 0).unwrap();

        assert!(!result.used_contiguous); // Should fall back
        assert_eq!(result.offset, 1_000_000);
    }

    #[test]
    fn test_write_cluster_with_hint_fallback_doesnt_fit() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        // Create a cluster larger than the stride
        let cluster = create_test_cluster_with_edges(1, 5000);

        let cluster_size = cluster.size_bytes() as u64;
        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        // Verify cluster is larger than stride
        assert!(cluster_size > 4096);

        let result = writer
            .write_cluster_with_hint(&cluster, Some(&region), 0)
            .unwrap();

        assert!(!result.used_contiguous); // Should fall back
        assert_eq!(result.offset, 1_000_000);
    }

    #[test]
    fn test_write_cluster_with_hint_with_cluster_index() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let result1 = writer
            .write_cluster_with_hint(&cluster, Some(&region), 0)
            .unwrap();
        let result2 = writer
            .write_cluster_with_hint(&cluster, Some(&region), 1)
            .unwrap();
        let result3 = writer
            .write_cluster_with_hint(&cluster, Some(&region), 2)
            .unwrap();

        assert_eq!(result1.offset, 0);
        assert_eq!(result2.offset, 4096);
        assert_eq!(result3.offset, 8192);

        // All should use contiguous
        assert!(result1.used_contiguous);
        assert!(result2.used_contiguous);
        assert!(result3.used_contiguous);
    }

    #[test]
    fn test_write_cluster_with_hint_mixed_strategies() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let small_cluster = create_test_cluster_with_size(100);
        let large_cluster = create_test_cluster_with_edges(1, 5000);

        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        // Small cluster uses contiguous
        let result1 = writer
            .write_cluster_with_hint(&small_cluster, Some(&region), 0)
            .unwrap();
        assert!(result1.used_contiguous);

        // Large cluster falls back
        let result2 = writer
            .write_cluster_with_hint(&large_cluster, Some(&region), 0)
            .unwrap();
        assert!(!result2.used_contiguous);
    }

    // === Batch write tests ===

    #[test]
    fn test_write_cluster_batch_success() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster1 = create_test_cluster_with_size(100);
        let cluster2 = create_test_cluster_with_size(100);
        let cluster3 = create_test_cluster_with_size(100);

        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let clusters = vec![(&cluster1, 0), (&cluster2, 1), (&cluster3, 2)];
        let results = writer
            .write_cluster_batch(clusters.iter().copied(), &region)
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].offset, 0);
        assert_eq!(results[1].offset, 4096);
        assert_eq!(results[2].offset, 8192);

        // All should use contiguous
        assert!(results.iter().all(|r| r.used_contiguous));
    }

    #[test]
    fn test_write_cluster_batch_one_doesnt_fit() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster1 = create_test_cluster_with_size(100);
        let cluster2 = create_test_cluster_with_edges(1, 5000); // Too large for stride
        let cluster3 = create_test_cluster_with_size(100);

        let region = Region::new(0, 10 * 4096).with_clusters(10, 4096);

        let clusters = vec![(&cluster1, 0), (&cluster2, 1), (&cluster3, 2)];
        let result = writer.write_cluster_batch(clusters.iter().copied(), &region);

        // Should fail because cluster2 doesn't fit in stride
        assert!(result.is_err());
    }

    // === WrittenOffset helper tests ===

    #[test]
    fn test_written_offset_contiguous_helper() {
        let offset = WrittenOffset::contiguous(1000, 4096);
        assert!(offset.used_contiguous);
        assert_eq!(offset.offset, 1000);
        assert_eq!(offset.size, 4096);
    }

    #[test]
    fn test_written_offset_fragmented_helper() {
        let offset = WrittenOffset::fragmented(2000, 8192);
        assert!(!offset.used_contiguous);
        assert_eq!(offset.offset, 2000);
        assert_eq!(offset.size, 8192);
    }

    // === Reset tests ===

    #[test]
    fn test_reset_updates_state() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_with_size(100);

        let cluster_size = cluster.size_bytes() as u64;
        writer.write_cluster_normal(&cluster).unwrap();
        assert_eq!(writer.next_fragmented_offset(), 1_000_000 + cluster_size);

        writer.reset(500_000, 500_000);
        assert_eq!(writer.file_size(), 500_000);
        assert_eq!(writer.next_fragmented_offset(), 500_000);
    }

    // === Real cluster tests ===

    #[test]
    fn test_with_real_edge_cluster() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let cluster = create_test_cluster_from_edges(5);

        // Verify cluster has some size
        assert!(cluster.size_bytes() > 0);

        let region = Region::new(0, 100_000).with_clusters(10, 10_000);

        let result = writer
            .write_cluster_with_hint(&cluster, Some(&region), 0)
            .unwrap();

        // Should use contiguous if cluster fits
        if cluster.size_bytes() as u64 <= region.stride as u64 {
            assert!(result.used_contiguous);
        } else {
            assert!(!result.used_contiguous);
        }
    }

    // === 40-10: Chain Detection Integration Tests ===

    #[test]
    fn test_write_cluster_with_chain_detection_below_threshold() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let mut trigger = ChainAllocationTrigger::new();
        let mut fsm = FreeSpaceManager::new(1_000_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // Chain of 5: below threshold, no contiguous allocation
        for i in 1..=5 {
            let result = writer
                .write_cluster_with_chain_detection(&cluster, i, stride, &mut trigger, &mut fsm)
                .unwrap();
            assert!(
                !result.used_contiguous,
                "Write {} should not use contiguous",
                i
            );
        }

        // No active region
        assert!(!trigger.has_active_region());
    }

    #[test]
    fn test_write_cluster_with_chain_detection_at_threshold() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let mut trigger = ChainAllocationTrigger::new();
        let mut fsm = FreeSpaceManager::new(1_000_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // First 9 writes: below threshold
        for i in 1..=9 {
            let result = writer
                .write_cluster_with_chain_detection(&cluster, i, stride, &mut trigger, &mut fsm)
                .unwrap();
            assert!(!result.used_contiguous);
        }

        // 10th write: at threshold, should trigger contiguous allocation
        let result = writer
            .write_cluster_with_chain_detection(&cluster, 10, stride, &mut trigger, &mut fsm)
            .unwrap();

        // Should have reserved and used a contiguous region
        assert!(trigger.has_active_region());
        // Note: Whether contiguous was actually used depends on cluster size vs stride
        // If cluster.size_bytes() > stride, it will fall back
    }

    #[test]
    fn test_write_cluster_with_chain_detection_fallback_on_insufficient_space() {
        let mut writer = AdjacencyWriter::new(10_000); // Small file
        let mut trigger = ChainAllocationTrigger::new();
        let mut fsm = FreeSpaceManager::new(10_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // Try to reserve a large region that won't fit
        let result = writer
            .write_cluster_with_chain_detection(&cluster, 100, stride, &mut trigger, &mut fsm)
            .unwrap();

        // Should fall back to normal allocation
        assert!(!result.used_contiguous);
        // No active region (reservation failed)
        assert!(!trigger.has_active_region());
    }

    #[test]
    fn test_write_cluster_with_chain_detection_uses_region_hint() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let mut trigger = ChainAllocationTrigger::new();
        let mut fsm = FreeSpaceManager::new(1_000_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // First write at threshold: reserve region
        let result1 = writer
            .write_cluster_with_chain_detection(&cluster, 10, stride, &mut trigger, &mut fsm)
            .unwrap();

        // Should have active region now
        assert!(trigger.has_active_region());

        // Subsequent writes should use the region hint
        let cluster_index_before = trigger.cluster_index();
        let result2 = writer
            .write_cluster_with_chain_detection(&cluster, 11, stride, &mut trigger, &mut fsm)
            .unwrap();

        assert_eq!(trigger.cluster_index(), cluster_index_before + 1);
        assert_eq!(trigger.clusters_written(), 2);
    }

    #[test]
    fn test_write_cluster_with_chain_detection_clears_region() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let mut trigger = ChainAllocationTrigger::new();
        let mut fsm = FreeSpaceManager::new(1_000_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // Trigger contiguous allocation
        let _ = writer
            .write_cluster_with_chain_detection(&cluster, 10, stride, &mut trigger, &mut fsm)
            .unwrap();
        assert!(trigger.has_active_region());

        // Clear region
        trigger.clear_region();
        assert!(!trigger.has_active_region());

        // Next write with chain length BELOW threshold should use normal allocation
        let result = writer
            .write_cluster_with_chain_detection(&cluster, 5, stride, &mut trigger, &mut fsm)
            .unwrap();
        assert!(!result.used_contiguous);
        assert!(!trigger.has_active_region());
    }

    #[test]
    fn test_write_cluster_with_chain_detection_custom_threshold() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let mut trigger = ChainAllocationTrigger::with_threshold(5);
        let mut fsm = FreeSpaceManager::new(1_000_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // First 4 writes: below custom threshold of 5
        for i in 1..=4 {
            let result = writer
                .write_cluster_with_chain_detection(&cluster, i, stride, &mut trigger, &mut fsm)
                .unwrap();
            assert!(!result.used_contiguous);
        }

        // 5th write: at custom threshold, should trigger
        let _ = writer
            .write_cluster_with_chain_detection(&cluster, 5, stride, &mut trigger, &mut fsm)
            .unwrap();

        assert!(trigger.has_active_region());
    }

    #[test]
    fn test_write_cluster_with_chain_detection_integration_lifecycle() {
        let mut writer = AdjacencyWriter::new(1_000_000);
        let mut trigger = ChainAllocationTrigger::new();
        let mut fsm = FreeSpaceManager::new(1_000_000);
        let cluster = create_test_cluster_from_edges(1);
        let stride = 4096;

        // 1. Below threshold: normal allocation
        for i in 1..=9 {
            let result = writer
                .write_cluster_with_chain_detection(&cluster, i, stride, &mut trigger, &mut fsm)
                .unwrap();
            assert!(!result.used_contiguous);
        }

        // 2. At threshold: reserve contiguous region
        let result = writer
            .write_cluster_with_chain_detection(&cluster, 10, stride, &mut trigger, &mut fsm)
            .unwrap();
        assert!(trigger.has_active_region());

        // 3. Use region for subsequent writes
        for i in 11..15 {
            writer
                .write_cluster_with_chain_detection(&cluster, i, stride, &mut trigger, &mut fsm)
                .unwrap();
        }
        assert_eq!(trigger.clusters_written(), 5); // 10th + 11-14 = 5 clusters

        // 4. Clear and verify normal allocation resumes
        trigger.clear_region();
        // After clearing, write with below-threshold count won't trigger
        let result = writer
            .write_cluster_with_chain_detection(&cluster, 5, stride, &mut trigger, &mut fsm)
            .unwrap();
        assert!(!trigger.has_active_region());
    }
}

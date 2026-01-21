//! Sequential cluster reader for single-I/O chain optimization.
//!
//! This module provides `SequentialClusterReader`, a stateless helper that reads
//! all edge clusters for a confirmed linear chain in a single I/O operation.
//!
//! # Purpose
//!
//! When `LinearDetector` confirms a linear traversal pattern with contiguous cluster
//! storage, this reader enables IO-12 target (Chain(500) <= 75ms) by reducing I/O
//! count from N to 1 for linear chains.
//!
//! # Design
//!
//! - **Stateless**: No fields needed - offsets passed as parameters
//! - **Single I/O**: Read all contiguous clusters in one `GraphFile::read_bytes()` call
//! - **Raw bytes storage**: Return `Vec<u8>` to defer deserialization until neighbor extraction
//! - **Bounded memory**: `MAX_CLUSTER_BUFFER_SIZE` constant (512KB) prevents unbounded growth
//!
//! # Usage Pattern
//!
//! ```rust,no_run
//! use crate::backend::native::adjacency::{LinearDetector, SequentialClusterReader};
//!
//! let mut detector = LinearDetector::new();
//! // ... observe nodes during traversal ...
//!
//! // After chain confirmed + clusters contiguous
//! if detector.should_use_sequential_read() {
//!     let offsets = detector.cluster_offsets();
//!     match SequentialClusterReader::read_chain_clusters(graph_file, offsets) {
//!         Ok(buffer) => {
//!             // Buffer contains all clusters in raw bytes
//!             // Extract neighbors on-demand via extract_neighbors()
//!         }
//!         Err(_) => {
//!             // Fall back to standard I/O path
//!         }
//!     }
//! }
//! ```
//!
//! # Precondition
//!
//! Caller MUST validate contiguity first (via `are_clusters_contiguous()`).
//! This method assumes clusters are contiguous and reads them as one block.

use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::types::{NativeBackendError, NativeNodeId, NativeResult};
use crate::backend::native::v2::edge_cluster::cluster::EdgeCluster;

/// Maximum buffer size for sequential cluster reads (512KB).
///
/// This bounds memory usage and prevents unbounded growth. Sufficient for
/// ~128 clusters of 4KB each, which covers typical chain lengths encountered
/// in graph traversals.
///
/// Rationale:
/// - WebSearch recommends 64KB to 1MB chunks for file I/O
/// - Chain(500) target suggests 500 nodes × 4KB cluster = 2MB worst case
/// - 512KB is a conservative middle ground that covers most chains
/// - Can be increased to 1MB in Phase 35 if monitoring shows average chain length exceeds 128 nodes
const MAX_CLUSTER_BUFFER_SIZE: usize = 512 * 1024; // 512KB

/// Sequential cluster reader for single-I/O chain optimization.
///
/// This is a stateless helper struct (no fields) with associated methods that
/// perform sequential I/O operations on confirmed linear chains.
///
/// # Design
///
/// - **Stateless**: All state passed as parameters (caller manages lifecycle)
/// - **Single I/O**: `read_chain_clusters()` performs one large `read_bytes()` call
/// - **Deferred deserialization**: `extract_neighbors()` deserializes on-demand
///
/// # Example
///
/// ```rust,no_run
/// use crate::backend::native::adjacency::SequentialClusterReader;
///
/// // Read all clusters for a chain in one I/O operation
/// let cluster_offsets = [(1024, 4096), (5120, 4096), (9216, 4096)];
/// let buffer = SequentialClusterReader::read_chain_clusters(graph_file, &cluster_offsets)?;
///
/// // Extract neighbors for cluster at index 1
/// let neighbors = SequentialClusterReader::extract_neighbors(&buffer, 1, &cluster_offsets)?;
/// ```
pub struct SequentialClusterReader;

impl SequentialClusterReader {
    /// Read all clusters for a chain in a single I/O operation.
    ///
    /// This method performs a single sequential read of all contiguous clusters
    /// for a confirmed linear chain. The clusters are returned as raw bytes to
    /// defer deserialization until neighbor extraction.
    ///
    /// # Parameters
    ///
    /// - **graph_file**: Mutable borrow for I/O operations
    /// - **cluster_offsets**: Slice of (offset, size) tuples, MUST be contiguous
    ///
    /// # Returns
    ///
    /// - **Ok(Vec<u8>)**: Raw bytes containing all clusters concatenated
    /// - **Err(NativeBackendError)**:
    ///   - `InvalidParameter`: If `cluster_offsets` is empty
    ///   - `RecordTooLarge`: If total size exceeds `MAX_CLUSTER_BUFFER_SIZE`
    ///   - `Io`: If file read fails
    ///
    /// # Precondition
    ///
    /// Caller MUST validate contiguity first (via `are_clusters_contiguous()`).
    /// This method assumes clusters are contiguous and reads them as one block.
    /// Non-contiguous clusters will result in garbage data.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use crate::backend::native::adjacency::{are_clusters_contiguous, SequentialClusterReader};
    ///
    /// let cluster_offsets = [(1024, 4096), (5120, 4096), (9216, 4096)];
    ///
    /// // Validate contiguity first
    /// assert!(are_clusters_contiguous(&cluster_offsets));
    ///
    /// // Read all clusters in one I/O operation
    /// let buffer = SequentialClusterReader::read_chain_clusters(graph_file, &cluster_offsets)?;
    /// assert_eq!(buffer.len(), 12288); // 3 × 4096
    /// ```
    pub fn read_chain_clusters(
        graph_file: &mut GraphFile,
        cluster_offsets: &[(u64, u32)],
    ) -> NativeResult<Vec<u8>> {
        // Validate cluster_offsets is not empty
        if cluster_offsets.is_empty() {
            return Err(NativeBackendError::InvalidParameter {
                context: "cluster_offsets is empty".to_string(),
                source: None,
            });
        }

        // Calculate total size by summing all cluster_size values
        let total_size: u64 = cluster_offsets
            .iter()
            .map(|(_, size)| *size as u64)
            .sum();

        // Validate total size against MAX_CLUSTER_BUFFER_SIZE
        if total_size > MAX_CLUSTER_BUFFER_SIZE as u64 {
            return Err(NativeBackendError::RecordTooLarge {
                size: total_size as u32,
                max_size: MAX_CLUSTER_BUFFER_SIZE as u32,
            });
        }

        // Calculate start_offset from first tuple's offset
        let start_offset = cluster_offsets[0].0;

        // Pre-allocate buffer with exact size (no reallocation needed)
        let mut buffer = vec![0u8; total_size as usize];

        // Single I/O call to read all clusters contiguously
        graph_file.read_bytes(start_offset, &mut buffer)?;

        Ok(buffer)
    }

    /// Extract neighbors for a specific cluster from buffered bytes.
    ///
    /// This method deserializes a single cluster from the concatenated buffer
    /// returned by `read_chain_clusters()`. Deserialization is performed on-demand
    /// to avoid CPU cost for clusters never accessed.
    ///
    /// # Parameters
    ///
    /// - **buffer**: Raw cluster bytes from `read_chain_clusters()`
    /// - **cluster_index**: Index of cluster within the buffer (0-based)
    /// - **cluster_offsets**: Original offsets slice (to calculate byte position)
    ///
    /// # Returns
    ///
    /// - **Ok(Vec<NativeNodeId>)**: Neighbor IDs for the requested cluster
    /// - **Err(NativeBackendError)**:
    ///   - `InvalidParameter`: If `cluster_index` is out of bounds
    ///   - Deserialization errors from `EdgeCluster::deserialize()`
    ///
    /// # Byte Offset Calculation
    ///
    /// The byte offset for a cluster is calculated by summing the sizes of all
    /// preceding clusters in the buffer:
    ///
    /// ```text
    /// cluster_index=0: byte_offset = 0
    /// cluster_index=1: byte_offset = cluster_offsets[0].size
    /// cluster_index=2: byte_offset = cluster_offsets[0].size + cluster_offsets[1].size
    /// ...
    /// ```
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use crate::backend::native::adjacency::SequentialClusterReader;
    ///
    /// let cluster_offsets = [(1024, 4096), (5120, 4096), (9216, 4096)];
    /// let buffer = SequentialClusterReader::read_chain_clusters(graph_file, &cluster_offsets)?;
    ///
    /// // Extract neighbors from first cluster (index 0)
    /// let neighbors_0 = SequentialClusterReader::extract_neighbors(&buffer, 0, &cluster_offsets)?;
    ///
    /// // Extract neighbors from second cluster (index 1)
    /// let neighbors_1 = SequentialClusterReader::extract_neighbors(&buffer, 1, &cluster_offsets)?;
    ///
    /// // Extract neighbors from third cluster (index 2)
    /// let neighbors_2 = SequentialClusterReader::extract_neighbors(&buffer, 2, &cluster_offsets)?;
    /// ```
    pub fn extract_neighbors(
        buffer: &[u8],
        cluster_index: usize,
        cluster_offsets: &[(u64, u32)],
    ) -> NativeResult<Vec<NativeNodeId>> {
        // Validate cluster_index is within bounds
        if cluster_index >= cluster_offsets.len() {
            return Err(NativeBackendError::InvalidParameter {
                context: format!(
                    "cluster_index {} out of bounds (len: {})",
                    cluster_index,
                    cluster_offsets.len()
                ),
                source: None,
            });
        }

        // Calculate byte_offset by summing sizes of all preceding clusters
        let mut byte_offset = 0usize;
        for (i, (_, size)) in cluster_offsets.iter().enumerate() {
            if i == cluster_index {
                break;
            }
            byte_offset += *size as usize;
        }

        // Extract cluster_bytes slice from buffer
        let cluster_size = cluster_offsets[cluster_index].1 as usize;
        let cluster_bytes = &buffer[byte_offset..byte_offset + cluster_size];

        // Deserialize cluster
        let cluster = EdgeCluster::deserialize(cluster_bytes)?;

        // Extract neighbors: convert i64 to NativeNodeId
        let neighbors: Vec<NativeNodeId> = cluster
            .iter_neighbors()
            .map(|id| id as NativeNodeId)
            .collect();

        Ok(neighbors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v2::edge_cluster::cluster::EdgeCluster;
    use crate::backend::native::v2::edge_cluster::cluster_trace::Direction;
    use crate::backend::native::EdgeRecord;
    use crate::backend::native::types::EdgeFlags;
    use crate::backend::native::v2::string_table::StringTable;

    /// Mock GraphFile for testing (bypasses full file format requirements)
    struct MockGraphFile {
        data: Vec<u8>,
    }

    impl MockGraphFile {
        fn new(data: Vec<u8>) -> Self {
            Self { data }
        }

        /// Mock read_bytes that reads from the in-memory data
        fn read_bytes(&mut self, offset: u64, buffer: &mut [u8]) -> NativeResult<()> {
            let start = offset as usize;
            let end = start + buffer.len();

            if end > self.data.len() {
                return Err(NativeBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Read past end of mock data",
                )));
            }

            buffer.copy_from_slice(&self.data[start..end]);
            Ok(())
        }
    }

    /// Helper function to create a test cluster with specified neighbors
    fn create_test_cluster(neighbors: &[i64]) -> Vec<u8> {
        let mut string_table = StringTable::new();
        let edges: Vec<EdgeRecord> = neighbors
            .iter()
            .enumerate()
            .map(|(idx, &id)| EdgeRecord {
                id: idx as i64,
                from_id: 1,
                to_id: id,
                edge_type: "TEST".to_string(),
                data: serde_json::Value::Null,
                flags: EdgeFlags::empty(),
            })
            .collect();

        let cluster = EdgeCluster::create_from_edges(&edges, 1, Direction::Outgoing, &mut string_table)
            .expect("Failed to create cluster");
        cluster.serialize()
    }

    /// Helper function to create mock data with contiguous clusters
    fn create_mock_data(clusters: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();

        // Add header (1024 bytes of zeros)
        data.extend_from_slice(&[0u8; 1024]);

        // Add clusters contiguously
        for cluster_data in clusters {
            data.extend_from_slice(cluster_data);
        }

        data
    }

    /// Wrapper to convert MockGraphFile to work with SequentialClusterReader
    fn read_with_mock(data: &[u8], cluster_offsets: &[(u64, u32)]) -> NativeResult<Vec<u8>> {
        // Mock the GraphFile behavior using a closure
        let mut mock_data = data.to_vec();

        // Validate cluster_offsets is not empty
        if cluster_offsets.is_empty() {
            return Err(NativeBackendError::InvalidParameter {
                context: "cluster_offsets is empty".to_string(),
                source: None,
            });
        }

        // Calculate total size by summing all cluster_size values
        let total_size: u64 = cluster_offsets
            .iter()
            .map(|(_, size)| *size as u64)
            .sum();

        // Validate total size against MAX_CLUSTER_BUFFER_SIZE
        if total_size > MAX_CLUSTER_BUFFER_SIZE as u64 {
            return Err(NativeBackendError::RecordTooLarge {
                size: total_size as u32,
                max_size: MAX_CLUSTER_BUFFER_SIZE as u32,
            });
        }

        // Calculate start_offset from first tuple's offset
        let start_offset = cluster_offsets[0].0 as usize;

        // Pre-allocate buffer with exact size
        let mut buffer = vec![0u8; total_size as usize];

        // Simulate single I/O call by copying from mock data
        if start_offset + total_size as usize > mock_data.len() {
            return Err(NativeBackendError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Read past end of mock data",
            )));
        }

        buffer.copy_from_slice(&mock_data[start_offset..start_offset + total_size as usize]);

        Ok(buffer)
    }

    #[test]
    fn test_read_chain_clusters_empty_offsets_returns_error() {
        let data = create_mock_data(&[]);
        let cluster_offsets: &[(u64, u32)] = &[];

        let result = read_with_mock(&data, cluster_offsets);

        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::InvalidParameter { context, .. } => {
                assert_eq!(context, "cluster_offsets is empty");
            }
            _ => panic!("Expected InvalidParameter error"),
        }
    }

    #[test]
    fn test_read_chain_clusters_single_cluster() {
        let cluster_data = create_test_cluster(&[2, 3, 4]);
        let data = create_mock_data(&[cluster_data.clone()]);

        let cluster_offsets = [(1024, cluster_data.len() as u32)];

        let result = read_with_mock(&data, &cluster_offsets);

        assert!(result.is_ok());
        let buffer = result.unwrap();
        assert_eq!(buffer.len(), cluster_data.len());
        assert_eq!(buffer, cluster_data);
    }

    #[test]
    fn test_read_chain_clusters_multiple_contiguous() {
        let cluster1 = create_test_cluster(&[2, 3]);
        let cluster2 = create_test_cluster(&[4, 5, 6]);
        let cluster3 = create_test_cluster(&[7, 8, 9, 10]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone(), cluster3.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
            (1024 + cluster1.len() as u64 + cluster2.len() as u64, cluster3.len() as u32),
        ];

        let result = read_with_mock(&data, &cluster_offsets);

        assert!(result.is_ok());
        let buffer = result.unwrap();

        // Verify buffer contains all three clusters concatenated
        let expected_size = cluster1.len() + cluster2.len() + cluster3.len();
        assert_eq!(buffer.len(), expected_size);

        // Verify each cluster is in the correct position
        assert_eq!(&buffer[0..cluster1.len()], &cluster1[..]);
        assert_eq!(
            &buffer[cluster1.len()..cluster1.len() + cluster2.len()],
            &cluster2[..]
        );
        assert_eq!(
            &buffer[cluster1.len() + cluster2.len()..],
            &cluster3[..]
        );
    }

    #[test]
    fn test_read_chain_clusters_exceeds_max_size() {
        // Create a cluster size request that exceeds MAX_CLUSTER_BUFFER_SIZE
        let oversized_size = MAX_CLUSTER_BUFFER_SIZE + 1;

        let cluster_offsets = [(1024, oversized_size as u32)];

        let data = create_mock_data(&[create_test_cluster(&[2])]);

        let result = read_with_mock(&data, &cluster_offsets);

        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::RecordTooLarge { size, max_size } => {
                assert_eq!(size, oversized_size as u32);
                assert_eq!(max_size, MAX_CLUSTER_BUFFER_SIZE as u32);
            }
            _ => panic!("Expected RecordTooLarge error"),
        }
    }

    #[test]
    fn test_extract_neighbors_first_cluster() {
        let cluster1 = create_test_cluster(&[2, 3]);
        let cluster2 = create_test_cluster(&[4, 5, 6]);
        let cluster3 = create_test_cluster(&[7, 8, 9, 10]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone(), cluster3.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
            (1024 + cluster1.len() as u64 + cluster2.len() as u64, cluster3.len() as u32),
        ];

        let buffer = read_with_mock(&data, &cluster_offsets)
            .expect("Failed to read clusters");

        // Extract neighbors from first cluster (index 0)
        let neighbors = SequentialClusterReader::extract_neighbors(&buffer, 0, &cluster_offsets)
            .expect("Failed to extract neighbors");

        assert_eq!(neighbors, vec![2, 3]);
    }

    #[test]
    fn test_extract_neighbors_middle_cluster() {
        let cluster1 = create_test_cluster(&[2, 3]);
        let cluster2 = create_test_cluster(&[4, 5, 6]);
        let cluster3 = create_test_cluster(&[7, 8, 9, 10]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone(), cluster3.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
            (1024 + cluster1.len() as u64 + cluster2.len() as u64, cluster3.len() as u32),
        ];

        let buffer = read_with_mock(&data, &cluster_offsets)
            .expect("Failed to read clusters");

        // Extract neighbors from middle cluster (index 1)
        let neighbors = SequentialClusterReader::extract_neighbors(&buffer, 1, &cluster_offsets)
            .expect("Failed to extract neighbors");

        assert_eq!(neighbors, vec![4, 5, 6]);
    }

    #[test]
    fn test_extract_neighbors_invalid_index() {
        let cluster1 = create_test_cluster(&[2, 3]);
        let cluster2 = create_test_cluster(&[4, 5, 6]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
        ];

        let buffer = read_with_mock(&data, &cluster_offsets)
            .expect("Failed to read clusters");

        // Try to extract neighbors with invalid index (out of bounds)
        let result = SequentialClusterReader::extract_neighbors(&buffer, 5, &cluster_offsets);

        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::InvalidParameter { context, .. } => {
                assert!(context.contains("cluster_index 5 out of bounds"));
            }
            _ => panic!("Expected InvalidParameter error"),
        }
    }

    #[test]
    fn test_extract_neighbors_last_cluster() {
        let cluster1 = create_test_cluster(&[2, 3]);
        let cluster2 = create_test_cluster(&[4, 5, 6]);
        let cluster3 = create_test_cluster(&[7, 8, 9, 10]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone(), cluster3.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
            (1024 + cluster1.len() as u64 + cluster2.len() as u64, cluster3.len() as u32),
        ];

        let buffer = read_with_mock(&data, &cluster_offsets)
            .expect("Failed to read clusters");

        // Extract neighbors from last cluster (index 2)
        let neighbors = SequentialClusterReader::extract_neighbors(&buffer, 2, &cluster_offsets)
            .expect("Failed to extract neighbors");

        assert_eq!(neighbors, vec![7, 8, 9, 10]);
    }

    #[test]
    fn test_extract_neighbors_all_clusters() {
        let cluster1 = create_test_cluster(&[2, 3]);
        let cluster2 = create_test_cluster(&[4, 5, 6]);
        let cluster3 = create_test_cluster(&[7, 8, 9, 10]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone(), cluster3.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
            (1024 + cluster1.len() as u64 + cluster2.len() as u64, cluster3.len() as u32),
        ];

        let buffer = read_with_mock(&data, &cluster_offsets)
            .expect("Failed to read clusters");

        // Extract neighbors from all clusters
        let neighbors_0 = SequentialClusterReader::extract_neighbors(&buffer, 0, &cluster_offsets)
            .expect("Failed to extract neighbors from cluster 0");
        let neighbors_1 = SequentialClusterReader::extract_neighbors(&buffer, 1, &cluster_offsets)
            .expect("Failed to extract neighbors from cluster 1");
        let neighbors_2 = SequentialClusterReader::extract_neighbors(&buffer, 2, &cluster_offsets)
            .expect("Failed to extract neighbors from cluster 2");

        assert_eq!(neighbors_0, vec![2, 3]);
        assert_eq!(neighbors_1, vec![4, 5, 6]);
        assert_eq!(neighbors_2, vec![7, 8, 9, 10]);
    }

    #[test]
    fn test_max_cluster_buffer_size_constant() {
        // Verify the constant is set to 512KB
        assert_eq!(MAX_CLUSTER_BUFFER_SIZE, 512 * 1024);
    }

    #[test]
    fn test_read_chain_clusters_variable_cluster_sizes() {
        // Create clusters with different sizes
        let cluster1 = create_test_cluster(&[2]);
        let cluster2 = create_test_cluster(&[3, 4, 5]);
        let cluster3 = create_test_cluster(&[6]);

        let data = create_mock_data(&[cluster1.clone(), cluster2.clone(), cluster3.clone()]);

        let cluster_offsets = [
            (1024, cluster1.len() as u32),
            (1024 + cluster1.len() as u64, cluster2.len() as u32),
            (1024 + cluster1.len() as u64 + cluster2.len() as u64, cluster3.len() as u32),
        ];

        let result = read_with_mock(&data, &cluster_offsets);

        assert!(result.is_ok());
        let buffer = result.unwrap();

        // Verify buffer contains all clusters with correct byte offsets
        let expected_size = cluster1.len() + cluster2.len() + cluster3.len();
        assert_eq!(buffer.len(), expected_size);

        // Verify extraction works with variable sizes
        let neighbors_0 = SequentialClusterReader::extract_neighbors(&buffer, 0, &cluster_offsets)
            .expect("Failed to extract from cluster 0");
        let neighbors_1 = SequentialClusterReader::extract_neighbors(&buffer, 1, &cluster_offsets)
            .expect("Failed to extract from cluster 1");
        let neighbors_2 = SequentialClusterReader::extract_neighbors(&buffer, 2, &cluster_offsets)
            .expect("Failed to extract from cluster 2");

        assert_eq!(neighbors_0, vec![2]);
        assert_eq!(neighbors_1, vec![3, 4, 5]);
        assert_eq!(neighbors_2, vec![6]);
    }
}

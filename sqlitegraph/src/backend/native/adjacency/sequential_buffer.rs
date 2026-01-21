//! Per-traversal buffer for sequential I/O optimization
//!
//! This module provides a traversal-scoped buffer for decoded node slots
//! from batch reads. The buffer is designed to:
//!
//! - **Evaporate on function return**: Stack-allocated, no explicit cleanup
//! - **Preserve MVCC isolation**: No cross-traversal data sharing
//! - **Prefetch sequentially**: Read 8 slots (32KB) after linear detection
//! - **Cache decoded data**: Avoid re-decoding slots across hops
//!
//! # Usage Pattern
//!
//! ```rust
//! use crate::backend::native::adjacency::{LinearDetector, SequentialReadBuffer};
//!
//! let mut detector = LinearDetector::new();
//! let mut buffer = SequentialReadBuffer::new();
//!
//! // During traversal
//! if detector.is_linear_confirmed() && !buffer.contains(node_id) {
//!     buffer.prefetch_from(graph_file, node_id)?;
//! }
//!
//! if let Some(node) = buffer.get(node_id) {
//!     // Use cached node data
//! }
//! // Buffer evaporates here
//! ```

use ahash::AHashMap;
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::node_store::NodeStore;
use crate::backend::native::types::{NativeNodeId, NativeResult};
use crate::backend::native::v2::node_record_v2::NodeRecordV2;

/// Per-traversal buffer for sequential I/O optimization
///
/// # Design
///
/// - Scoped to single traversal (evaporates when function returns)
/// - Prefetches 8 slots (32KB) after LinearDetector confirms linear pattern
/// - Stores decoded NodeRecordV2 for rapid access without re-decoding
///
/// # MVCC Safety
///
/// Buffer is stack-allocated per traversal. No cross-traversal data sharing
/// means no stale data across transactions.
pub struct SequentialReadBuffer {
    /// Decoded node records from batched reads
    slots: AHashMap<NativeNodeId, NodeRecordV2>,

    /// Prefetch window (default: 8 slots = 32KB)
    prefetch_window: usize,

    /// Next prefetch start ID (for tracking, not stateful across traversals)
    next_prefetch_start: Option<NativeNodeId>,
}

impl SequentialReadBuffer {
    /// Create a new empty buffer with default 8-slot prefetch window
    pub fn new() -> Self {
        Self {
            slots: AHashMap::new(),
            prefetch_window: 8,  // 32KB = 8 * 4096
            next_prefetch_start: None,
        }
    }

    /// Create buffer with custom prefetch window (for testing)
    pub fn with_prefetch_window(prefetch_window: usize) -> Self {
        Self {
            slots: AHashMap::new(),
            prefetch_window,
            next_prefetch_start: None,
        }
    }

    /// Get node from buffer, returns None if not cached
    #[inline]
    pub fn get(&self, node_id: NativeNodeId) -> Option<&NodeRecordV2> {
        self.slots.get(&node_id)
    }

    /// Check if node is in buffer
    #[inline]
    pub fn contains(&self, node_id: NativeNodeId) -> bool {
        self.slots.contains_key(&node_id)
    }

    /// Get number of nodes currently cached
    #[inline]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Check if buffer is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Insert a batch of decoded nodes into buffer
    pub fn insert_batch(&mut self, nodes: Vec<NodeRecordV2>) {
        for node in nodes {
            self.slots.insert(node.id, node);
        }
    }

    /// Insert a single node into buffer
    pub fn insert(&mut self, node: NodeRecordV2) {
        self.slots.insert(node.id, node);
    }

    /// Prefetch sequential slots starting from start_node_id
    ///
    /// Reads `prefetch_window` slots (default 8) using NodeStore::read_slots_batch()
    /// and caches the decoded NodeRecordV2 instances.
    ///
    /// # Parameters
    /// - `graph_file`: Mutable borrow for I/O operations
    /// - `start_node_id`: First node ID to prefetch
    ///
    /// # Errors
    /// Returns error if batch read fails (file I/O, decoding errors)
    pub fn prefetch_from(
        &mut self,
        graph_file: &mut GraphFile,
        start_node_id: NativeNodeId,
    ) -> NativeResult<()> {
        let mut node_store = NodeStore::new(graph_file);
        let nodes = node_store.read_slots_batch(start_node_id, self.prefetch_window)?;

        // Cache decoded nodes
        self.insert_batch(nodes);

        // Track next prefetch start (for potential future extension)
        self.next_prefetch_start = Some(start_node_id + self.prefetch_window as i64);

        Ok(())
    }

    /// Get the next prefetch start ID (for testing/monitoring)
    pub fn next_prefetch_start(&self) -> Option<NativeNodeId> {
        self.next_prefetch_start
    }

    /// Get the current prefetch window size (for testing)
    pub fn prefetch_window(&self) -> usize {
        self.prefetch_window
    }

    /// Clear all cached nodes
    pub fn clear(&mut self) {
        self.slots.clear();
        self.next_prefetch_start = None;
    }
}

impl Default for SequentialReadBuffer {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::NodeFlags;

    #[test]
    fn test_buffer_new() {
        let buffer = SequentialReadBuffer::new();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert!(!buffer.contains(1));
        assert_eq!(buffer.prefetch_window(), 8);
        assert!(buffer.next_prefetch_start().is_none());
    }

    #[test]
    fn test_buffer_default() {
        let buffer = SequentialReadBuffer::default();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(buffer.prefetch_window(), 8);
    }

    #[test]
    fn test_buffer_insert_get() {
        let mut buffer = SequentialReadBuffer::new();
        let node = NodeRecordV2::new(1, "Test".into(), "node1".into(), serde_json::json!({}));

        buffer.insert(node);
        assert!(buffer.contains(1));
        assert_eq!(buffer.len(), 1);
        assert!(!buffer.is_empty());

        let retrieved = buffer.get(1);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, 1);

        // Non-existent node returns None
        assert!(buffer.get(999).is_none());
        assert!(!buffer.contains(999));
    }

    #[test]
    fn test_buffer_insert_batch() {
        let mut buffer = SequentialReadBuffer::new();

        let nodes = vec![
            NodeRecordV2::new(1, "Type1".into(), "node1".into(), serde_json::json!({})),
            NodeRecordV2::new(2, "Type2".into(), "node2".into(), serde_json::json!({})),
            NodeRecordV2::new(3, "Type3".into(), "node3".into(), serde_json::json!({})),
        ];

        buffer.insert_batch(nodes);
        assert_eq!(buffer.len(), 3);
        assert!(buffer.contains(1));
        assert!(buffer.contains(2));
        assert!(buffer.contains(3));
    }

    #[test]
    fn test_buffer_custom_window() {
        let buffer = SequentialReadBuffer::with_prefetch_window(4);
        assert_eq!(buffer.prefetch_window(), 4);
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_buffer_clear() {
        let mut buffer = SequentialReadBuffer::new();
        buffer.insert(NodeRecordV2::new(1, "Test".into(), "node1".into(), serde_json::json!({})));
        buffer.insert(NodeRecordV2::new(2, "Test".into(), "node2".into(), serde_json::json!({})));

        assert_eq!(buffer.len(), 2);

        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert!(!buffer.contains(1));
        assert!(buffer.next_prefetch_start().is_none());
    }

    #[test]
    fn test_buffer_get_returns_reference() {
        let mut buffer = SequentialReadBuffer::new();
        let data = serde_json::json!({"key": "value"});
        let node = NodeRecordV2::new(1, "Test".into(), "node1".into(), data.clone());

        buffer.insert(node);

        let retrieved = buffer.get(1).unwrap();
        assert_eq!(retrieved.id, 1);
        assert_eq!(retrieved.kind, "Test");
        assert_eq!(retrieved.name, "node1");
        assert_eq!(retrieved.data, data);
    }

    #[test]
    fn test_buffer_overwrite() {
        let mut buffer = SequentialReadBuffer::new();

        // Insert first version
        buffer.insert(NodeRecordV2::new(1, "Type1".into(), "node1".into(), serde_json::json!({})));
        assert_eq!(buffer.len(), 1);

        // Overwrite with second version
        buffer.insert(NodeRecordV2::new(1, "Type2".into(), "node1_v2".into(), serde_json::json!({})));
        assert_eq!(buffer.len(), 1); // Still only 1 entry

        let retrieved = buffer.get(1).unwrap();
        assert_eq!(retrieved.kind, "Type2");
        assert_eq!(retrieved.name, "node1_v2");
    }

    #[test]
    fn test_buffer_empty_behavior() {
        let buffer = SequentialReadBuffer::new();

        // Empty buffer returns None for any get()
        assert!(buffer.get(1).is_none());
        assert!(buffer.get(999).is_none());

        // contains() returns false for any node
        assert!(!buffer.contains(1));

        // len() and is_empty() report correctly
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_buffer_mvcc_isolation() {
        // Simulate two traversals by creating separate buffers
        let mut buffer1 = SequentialReadBuffer::new();
        let mut buffer2 = SequentialReadBuffer::new();

        let node1 = NodeRecordV2::new(1, "T".into(), "a".into(), serde_json::json!({}));
        let node2 = NodeRecordV2::new(2, "T".into(), "b".into(), serde_json::json!({}));

        // Insert into buffer1 only
        buffer1.insert(node1);

        // Verify buffer2 doesn't see buffer1's data (MVCC isolation)
        assert!(!buffer2.contains(1));
        assert_eq!(buffer2.len(), 0);

        // Insert into buffer2
        buffer2.insert(node2);

        // Verify buffers are independent
        assert!(buffer1.contains(1));
        assert!(!buffer1.contains(2));
        assert!(!buffer2.contains(1));
        assert!(buffer2.contains(2));
    }
}

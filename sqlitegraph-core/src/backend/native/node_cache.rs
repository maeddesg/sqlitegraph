//! Node store caching layer for performance optimization.
//!
//! This module provides a simple LRU cache for NodeRecord instances to avoid
//! repeated deserialization during graph traversal operations.

use crate::backend::native::types::*;
use std::collections::HashMap;

/// Simple LRU cache for node records with bounded size
pub struct NodeRecordCache {
    cache: HashMap<NativeNodeId, NodeRecord>,
    access_order: Vec<NativeNodeId>,
    max_size: usize,
}

impl NodeRecordCache {
    /// Create a new cache with the specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            access_order: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Get a cached node record, returning None if not found
    pub fn get(&mut self, node_id: NativeNodeId) -> Option<NodeRecord> {
        if let Some(node) = self.cache.remove(&node_id) {
            // Move to end (most recently used)
            self.access_order.retain(|&id| id != node_id);
            self.access_order.push(node_id);
            self.cache.insert(node_id, node.clone());
            Some(node)
        } else {
            None
        }
    }

    /// Insert a node record into the cache
    pub fn put(&mut self, node_id: NativeNodeId, node: NodeRecord) {
        // Remove existing entry if present
        if self.cache.contains_key(&node_id) {
            self.access_order.retain(|&id| id != node_id);
        }

        // Evict oldest entries if necessary
        while self.cache.len() >= self.max_size {
            if let Some(oldest_id) = self.access_order.first() {
                let oldest_id = *oldest_id;
                self.cache.remove(&oldest_id);
                self.access_order.remove(0);
            } else {
                break;
            }
        }

        // Insert new entry
        self.access_order.push(node_id);
        self.cache.insert(node_id, node);
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
    }

    /// Get the current cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache contains the given node ID
    pub fn contains(&self, node_id: NativeNodeId) -> bool {
        self.cache.contains_key(&node_id)
    }

    /// Remove a specific node from the cache
    pub fn remove(&mut self, node_id: NativeNodeId) -> Option<NodeRecord> {
        if self.cache.remove(&node_id).is_some() {
            self.access_order.retain(|&id| id != node_id);
            // Note: We can't return the node because it would require cloning
            None
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic_operations() {
        let mut cache = NodeRecordCache::new(3);

        // Test empty cache
        assert!(cache.get(1).is_none());
        assert_eq!(cache.size(), 0);

        // Test insert and get
        let node = NodeRecord::new(
            1,
            "Test".to_string(),
            "test".to_string(),
            serde_json::json!({}),
        );
        cache.put(1, node.clone());
        assert_eq!(cache.size(), 1);

        let retrieved = cache.get(1).unwrap();
        assert_eq!(retrieved.id, node.id);
        assert_eq!(retrieved.kind, node.kind);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = NodeRecordCache::new(2);

        // Insert two nodes
        let node1 = NodeRecord::new(
            1,
            "Test".to_string(),
            "test1".to_string(),
            serde_json::json!({}),
        );
        let node2 = NodeRecord::new(
            2,
            "Test".to_string(),
            "test2".to_string(),
            serde_json::json!({}),
        );

        cache.put(1, node1);
        cache.put(2, node2);
        assert_eq!(cache.size(), 2);

        // Insert third node, should evict first
        let node3 = NodeRecord::new(
            3,
            "Test".to_string(),
            "test3".to_string(),
            serde_json::json!({}),
        );
        cache.put(3, node3);
        assert_eq!(cache.size(), 2);

        // First node should be evicted
        assert!(cache.get(1).is_none());

        // Second and third should be available
        assert!(cache.get(2).is_some());
        assert!(cache.get(3).is_some());
    }

    #[test]
    fn test_access_order_update() {
        let mut cache = NodeRecordCache::new(3);

        // Insert three nodes
        let node1 = NodeRecord::new(
            1,
            "Test".to_string(),
            "test1".to_string(),
            serde_json::json!({}),
        );
        let node2 = NodeRecord::new(
            2,
            "Test".to_string(),
            "test2".to_string(),
            serde_json::json!({}),
        );
        let node3 = NodeRecord::new(
            3,
            "Test".to_string(),
            "test3".to_string(),
            serde_json::json!({}),
        );

        cache.put(1, node1);
        cache.put(2, node2);
        cache.put(3, node3);

        // Access first node, should make it most recently used
        cache.get(1);

        // Insert fourth node, should evict the least recently used (node2)
        let node4 = NodeRecord::new(
            4,
            "Test".to_string(),
            "test4".to_string(),
            serde_json::json!({}),
        );
        cache.put(4, node4);

        // Node2 should be evicted, nodes 1, 3, 4 should be available
        assert!(cache.get(2).is_none());
        assert!(cache.get(1).is_some());
        assert!(cache.get(3).is_some());
        assert!(cache.get(4).is_some());
    }
}

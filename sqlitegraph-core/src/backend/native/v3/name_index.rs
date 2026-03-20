//! Name index for fast exact, prefix, and substring name lookups
//!
//! Supports:
//! - Exact match: "my_func" → O(1) lookup
//! - Prefix match: "my_func*" → O(k) lookup where k = matches
//! - Substring match: "func" → O(n) lookup where n = unique names (contains)
//!
//! Does NOT support:
//! - Suffix match: "*func"
//! - Middle wildcard: "my*func"
//! - Single char wildcard: "func?bar"
//! - Character classes: "func[abc]"

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Name index mapping names to node IDs
///
/// Structure: HashMap<name, Vec<node_id>>
/// - Fast exact lookup: O(1)
/// - Fast prefix lookup: O(m) where m = unique names starting with prefix
pub struct NameIndex {
    inner: Arc<RwLock<HashMap<String, Vec<i64>>>>,
}

impl NameIndex {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a node name into the index
    pub fn insert(&self, name: String, node_id: i64) {
        let mut index = self.inner.write();
        index.entry(name).or_default().push(node_id);
    }

    /// Clear all entries from the index
    pub fn clear(&self) {
        let mut index = self.inner.write();
        index.clear();
    }

    /// Exact match lookup
    pub fn get_exact(&self, name: &str) -> Vec<i64> {
        let index = self.inner.read();
        index.get(name).cloned().unwrap_or_default()
    }

    /// Prefix match lookup
    /// Returns all node IDs where name starts with the given prefix
    pub fn get_prefix(&self, prefix: &str) -> Vec<i64> {
        let index = self.inner.read();
        let mut result = Vec::new();
        for (name, ids) in index.iter() {
            if name.starts_with(prefix) {
                result.extend(ids.clone());
            }
        }
        result
    }

    /// Substring match lookup
    /// Returns all node IDs where name contains the given substring
    /// This is O(n) where n = unique names in the index
    pub fn get_substring(&self, substring: &str) -> Vec<i64> {
        let index = self.inner.read();
        let mut result = Vec::new();
        for (name, ids) in index.iter() {
            if name.contains(substring) {
                result.extend(ids.clone());
            }
        }
        result
    }

    /// Get index statistics
    pub fn stats(&self) -> NameIndexStats {
        let index = self.inner.read();
        let total_names = index.len();
        let total_nodes: usize = index.values().map(|v| v.len()).sum();
        NameIndexStats {
            unique_names: total_names,
            total_nodes,
        }
    }
}

pub struct NameIndexStats {
    pub unique_names: usize,
    pub total_nodes: usize,
}

impl NameIndex {
    /// Export all index data for persistence
    pub(crate) fn export(&self) -> HashMap<String, Vec<i64>> {
        self.inner.read().clone()
    }

    /// Import index data from persistence
    pub(crate) fn import(&self, data: HashMap<String, Vec<i64>>) {
        let mut index = self.inner.write();
        *index = data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let index = NameIndex::new();
        index.insert("func_a".to_string(), 1);
        index.insert("func_b".to_string(), 2);
        index.insert("class_a".to_string(), 3);

        assert_eq!(index.get_exact("func_a"), vec![1]);
        assert_eq!(index.get_exact("func_b"), vec![2]);
        assert_eq!(index.get_exact("nonexistent"), Vec::<i64>::new());
    }

    #[test]
    fn test_prefix_match() {
        let index = NameIndex::new();
        index.insert("func_a".to_string(), 1);
        index.insert("func_b".to_string(), 2);
        index.insert("class_a".to_string(), 3);
        index.insert("func_ab".to_string(), 4);

        let results = index.get_prefix("func");
        assert_eq!(results.len(), 3);
        assert!(results.contains(&1));
        assert!(results.contains(&2));
        assert!(results.contains(&4));
        assert!(!results.contains(&3));
    }

    #[test]
    fn test_multiple_nodes_same_name() {
        let index = NameIndex::new();
        index.insert("duplicate".to_string(), 1);
        index.insert("duplicate".to_string(), 2);
        index.insert("duplicate".to_string(), 3);

        let mut results = index.get_exact("duplicate");
        results.sort();
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[test]
    fn test_stats() {
        let index = NameIndex::new();
        index.insert("a".to_string(), 1);
        index.insert("b".to_string(), 2);
        index.insert("a".to_string(), 3); // duplicate name

        let stats = index.stats();
        assert_eq!(stats.unique_names, 2);
        assert_eq!(stats.total_nodes, 3);
    }
}

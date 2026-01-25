//! In-memory HashMap-based KV store implementation

use crate::backend::native::v2::kv_store::types::{KvEntry, KvMetadata, KvStoreError, KvValue};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::SystemTime;

/// In-memory key-value store with HashMap-based storage
#[derive(Debug, Default)]
pub struct KvStore {
    entries: RwLock<HashMap<Vec<u8>, KvEntry>>,
}

impl KvStore {
    /// Create a new empty KV store
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a value by key
    pub fn get(&self, key: &[u8]) -> Result<Option<KvValue>, KvStoreError> {
        let entries = self.entries.read();
        Ok(entries.get(key).map(|entry| entry.value.clone()))
    }

    /// Set a value with optional TTL
    pub fn set(&mut self, key: Vec<u8>, value: KvValue, ttl: Option<u64>) -> Result<(), KvStoreError> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entries = self.entries.write();
        let is_new = !entries.contains_key(&key);

        let metadata = KvMetadata {
            created_at: if is_new { now } else { entries[&key].metadata.created_at },
            updated_at: now,
            ttl_seconds: ttl,
            version: 0, // Will be set by WAL in plan 02
        };

        let entry = KvEntry { key: key.clone(), value, metadata };
        drop(entries); // Release lock before insert

        let mut entries = self.entries.write();
        entries.insert(key, entry);
        Ok(())
    }

    /// Delete a key
    pub fn delete(&mut self, key: &[u8]) -> Result<(), KvStoreError> {
        let mut entries = self.entries.write();
        entries.remove(key).map(|_| ()).ok_or_else(|| KvStoreError::KeyNotFound(key.to_vec()))
    }

    /// Check if a key exists
    pub fn exists(&self, key: &[u8]) -> bool {
        let entries = self.entries.read();
        entries.contains_key(key)
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        let entries = self.entries.read();
        entries.len()
    }

    /// Internal method for WAL replay - set with explicit version
    ///
    /// This is used during WAL recovery to restore entries with their original versions.
    /// Normal set() operations should use version 0 (the WAL system assigns the real version).
    pub fn set_with_version(&mut self, key: Vec<u8>, value: KvValue, ttl: Option<u64>, version: u64) -> Result<(), KvStoreError> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entries = self.entries.write();
        let is_new = !entries.contains_key(&key);

        let metadata = KvMetadata {
            created_at: if is_new { now } else { entries[&key].metadata.created_at },
            updated_at: now,
            ttl_seconds: ttl,
            version,
        };

        let entry = KvEntry { key: key.clone(), value, metadata };
        drop(entries);

        let mut entries = self.entries.write();
        entries.insert(key, entry);
        Ok(())
    }

    /// Check if an entry is expired (TTL exceeded)
    #[cfg(test)]
    fn is_expired(&self, entry: &KvEntry) -> bool {
        if let Some(ttl) = entry.metadata.ttl_seconds {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            now.saturating_sub(entry.metadata.created_at) > ttl
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_store() {
        let store = KvStore::new();
        assert_eq!(store.len(), 0);
    }
}

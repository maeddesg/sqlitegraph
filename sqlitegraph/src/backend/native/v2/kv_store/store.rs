//! In-memory HashMap-based KV store implementation

use crate::backend::native::v2::kv_store::types::{KvEntry, KvMetadata, KvStoreError, KvValue};
use crate::backend::native::v2::kv_store::ttl;
use crate::snapshot::SnapshotId;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::SystemTime;

/// In-memory key-value store with HashMap-based storage
#[derive(Debug, Default)]
pub struct KvStore {
    /// Visible to kv_store modules for WAL recovery and TTL cleanup
    pub(crate) entries: RwLock<HashMap<Vec<u8>, KvEntry>>,
}

impl KvStore {
    /// Create a new empty KV store
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a value by key
    ///
    /// For tests only - production code should use get_at_snapshot()
    /// Returns most recent committed value (no version filtering)
    pub fn get(&self, key: &[u8]) -> Result<Option<KvValue>, KvStoreError> {
        let entries = self.entries.read();
        Ok(entries.get(key).map(|entry| entry.value.clone()))
    }

    /// Get a value at a specific snapshot
    ///
    /// This enforces snapshot isolation: only data committed at or before
    /// the given snapshot_id is visible.
    ///
    /// TTL is checked lazily: expired entries are filtered on read.
    ///
    /// # Arguments
    /// * `key` - Key to retrieve
    /// * `snapshot_id` - Only return data committed at or before this snapshot
    ///
    /// # Returns
    /// The value if found and visible at snapshot, or None if not found or not visible
    pub fn get_at_snapshot(&self, key: &[u8], snapshot_id: SnapshotId) -> Result<Option<KvValue>, KvStoreError> {
        let entries = self.entries.read();

        if let Some(entry) = entries.get(key) {
            // Check if entry is visible at this snapshot
            if !self.is_visible_at_snapshot(entry.metadata.version, snapshot_id) {
                // Entry version is newer than snapshot - not visible yet
                return Ok(None);
            }

            // Check if entry is expired (lazy TTL cleanup)
            if ttl::is_expired(entry) {
                return Ok(None);
            }

            // Entry is visible and not expired
            Ok(Some(entry.value.clone()))
        } else {
            // Key not found
            Ok(None)
        }
    }

    /// Check if an entry version is visible at a given snapshot
    ///
    /// Entry is visible if version <= snapshot_id.as_lsn()
    /// This matches the Phase 38 architecture where SnapshotId wraps CommitLSN
    fn is_visible_at_snapshot(&self, version: u64, snapshot_id: SnapshotId) -> bool {
        // Entry is visible if version <= snapshot_id.as_lsn()
        // Phase 38: SnapshotId = CommitLSN
        version <= snapshot_id.as_lsn()
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
    ///
    /// Note: This checks TTL lazily - expired keys return false even if present in storage.
    pub fn exists(&self, key: &[u8]) -> bool {
        let entries = self.entries.read();
        if let Some(entry) = entries.get(key) {
            // Key exists, but check if expired
            !ttl::is_expired(entry)
        } else {
            false
        }
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        let entries = self.entries.read();
        entries.len()
    }

    /// Explicit cleanup of all expired entries
    ///
    /// This is a manual cleanup operation - NOT called automatically.
    /// Lazy cleanup on read is sufficient for correctness.
    /// This method is only for space reclamation optimization.
    ///
    /// # Returns
    /// The number of entries removed
    pub fn cleanup_expired(&mut self) -> usize {
        ttl::cleanup_expired_entries(self)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_store() {
        let store = KvStore::new();
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_get_at_snapshot_visible() {
        // Entry should be visible if version <= snapshot_id
        let mut store = KvStore::new();

        // Create entry with version 100
        store.set_with_version(
            b"key".to_vec(),
            KvValue::Integer(42),
            None,
            100
        ).unwrap();

        // Snapshot at version 150 should see the entry
        let snapshot = SnapshotId::from_lsn(150);
        let result = store.get_at_snapshot(b"key", snapshot).unwrap();
        assert_eq!(result, Some(KvValue::Integer(42)));
    }

    #[test]
    fn test_get_at_snapshot_not_visible() {
        // Entry should NOT be visible if version > snapshot_id
        let mut store = KvStore::new();

        // Create entry with version 200
        store.set_with_version(
            b"key".to_vec(),
            KvValue::Integer(42),
            None,
            200
        ).unwrap();

        // Snapshot at version 150 should NOT see the entry (version 200 > 150)
        let snapshot = SnapshotId::from_lsn(150);
        let result = store.get_at_snapshot(b"key", snapshot).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_at_snapshot_expired() {
        // Expired entry should not be visible even if version matches
        let mut store = KvStore::new();

        // Create entry with short TTL (1 second)
        store.set_with_version(
            b"key".to_vec(),
            KvValue::Integer(42),
            Some(1), // 1 second TTL
            100
        ).unwrap();

        // Sleep to ensure expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Snapshot at version 150 should NOT see the entry (expired)
        let snapshot = SnapshotId::from_lsn(150);
        let result = store.get_at_snapshot(b"key", snapshot).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_at_snapshot_missing_key() {
        // Missing key should return None
        let store = KvStore::new();

        let snapshot = SnapshotId::from_lsn(100);
        let result = store.get_at_snapshot(b"missing", snapshot).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_is_visible_at_snapshot() {
        let store = KvStore::new();

        // Snapshot at LSN 100
        let snapshot = SnapshotId::from_lsn(100);

        // Version 50 <= 100: visible
        assert!(store.is_visible_at_snapshot(50, snapshot));

        // Version 100 <= 100: visible
        assert!(store.is_visible_at_snapshot(100, snapshot));

        // Version 150 > 100: NOT visible
        assert!(!store.is_visible_at_snapshot(150, snapshot));
    }

    #[test]
    fn test_snapshot_isolation_multiple_versions() {
        // Test that snapshot isolation filters by version
        let mut store = KvStore::new();

        // Create key with version 100
        store.set_with_version(
            b"key".to_vec(),
            KvValue::Integer(100),
            None,
            100
        ).unwrap();

        // Update same key with version 200 (overwrites version 100)
        store.set_with_version(
            b"key".to_vec(),
            KvValue::Integer(200),
            None,
            200
        ).unwrap();

        // Snapshot at 250 should see version 200 (200 <= 250)
        let snapshot_250 = SnapshotId::from_lsn(250);
        let result = store.get_at_snapshot(b"key", snapshot_250).unwrap();
        assert_eq!(result, Some(KvValue::Integer(200)));

        // Snapshot at 150 should NOT see version 200 (200 > 150)
        let snapshot_150 = SnapshotId::from_lsn(150);
        let result = store.get_at_snapshot(b"key", snapshot_150).unwrap();
        assert_eq!(result, None);

        // Snapshot at 50 should see nothing (version 200 > 50)
        let snapshot_50 = SnapshotId::from_lsn(50);
        let result = store.get_at_snapshot(b"key", snapshot_50).unwrap();
        assert_eq!(result, None);
    }
}

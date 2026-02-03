//! In-memory HashMap-based KV store implementation

use crate::backend::native::v2::kv_store::ttl;
use crate::backend::native::v2::kv_store::types::{KvEntry, KvMetadata, KvStoreError, KvValue};
use crate::snapshot::SnapshotId;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::SystemTime;

/// In-memory key-value store with multi-version storage
///
/// Each key retains a version history (Vec<KvEntry> ordered by LSN ascending).
/// This enables true MVCC snapshot isolation where older snapshots see older versions.
#[derive(Debug, Default)]
pub struct KvStore {
    /// Visible to kv_store modules for WAL recovery and TTL cleanup
    /// Each key maps to a version history Vec<KvEntry>, sorted by version (ascending LSN)
    pub(crate) entries: RwLock<HashMap<Vec<u8>, Vec<KvEntry>>>,
}

impl KvStore {
    /// Create a new empty KV store
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a value by key
    ///
    /// For tests only - production code should use get_at_snapshot()
    /// Returns most recent committed value (latest version in history)
    /// TTL is checked lazily: expired entries return None
    pub fn get(&self, key: &[u8]) -> Result<Option<KvValue>, KvStoreError> {
        let entries = self.entries.read();
        if let Some(versions) = entries.get(key) {
            // Get latest version (last element in Vec)
            if let Some(entry) = versions.last() {
                // Check TTL before returning value
                if ttl::is_expired(entry) {
                    return Ok(None);
                }
                Ok(Some(entry.value.clone()))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get a value at a specific snapshot
    ///
    /// This enforces true MVCC snapshot isolation: finds the latest version
    /// committed at or before the given snapshot_id.
    ///
    /// Uses binary search (O(log n)) to find the correct version in the history.
    ///
    /// TTL is checked lazily: expired entries are filtered on read.
    ///
    /// # Arguments
    /// * `key` - Key to retrieve
    /// * `snapshot_id` - Only return data committed at or before this snapshot
    ///
    /// # Returns
    /// The value if found and visible at snapshot, or None if not found or not visible
    pub fn get_at_snapshot(
        &self,
        key: &[u8],
        snapshot_id: SnapshotId,
    ) -> Result<Option<KvValue>, KvStoreError> {
        let entries = self.entries.read();
        let snapshot_lsn = snapshot_id.as_lsn();

        if let Some(versions) = entries.get(key) {
            // Snapshot at 0 means "see all data" - return latest version
            if snapshot_lsn == 0 {
                if let Some(entry) = versions.last() {
                    if ttl::is_expired(entry) {
                        return Ok(None);
                    }
                    return Ok(Some(entry.value.clone()));
                }
                return Ok(None);
            }

            // Binary search for the latest version with version <= snapshot_lsn
            // partition_point returns index of first element where predicate is false
            // We want: entry.version <= snapshot_lsn
            let idx = versions.partition_point(|e| e.metadata.version <= snapshot_lsn);

            if idx == 0 {
                // All versions are newer than snapshot (all version > snapshot_lsn)
                return Ok(None);
            }

            // versions[idx - 1] is the latest version with version <= snapshot_lsn
            let entry = &versions[idx - 1];

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

    /// Set a value with optional TTL
    ///
    /// Appends a new version to the key's version history.
    /// The version number is set to 0 and will be updated by the WAL system.
    pub fn set(
        &mut self,
        key: Vec<u8>,
        value: KvValue,
        ttl: Option<u64>,
    ) -> Result<(), KvStoreError> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut entries = self.entries.write();

        // Get created_at from existing versions (if any)
        let created_at = if let Some(versions) = entries.get(&key) {
            if let Some(latest) = versions.last() {
                latest.metadata.created_at
            } else {
                now
            }
        } else {
            now
        };

        let metadata = KvMetadata {
            created_at,
            updated_at: now,
            ttl_seconds: ttl,
            version: 0, // Will be set by WAL in plan 02
        };

        let entry = KvEntry {
            key: key.clone(),
            value,
            metadata,
        };

        // Append new version to history (maintains sorted order since LSNs are monotonic)
        entries.entry(key).or_default().push(entry);

        Ok(())
    }

    /// Delete a key
    ///
    /// Removes the entire version history for the key.
    pub fn delete(&mut self, key: &[u8]) -> Result<(), KvStoreError> {
        let mut entries = self.entries.write();
        entries
            .remove(key)
            .map(|_| ())
            .ok_or_else(|| KvStoreError::KeyNotFound(key.to_vec()))
    }

    /// Check if a key exists
    ///
    /// Note: This checks TTL lazily - expired keys return false even if present in storage.
    /// Only the latest version is checked.
    pub fn exists(&self, key: &[u8]) -> bool {
        let entries = self.entries.read();
        if let Some(versions) = entries.get(key) {
            // Check latest version
            if let Some(entry) = versions.last() {
                // Key exists, but check if expired
                !ttl::is_expired(entry)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        let entries = self.entries.read();
        entries.len()
    }

    /// Scan all entries with a given prefix at a snapshot
    ///
    /// Returns all keys that start with the given prefix, along with their values.
    /// Results are sorted in lexicographic order by key.
    ///
    /// # Arguments
    /// * `snapshot_id` - Only return data committed at or before this snapshot
    /// * `prefix` - Prefix to match (empty prefix returns all keys)
    ///
    /// # Returns
    /// Vector of (key, value) pairs for all matching keys
    pub fn prefix_scan(
        &self,
        snapshot_id: SnapshotId,
        prefix: &[u8],
    ) -> Result<Vec<(Vec<u8>, KvValue)>, KvStoreError> {
        let entries = self.entries.read();
        let snapshot_lsn = snapshot_id.as_lsn();

        let mut results = Vec::new();
        for (key, versions) in entries.iter() {
            if !key.starts_with(prefix) {
                continue;
            }

            // Find version visible at snapshot
            let entry = if snapshot_lsn == 0 {
                // Snapshot at 0 means "see all data" - get latest version
                versions.last()
            } else {
                // Binary search for version <= snapshot_lsn
                let idx = versions.partition_point(|e| e.metadata.version <= snapshot_lsn);
                if idx == 0 {
                    None
                } else {
                    Some(&versions[idx - 1])
                }
            };

            if let Some(e) = entry {
                if !ttl::is_expired(e) {
                    results.push((key.clone(), e.value.clone()));
                }
            }
        }
        results.sort_by(|a, b| a.0.cmp(&b.0)); // Lexicographic order
        Ok(results)
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
    ///
    /// Maintains version history in sorted order by LSN.
    pub fn set_with_version(
        &mut self,
        key: Vec<u8>,
        value: KvValue,
        ttl: Option<u64>,
        version: u64,
    ) -> Result<(), KvStoreError> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut entries = self.entries.write();

        // Get created_at from existing versions (if any)
        let created_at = if let Some(versions) = entries.get(&key) {
            if let Some(latest) = versions.last() {
                latest.metadata.created_at
            } else {
                now
            }
        } else {
            now
        };

        let metadata = KvMetadata {
            created_at,
            updated_at: now,
            ttl_seconds: ttl,
            version,
        };

        let entry = KvEntry {
            key: key.clone(),
            value,
            metadata,
        };

        // Insert into version history, maintaining sorted order by version
        let versions = entries.entry(key).or_default();

        // Find insertion point to maintain sorted order
        let pos = versions.partition_point(|e| e.metadata.version < version);

        // Insert at correct position
        versions.insert(pos, entry);

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
        store
            .set_with_version(b"key".to_vec(), KvValue::Integer(42), None, 100)
            .unwrap();

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
        store
            .set_with_version(b"key".to_vec(), KvValue::Integer(42), None, 200)
            .unwrap();

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
        store
            .set_with_version(
                b"key".to_vec(),
                KvValue::Integer(42),
                Some(1), // 1 second TTL
                100,
            )
            .unwrap();

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
    fn test_snapshot_isolation_multiple_versions() {
        // Test true MVCC: multiple versions retained, snapshots see correct version
        let mut store = KvStore::new();

        // Create key with version 100
        store
            .set_with_version(b"key".to_vec(), KvValue::Integer(100), None, 100)
            .unwrap();

        // Update same key with version 200 (MVCC: retains version 100)
        store
            .set_with_version(b"key".to_vec(), KvValue::Integer(200), None, 200)
            .unwrap();

        // Snapshot at 250 should see version 200 (latest version <= 250)
        let snapshot_250 = SnapshotId::from_lsn(250);
        let result = store.get_at_snapshot(b"key", snapshot_250).unwrap();
        assert_eq!(result, Some(KvValue::Integer(200)));

        // Snapshot at 150 should see version 100 (version history retained!)
        let snapshot_150 = SnapshotId::from_lsn(150);
        let result = store.get_at_snapshot(b"key", snapshot_150).unwrap();
        assert_eq!(result, Some(KvValue::Integer(100))); // TRUE MVCC!

        // Snapshot at 50 should see nothing (all versions > 50)
        let snapshot_50 = SnapshotId::from_lsn(50);
        let result = store.get_at_snapshot(b"key", snapshot_50).unwrap();
        assert_eq!(result, None);
    }
}

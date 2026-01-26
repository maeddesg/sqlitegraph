//! TTL (Time-To-Live) helpers for KV store
//!
//! This module provides lazy TTL cleanup - expiration is checked on read,
//! not proactively by background threads. This matches the Phase 43 architecture
//! constraint: NO background TTL sweepers.

use crate::backend::native::v2::kv_store::store::KvStore;
use crate::backend::native::v2::kv_store::types::KvEntry;
use std::time::SystemTime;

/// Check if an entry is expired (TTL exceeded)
///
/// This implements lazy TTL cleanup: expiration is detected when the entry
/// is accessed, not proactively by background threads.
///
/// # Arguments
/// * `entry` - The KV entry to check
///
/// # Returns
/// * `true` - Entry is expired (TTL exceeded)
/// * `false` - Entry is not expired (no TTL set or TTL not yet exceeded)
///
/// # TTL Precision
/// Second-level precision is used. The entry is expired if:
/// `current_time > created_at + ttl_seconds`
pub fn is_expired(entry: &KvEntry) -> bool {
    if let Some(ttl) = entry.metadata.ttl_seconds {
        // TTL of 0 means "already expired"
        if ttl == 0 {
            return true;
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Check if current time > expiration time
        // Handle overflow gracefully with saturating_add
        let expiration_time = entry.metadata.created_at.saturating_add(ttl);
        now > expiration_time
    } else {
        // No TTL set - entry never expires
        false
    }
}

/// Calculate seconds until expiration
///
/// # Arguments
/// * `entry` - The KV entry to check
///
/// # Returns
/// * `Some(seconds)` - Time remaining until expiration (may be 0 if expiring now)
/// * `None` - No TTL set (entry persists indefinitely)
///
/// # Examples
/// ```ignore
/// let entry = create_entry_with_ttl(60); // 60 second TTL
/// assert!(seconds_until_expiration(&entry).is_some());
/// assert!(seconds_until_expiration(&entry).unwrap() <= 60);
/// ```
pub fn seconds_until_expiration(entry: &KvEntry) -> Option<u64> {
    if let Some(ttl) = entry.metadata.ttl_seconds {
        // TTL of 0 means "already expired"
        if ttl == 0 {
            return Some(0);
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let expiration_time = entry.metadata.created_at.saturating_add(ttl);

        if now >= expiration_time {
            // Already expired
            Some(0)
        } else {
            // Time remaining
            Some(expiration_time - now)
        }
    } else {
        // No TTL set
        None
    }
}

/// Explicit cleanup of all expired entries
///
/// This is a manual cleanup operation that removes expired entries
/// from the store. It is NOT called automatically - users must call it
/// explicitly if they want to reclaim space from expired entries.
///
/// With multi-version storage, this selectively removes only expired versions,
/// keeping unexpired versions intact. If all versions for a key are expired,
/// the key is removed entirely.
///
/// NOTE: This is NOT needed for correctness - lazy cleanup on read is
/// sufficient. This is purely an optimization for space reclamation.
///
/// # Arguments
/// * `store` - The KV store to clean
///
/// # Returns
/// The number of versions removed
///
/// # Examples
/// ```ignore
/// let mut store = KvStore::new();
/// // ... add entries with TTL ...
/// let removed = cleanup_expired_entries(&mut store);
/// println!("Removed {} expired versions", removed);
/// ```
pub fn cleanup_expired_entries(store: &mut KvStore) -> usize {
    use parking_lot::RwLockWriteGuard;
    use std::collections::HashMap;

    // Get write access to entries
    let mut entries: RwLockWriteGuard<'_, HashMap<Vec<u8>, Vec<KvEntry>>> = store.entries.write();

    let mut total_removed = 0;

    // Process each key's version history
    let keys_to_remove: Vec<Vec<u8>> = entries
        .iter()
        .filter(|(_, versions)| {
            // Check if ALL versions are expired
            versions.iter().all(|v| is_expired(v))
        })
        .map(|(key, _)| key.clone())
        .collect();

    // Remove keys where all versions are expired
    for key in keys_to_remove {
        let count = entries.remove(&key).map_or(0, |v| v.len());
        total_removed += count;
    }

    // For keys with some expired versions, filter them out
    for versions in entries.values_mut() {
        let original_len = versions.len();
        versions.retain(|v| !is_expired(v));
        total_removed += original_len - versions.len();
    }

    total_removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v2::kv_store::types::{KvEntry, KvMetadata, KvValue};
    use std::time::{Duration, SystemTime};

    fn create_test_entry(ttl_seconds: Option<u64>) -> KvEntry {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        KvEntry {
            key: b"test_key".to_vec(),
            value: KvValue::Integer(42),
            metadata: KvMetadata {
                created_at: now,
                updated_at: now,
                ttl_seconds,
                version: 100,
            },
        }
    }

    fn create_old_entry(ttl_seconds: u64) -> KvEntry {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Create entry that was made `ttl_seconds + 1` seconds ago
        let old_created_at = now.saturating_sub(ttl_seconds + 1);

        KvEntry {
            key: b"old_key".to_vec(),
            value: KvValue::Integer(42),
            metadata: KvMetadata {
                created_at: old_created_at,
                updated_at: old_created_at,
                ttl_seconds: Some(ttl_seconds),
                version: 100,
            },
        }
    }

    #[test]
    fn test_is_expired_no_ttl() {
        let entry = create_test_entry(None);
        // No TTL - should never expire
        assert!(!is_expired(&entry));
    }

    #[test]
    fn test_is_expired_not_yet() {
        let entry = create_test_entry(Some(60)); // 60 second TTL
        // Just created - should not be expired yet
        assert!(!is_expired(&entry));
    }

    #[test]
    fn test_is_expired_after_ttl() {
        let entry = create_old_entry(1); // Created 2 seconds ago, 1 second TTL
        // Should be expired
        assert!(is_expired(&entry));
    }

    #[test]
    fn test_seconds_until_expiration_no_ttl() {
        let entry = create_test_entry(None);
        assert_eq!(seconds_until_expiration(&entry), None);
    }

    #[test]
    fn test_seconds_until_expiration_future() {
        let entry = create_test_entry(Some(60)); // 60 second TTL
        let seconds = seconds_until_expiration(&entry).expect("should have TTL");
        // Should be roughly 60 seconds (allow 1 second for test execution)
        assert!(seconds <= 60);
        assert!(seconds > 58); // Allow some time for test execution
    }

    #[test]
    fn test_seconds_until_expiration_past() {
        let entry = create_old_entry(1); // Created 2 seconds ago, 1 second TTL
        let seconds = seconds_until_expiration(&entry).expect("should have TTL");
        // Should be 0 (already expired)
        assert_eq!(seconds, 0);
    }

    #[test]
    fn test_cleanup_expired_entries() {
        let mut store = KvStore::new();

        // Add non-expired entry (no TTL)
        store
            .set(b"key1".to_vec(), KvValue::Integer(1), None)
            .unwrap();

        // Add non-expired entry (long TTL)
        store
            .set(b"key2".to_vec(), KvValue::Integer(2), Some(3600))
            .unwrap();

        // Add expired entry by manually manipulating metadata
        {
            let old_created_at = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs().saturating_sub(10))
                .unwrap_or(0);

            // Manually insert an expired entry
            let expired_entry = KvEntry {
                key: b"key3".to_vec(),
                value: KvValue::Integer(3),
                metadata: KvMetadata {
                    created_at: old_created_at,
                    updated_at: old_created_at,
                    ttl_seconds: Some(1), // 1 second TTL, created 10 seconds ago
                    version: 100,
                },
            };

            let mut entries = store.entries.write();
            entries.insert(b"key3".to_vec(), vec![expired_entry]);
        }

        // Before cleanup: 3 entries
        assert_eq!(store.len(), 3);

        // Cleanup expired entries
        let removed = cleanup_expired_entries(&mut store);

        // Should have removed 1 entry
        assert_eq!(removed, 1);

        // After cleanup: 2 entries
        assert_eq!(store.len(), 2);

        // Verify non-expired entries still exist
        assert!(store.exists(b"key1"));
        assert!(store.exists(b"key2"));
        assert!(!store.exists(b"key3"));
    }

    #[test]
    fn test_cleanup_empty_store() {
        let mut store = KvStore::new();
        let removed = cleanup_expired_entries(&mut store);
        assert_eq!(removed, 0);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_ttl_overflow_handling() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Create entry with very large TTL (near u64::MAX)
        let entry = KvEntry {
            key: b"test_key".to_vec(),
            value: KvValue::Integer(42),
            metadata: KvMetadata {
                created_at: now,
                updated_at: now,
                ttl_seconds: Some(u64::MAX - 100), // Very large TTL
                version: 100,
            },
        };

        // Should handle overflow gracefully and not be expired
        assert!(!is_expired(&entry));

        // seconds_until_expiration should return Some value (not overflow)
        let seconds = seconds_until_expiration(&entry);
        assert!(seconds.is_some());
    }
}

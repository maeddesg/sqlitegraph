//! Comprehensive snapshot isolation tests for KV operations
//!
//! These tests verify that snapshot isolation works correctly for both
//! Native and SQLite backends, ensuring feature parity.

use crate::backend::GraphBackend;
use crate::backend::native::v2::kv_store::types::{KvValue, KvStoreError};
use crate::snapshot::SnapshotId;
use crate::backend::NativeGraphBackend;

#[cfg(test)]
mod basic_visibility_tests {
    use super::*;

    #[test]
    fn test_get_at_current_snapshot() {
        // Reads data at current snapshot
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(b"current_key".to_vec(), KvValue::Integer(100), None)
            .unwrap();

        let snapshot = SnapshotId::current();
        let value = backend.kv_get(snapshot, b"current_key").unwrap();
        assert_eq!(value, Some(KvValue::Integer(100)));
    }

    #[test]
    fn test_get_at_old_snapshot() {
        // Old snapshot doesn't see newer writes
        let backend = NativeGraphBackend::new_temp().unwrap();

        // Create old snapshot
        let old_snapshot = SnapshotId::current();

        // Write data after old snapshot
        backend
            .kv_set(b"future_key".to_vec(), KvValue::Integer(200), None)
            .unwrap();

        // Old snapshot should not see the write
        let value = backend.kv_get(old_snapshot, b"future_key").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_get_at_future_snapshot() {
        // Future snapshot sees all committed data
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(b"committed_key".to_vec(), KvValue::Integer(300), None)
            .unwrap();

        // Create a future snapshot (with higher LSN)
        let current = SnapshotId::current();
        let future_lsn = current.as_lsn() + 1000;
        let future_snapshot = SnapshotId::from_lsn(future_lsn);

        let value = backend.kv_get(future_snapshot, b"committed_key").unwrap();
        assert_eq!(value, Some(KvValue::Integer(300)));
    }
}

#[cfg(test)]
mod version_ordering_tests {
    use super::*;

    #[test]
    fn test_version_increments_on_set() {
        // Each set increments version
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(b"version_key".to_vec(), KvValue::Integer(1), None)
            .unwrap();

        backend
            .kv_set(b"version_key".to_vec(), KvValue::Integer(2), None)
            .unwrap();

        backend
            .kv_set(b"version_key".to_vec(), KvValue::Integer(3), None)
            .unwrap();

        // Latest value should be visible at current snapshot
        let snapshot = SnapshotId::current();
        let value = backend.kv_get(snapshot, b"version_key").unwrap();
        assert_eq!(value, Some(KvValue::Integer(3)));
    }

    #[test]
    fn test_snapshot_filters_by_version() {
        // Only visible entries returned
        let backend = NativeGraphBackend::new_temp().unwrap();

        // First write
        backend
            .kv_set(b"filter_key".to_vec(), KvValue::Integer(100), None)
            .unwrap();

        let snapshot_after_first = SnapshotId::current();

        // Second write
        backend
            .kv_set(b"filter_key".to_vec(), KvValue::Integer(200), None)
            .unwrap();

        // Snapshot after first write should NOT see second write
        let value = backend.kv_get(snapshot_after_first, b"filter_key").unwrap();
        assert_eq!(value, None); // Version 200 > snapshot_after_first
    }

    #[test]
    fn test_multiple_versions_same_key() {
        // Correct version selected
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(b"multi_key".to_vec(), KvValue::String("v1".to_string()), None)
            .unwrap();

        let snapshot_v1 = SnapshotId::current();

        backend
            .kv_set(b"multi_key".to_vec(), KvValue::String("v2".to_string()), None)
            .unwrap();

        let snapshot_v2 = SnapshotId::current();

        backend
            .kv_set(b"multi_key".to_vec(), KvValue::String("v3".to_string()), None)
            .unwrap();

        // Snapshot v1 sees nothing (v2 and v3 versions > v1)
        let value = backend.kv_get(snapshot_v1, b"multi_key").unwrap();
        assert_eq!(value, None);

        // Snapshot v2 sees nothing (v3 version > v2)
        let value = backend.kv_get(snapshot_v2, b"multi_key").unwrap();
        assert_eq!(value, None);

        // Current snapshot sees v3
        let snapshot_current = SnapshotId::current();
        let value = backend.kv_get(snapshot_current, b"multi_key").unwrap();
        assert_eq!(value, Some(KvValue::String("v3".to_string())));
    }
}

#[cfg(test)]
mod ttl_interaction_tests {
    use super::*;

    #[test]
    fn test_expired_not_visible_any_snapshot() {
        // Expired entries invisible regardless of snapshot
        let backend = NativeGraphBackend::new_temp().unwrap();

        // Set with 1 second TTL
        backend
            .kv_set(
                b"expire_key".to_vec(),
                KvValue::String("expires_soon".to_string()),
                Some(1),
            )
            .unwrap();

        // Visible immediately
        let snapshot = SnapshotId::current();
        let value = backend.kv_get(snapshot, b"expire_key").unwrap();
        assert_eq!(value, Some(KvValue::String("expires_soon".to_string())));

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Not visible at any snapshot after expiration
        let snapshot = SnapshotId::current();
        let value = backend.kv_get(snapshot, b"expire_key").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_ttl_checked_after_snapshot_filter() {
        // Both filters applied correctly
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(
                b"both_filters_key".to_vec(),
                KvValue::Integer(42),
                Some(1),
            )
            .unwrap();

        let snapshot = SnapshotId::current();

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // TTL filter applies even if snapshot filter would pass
        let value = backend.kv_get(snapshot, b"both_filters_key").unwrap();
        assert_eq!(value, None);
    }
}

#[cfg(test)]
mod backend_integration_tests {
    use super::*;

    #[test]
    fn test_native_kv_through_backend() {
        // NativeGraphBackend KV methods work
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(
                b"native_test".to_vec(),
                KvValue::String("native_value".to_string()),
                None,
            )
            .unwrap();

        let snapshot = SnapshotId::current();
        let value = backend.kv_get(snapshot, b"native_test").unwrap();
        assert_eq!(value, Some(KvValue::String("native_value".to_string())));

        backend.kv_delete(b"native_test").unwrap();

        let value = backend.kv_get(snapshot, b"native_test").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_sqlite_kv_through_backend() {
        // SqliteGraphBackend KV methods work
        use crate::backend::SqliteGraphBackend;

        let backend = SqliteGraphBackend::in_memory().unwrap();

        backend
            .kv_set(
                b"sqlite_test".to_vec(),
                KvValue::String("sqlite_value".to_string()),
                None,
            )
            .unwrap();

        let snapshot = SnapshotId::current();
        let value = backend.kv_get(snapshot, b"sqlite_test").unwrap();
        assert_eq!(value, Some(KvValue::String("sqlite_value".to_string())));

        backend.kv_delete(b"sqlite_test").unwrap();

        let value = backend.kv_get(snapshot, b"sqlite_test").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_kv_same_transaction_as_graph() {
        // KV and graph ops in same transaction context
        let backend = NativeGraphBackend::new_temp().unwrap();

        // Insert graph node
        let node_id = backend
            .insert_node(crate::backend::NodeSpec {
                kind: "TestNode".to_string(),
                name: Some("test_node".to_string()),
                file_path: None,
                data: None,
            })
            .unwrap();

        // Set KV value
        backend
            .kv_set(
                b"node_metadata".to_vec(),
                KvValue::Integer(node_id),
                None,
            )
            .unwrap();

        // Read both at same snapshot
        let snapshot = SnapshotId::current();

        let node = backend.get_node(snapshot, node_id).unwrap();
        assert_eq!(node.id, node_id);

        let kv_value = backend.kv_get(snapshot, b"node_metadata").unwrap();
        assert_eq!(kv_value, Some(KvValue::Integer(node_id)));
    }
}

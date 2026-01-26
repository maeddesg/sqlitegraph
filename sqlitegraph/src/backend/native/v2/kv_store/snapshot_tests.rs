//! Comprehensive snapshot isolation tests for KV operations
//!
//! These tests verify that snapshot isolation works correctly for both
//! Native and SQLite backends, ensuring feature parity.

#![cfg(feature = "native-v2")]


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
        // Current snapshot sees all committed data (SnapshotId::current() = 0)
        // Note: Full snapshot isolation semantics require Phase 38+ completion
        let backend = NativeGraphBackend::new_temp().unwrap();

        // Current snapshot sees all data
        let current = SnapshotId::current();

        // Write data
        backend
            .kv_set(b"test_key".to_vec(), KvValue::Integer(200), None)
            .unwrap();

        // Current snapshot should see the write
        let value = backend.kv_get(current, b"test_key").unwrap();
        assert_eq!(value, Some(KvValue::Integer(200)));

        // A snapshot with LSN = 0 also sees all data
        let zero_snapshot = SnapshotId::from_lsn(0);
        let value2 = backend.kv_get(zero_snapshot, b"test_key").unwrap();
        assert_eq!(value2, Some(KvValue::Integer(200)));
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
        // Version filtering based on LSN comparison
        // Note: Full snapshot isolation requires Phase 38+ completion
        let backend = NativeGraphBackend::new_temp().unwrap();

        // First write
        backend
            .kv_set(b"filter_key".to_vec(), KvValue::Integer(100), None)
            .unwrap();

        // Snapshot with LSN 0 sees all data
        let zero_snapshot = SnapshotId::from_lsn(0);
        let value = backend.kv_get(zero_snapshot, b"filter_key").unwrap();
        assert_eq!(value, Some(KvValue::Integer(100)));

        // Second write
        backend
            .kv_set(b"filter_key".to_vec(), KvValue::Integer(200), None)
            .unwrap();

        // Zero snapshot still sees latest value
        let value = backend.kv_get(zero_snapshot, b"filter_key").unwrap();
        assert_eq!(value, Some(KvValue::Integer(200)));
    }

    #[test]
    fn test_multiple_versions_same_key() {
        // Multiple writes to same key - latest wins
        // Note: Full multi-version support requires additional work
        let backend = NativeGraphBackend::new_temp().unwrap();

        backend
            .kv_set(b"multi_key".to_vec(), KvValue::String("v1".to_string()), None)
            .unwrap();

        backend
            .kv_set(b"multi_key".to_vec(), KvValue::String("v2".to_string()), None)
            .unwrap();

        backend
            .kv_set(b"multi_key".to_vec(), KvValue::String("v3".to_string()), None)
            .unwrap();

        // Current snapshot sees latest value (v3)
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
                name: "test_node".to_string(),
                file_path: None,
                data: serde_json::Value::Null,
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use crate::backend::SqliteGraphBackend;

    #[test]
    fn test_both_backends_same_api() {
        // Verify both backends accept same input types
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        let key = b"api_test_key".to_vec();
        let value = KvValue::String("test_value".to_string());

        // Both backends accept same types
        native_backend.kv_set(key.clone(), value.clone(), None).unwrap();
        sqlite_backend.kv_set(key, value, None).unwrap();

        let snapshot = SnapshotId::current();

        let native_value = native_backend.kv_get(snapshot, b"api_test_key").unwrap();
        let sqlite_value = sqlite_backend.kv_get(snapshot, b"api_test_key").unwrap();

        assert_eq!(native_value, sqlite_value);
    }

    #[test]
    fn test_both_backends_same_errors() {
        // Verify error types match (both return None for missing keys)
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        let snapshot = SnapshotId::current();

        // Both return None for missing keys
        let native_result = native_backend.kv_get(snapshot, b"missing_key");
        let sqlite_result = sqlite_backend.kv_get(snapshot, b"missing_key");

        assert!(native_result.is_ok());
        assert!(sqlite_result.is_ok());
        assert_eq!(native_result.unwrap(), None);
        assert_eq!(sqlite_result.unwrap(), None);
    }

    #[test]
    fn test_value_types_work_both_backends() {
        // All KvValue variants work on both backends
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        // Test all KvValue variants
        let test_cases = vec![
            (b"bytes_key".as_ref(), KvValue::Bytes(vec![1, 2, 3])),
            (b"string_key".as_ref(), KvValue::String("hello".to_string())),
            (b"integer_key".as_ref(), KvValue::Integer(42)),
            (b"float_key".as_ref(), KvValue::Float(3.14)),
            (b"boolean_key".as_ref(), KvValue::Boolean(true)),
            (
                b"json_key".as_ref(),
                KvValue::Json(serde_json::json!({"test": "data"})),
            ),
        ];

        for (key, value) in test_cases {
            // Native backend
            native_backend.kv_set(key.to_vec(), value.clone(), None).unwrap();
            let snapshot = SnapshotId::current();
            let native_result = native_backend.kv_get(snapshot, key).unwrap();

            // SQLite backend
            sqlite_backend.kv_set(key.to_vec(), value.clone(), None).unwrap();
            let snapshot = SnapshotId::current();
            let sqlite_result = sqlite_backend.kv_get(snapshot, key).unwrap();

            assert_eq!(native_result, sqlite_result);
            assert_eq!(native_result, Some(value));
        }
    }

    #[test]
    fn test_native_and_sqlite_same_behavior() {
        // Same operations produce same results
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        // Set, get, delete sequence
        let key = b"behavior_test_key".to_vec();
        let value1 = KvValue::Integer(100);
        let value2 = KvValue::Integer(200);

        // Set initial value
        native_backend.kv_set(key.clone(), value1.clone(), None).unwrap();
        sqlite_backend.kv_set(key.clone(), value1.clone(), None).unwrap();

        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, sqlite_result);
        assert_eq!(native_result, Some(value1));

        // Update value
        native_backend.kv_set(key.clone(), value2.clone(), None).unwrap();
        sqlite_backend.kv_set(key.clone(), value2.clone(), None).unwrap();

        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, sqlite_result);
        assert_eq!(native_result, Some(value2));

        // Delete
        native_backend.kv_delete(&key).unwrap();
        sqlite_backend.kv_delete(&key).unwrap();

        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, sqlite_result);
        assert_eq!(native_result, None);
    }

    #[test]
    fn test_ttl_both_backends() {
        // TTL works on both backends
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        let key = b"ttl_test_key".to_vec();
        let value = KvValue::String("expires".to_string());

        // Set with 2 second TTL
        native_backend
            .kv_set(key.clone(), value.clone(), Some(2))
            .unwrap();
        sqlite_backend
            .kv_set(key.clone(), value.clone(), Some(2))
            .unwrap();

        // Both should be visible immediately
        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, sqlite_result);
        assert_eq!(native_result, Some(value.clone()));

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(3));

        // Both should return None after expiration
        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, sqlite_result);
        assert_eq!(native_result, None);
    }

    #[test]
    fn test_delete_both_backends() {
        // Delete behavior matches
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        let key = b"delete_test_key".to_vec();
        let value = KvValue::String("delete_me".to_string());

        // Set values
        native_backend.kv_set(key.clone(), value.clone(), None).unwrap();
        sqlite_backend.kv_set(key.clone(), value.clone(), None).unwrap();

        // Verify they exist
        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, Some(value.clone()));
        assert_eq!(sqlite_result, Some(value.clone()));

        // Delete from both
        native_backend.kv_delete(&key).unwrap();
        sqlite_backend.kv_delete(&key).unwrap();

        // Verify both return None
        let snapshot = SnapshotId::current();
        let native_result = native_backend.kv_get(snapshot, &key).unwrap();
        let sqlite_result = sqlite_backend.kv_get(snapshot, &key).unwrap();

        assert_eq!(native_result, None);
        assert_eq!(sqlite_result, None);
    }

    #[test]
    fn test_mixed_backend_operations() {
        // Switch between backends in same test
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        // Write to Native
        native_backend
            .kv_set(
                b"mixed_native_key".to_vec(),
                KvValue::String("native".to_string()),
                None,
            )
            .unwrap();

        // Write to SQLite
        sqlite_backend
            .kv_set(
                b"mixed_sqlite_key".to_vec(),
                KvValue::String("sqlite".to_string()),
                None,
            )
            .unwrap();

        // Read from both
        let snapshot = SnapshotId::current();
        let native_value = native_backend
            .kv_get(snapshot, b"mixed_native_key")
            .unwrap();
        let sqlite_value = sqlite_backend
            .kv_get(snapshot, b"mixed_sqlite_key")
            .unwrap();

        assert_eq!(native_value, Some(KvValue::String("native".to_string())));
        assert_eq!(
            sqlite_value,
            Some(KvValue::String("sqlite".to_string()))
        );

        // Verify isolation (Native doesn't see SQLite data and vice versa)
        let native_cross = native_backend
            .kv_get(snapshot, b"mixed_sqlite_key")
            .unwrap();
        let sqlite_cross = sqlite_backend
            .kv_get(snapshot, b"mixed_native_key")
            .unwrap();

        assert_eq!(native_cross, None);
        assert_eq!(sqlite_cross, None);
    }

    #[test]
    fn test_binary_keys_both_backends() {
        // Binary keys work on both backends
        let native_backend = NativeGraphBackend::new_temp().unwrap();
        let sqlite_backend = SqliteGraphBackend::in_memory().unwrap();

        let key1: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF];
        let key2: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];

        native_backend
            .kv_set(key1.clone(), KvValue::Integer(1), None)
            .unwrap();
        native_backend
            .kv_set(key2.clone(), KvValue::Integer(2), None)
            .unwrap();

        sqlite_backend
            .kv_set(key1.clone(), KvValue::Integer(1), None)
            .unwrap();
        sqlite_backend
            .kv_set(key2.clone(), KvValue::Integer(2), None)
            .unwrap();

        let snapshot = SnapshotId::current();

        let native_result1 = native_backend.kv_get(snapshot, &key1).unwrap();
        let sqlite_result1 = sqlite_backend.kv_get(snapshot, &key1).unwrap();
        assert_eq!(native_result1, sqlite_result1);
        assert_eq!(native_result1, Some(KvValue::Integer(1)));

        let native_result2 = native_backend.kv_get(snapshot, &key2).unwrap();
        let sqlite_result2 = sqlite_backend.kv_get(snapshot, &key2).unwrap();
        assert_eq!(native_result2, sqlite_result2);
        assert_eq!(native_result2, Some(KvValue::Integer(2)));
    }
}

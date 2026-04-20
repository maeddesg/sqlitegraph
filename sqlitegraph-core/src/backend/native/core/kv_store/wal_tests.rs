//! WAL integration tests for KV store
//!
//! Comprehensive tests for KV store WAL integration including serialization,
//! recovery scenarios, and edge cases.

#[cfg(test)]
mod tests {
    use crate::backend::native::v2::kv_store::store::KvStore;
    use crate::backend::native::v2::kv_store::types::KvValue;
    use crate::backend::native::v2::kv_store::wal::{
        apply_delete, apply_set, deserialize_value, get_value_type_tag, serialize_value,
    };
    use crate::backend::native::v2::wal::record::{V2WALRecord, V2WALSerializer};
    use std::time::SystemTime;

    /// Helper to get current time in seconds
    fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    #[test]
    fn test_kv_set_serialization_roundtrip() {
        let record = V2WALRecord::KvSet {
            key: b"test_key".to_vec(),
            value_bytes: b"test_value".to_vec(),
            value_type: 0,
            ttl_seconds: Some(3600),
            version: 12345,
        };

        let serialized = V2WALSerializer::serialize(&record).unwrap();
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match (record, deserialized) {
            (
                V2WALRecord::KvSet {
                    key: k1,
                    value_bytes: v1,
                    value_type: t1,
                    ttl_seconds: ttl1,
                    version: ver1,
                },
                V2WALRecord::KvSet {
                    key: k2,
                    value_bytes: v2,
                    value_type: t2,
                    ttl_seconds: ttl2,
                    version: ver2,
                },
            ) => {
                assert_eq!(k1, k2);
                assert_eq!(v1, v2);
                assert_eq!(t1, t2);
                assert_eq!(ttl1, ttl2);
                assert_eq!(ver1, ver2);
            }
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_kv_delete_serialization_roundtrip() {
        let record = V2WALRecord::KvDelete {
            key: b"test_key".to_vec(),
            old_value_bytes: Some(b"old_value".to_vec()),
            old_value_type: 1,
            old_version: 12344,
        };

        let serialized = V2WALSerializer::serialize(&record).unwrap();
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match (record, deserialized) {
            (
                V2WALRecord::KvDelete {
                    key: k1,
                    old_value_bytes: v1,
                    old_value_type: t1,
                    old_version: ver1,
                },
                V2WALRecord::KvDelete {
                    key: k2,
                    old_value_bytes: v2,
                    old_value_type: t2,
                    old_version: ver2,
                },
            ) => {
                assert_eq!(k1, k2);
                assert_eq!(v1, v2);
                assert_eq!(t1, t2);
                assert_eq!(ver1, ver2);
            }
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_kv_set_with_ttl() {
        let record = V2WALRecord::KvSet {
            key: b"ttl_key".to_vec(),
            value_bytes: b"ttl_value".to_vec(),
            value_type: 0,
            ttl_seconds: Some(7200),
            version: 1,
        };

        let serialized = V2WALSerializer::serialize(&record).unwrap();
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match deserialized {
            V2WALRecord::KvSet { ttl_seconds, .. } => {
                assert_eq!(ttl_seconds, Some(7200));
            }
            _ => panic!("Wrong record type"),
        }
    }

    #[test]
    fn test_kv_set_all_value_types() {
        let test_cases = vec![
            (KvValue::Bytes(vec![1, 2, 3]), 0u8),
            (KvValue::String("test".to_string()), 1u8),
            (KvValue::Integer(42), 2u8),
            (KvValue::Float(3.14), 3u8),
            (KvValue::Boolean(true), 4u8),
            (KvValue::Boolean(false), 4u8),
        ];

        for (value, expected_type) in test_cases {
            let type_tag = get_value_type_tag(&value);
            assert_eq!(type_tag, expected_type);

            let serialized = serialize_value(&value).unwrap();
            let deserialized = deserialize_value(&serialized, type_tag).unwrap();
            assert_eq!(value, deserialized);
        }
    }

    #[test]
    fn test_write_set_to_wal() {
        let key = b"wal_test_key".to_vec();
        let value = KvValue::String("wal_test_value".to_string());
        let value_bytes = serialize_value(&value).unwrap();
        let value_type = get_value_type_tag(&value);
        let ttl = Some(3600);
        let version = 100;

        let record = V2WALRecord::KvSet {
            key: key.clone(),
            value_bytes: value_bytes.clone(),
            value_type,
            ttl_seconds: ttl,
            version,
        };

        let serialized = V2WALSerializer::serialize(&record).unwrap();

        // Verify the record can be deserialized
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match deserialized {
            V2WALRecord::KvSet {
                key: k,
                value_bytes: vb,
                value_type: vt,
                ttl_seconds: t,
                version: v,
            } => {
                assert_eq!(k, key);
                assert_eq!(vb, value_bytes);
                assert_eq!(vt, value_type);
                assert_eq!(t, ttl);
                assert_eq!(v, version);
            }
            _ => panic!("Wrong record type"),
        }
    }

    #[test]
    fn test_write_delete_to_wal() {
        let key = b"delete_test_key".to_vec();
        let old_value = KvValue::Integer(12345);
        let old_value_bytes = serialize_value(&old_value).unwrap();
        let old_value_type = get_value_type_tag(&old_value);
        let old_version = 99;

        let record = V2WALRecord::KvDelete {
            key: key.clone(),
            old_value_bytes: Some(old_value_bytes.clone()),
            old_value_type,
            old_version,
        };

        let serialized = V2WALSerializer::serialize(&record).unwrap();

        // Verify the record can be deserialized
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match deserialized {
            V2WALRecord::KvDelete {
                key: k,
                old_value_bytes: ovb,
                old_value_type: ovt,
                old_version: ov,
            } => {
                assert_eq!(k, key);
                assert_eq!(ovb, Some(old_value_bytes));
                assert_eq!(ovt, old_value_type);
                assert_eq!(ov, old_version);
            }
            _ => panic!("Wrong record type"),
        }
    }

    #[test]
    fn test_value_serialization_roundtrip() {
        let values = vec![
            KvValue::Bytes(vec![1, 2, 3, 4, 5]),
            KvValue::String("Hello, World!".to_string()),
            KvValue::Integer(-9876543210),
            KvValue::Float(2.718281828459045),
            KvValue::Boolean(true),
            KvValue::Json(serde_json::json!({"array": [1, 2, 3], "nested": {"key": "value"}})),
        ];

        for value in values {
            let type_tag = get_value_type_tag(&value);
            let serialized = serialize_value(&value).unwrap();
            let deserialized = deserialize_value(&serialized, type_tag).unwrap();
            assert_eq!(value, deserialized);
        }
    }

    #[test]
    fn test_recovery_from_empty() {
        let mut store = KvStore::new();

        // Simulate WAL replay - apply a set operation
        let key = b"recovered_key".to_vec();
        let value = KvValue::String("recovered_value".to_string());
        let value_bytes = serialize_value(&value).unwrap();
        let value_type = get_value_type_tag(&value);
        let ttl = None;
        let version = 1;

        apply_set(
            &mut store,
            key.clone(),
            value_bytes,
            value_type,
            ttl,
            version,
        )
        .unwrap();

        // Verify the entry was restored
        let result = store.get(&key).unwrap();
        assert_eq!(result, Some(value));
    }

    #[test]
    fn test_recovery_from_existing() {
        let mut store = KvStore::new();

        // Create an initial entry
        let key = b"update_key".to_vec();
        store.set(key.clone(), KvValue::Integer(100), None).unwrap();

        // Simulate WAL replay - update the entry
        let value = KvValue::Integer(200);
        let value_bytes = serialize_value(&value).unwrap();
        let value_type = get_value_type_tag(&value);
        let ttl = None;
        let version = 2;

        apply_set(
            &mut store,
            key.clone(),
            value_bytes,
            value_type,
            ttl,
            version,
        )
        .unwrap();

        // Verify the entry was updated
        let result = store.get(&key).unwrap();
        assert_eq!(result, Some(KvValue::Integer(200)));
    }

    #[test]
    fn test_recovery_with_deletes() {
        let mut store = KvStore::new();

        // Create some entries
        let key1 = b"key1".to_vec();
        let key2 = b"key2".to_vec();
        store
            .set(key1.clone(), KvValue::String("value1".to_string()), None)
            .unwrap();
        store
            .set(key2.clone(), KvValue::String("value2".to_string()), None)
            .unwrap();

        // Verify they exist
        assert!(store.get(&key1).unwrap().is_some());
        assert!(store.get(&key2).unwrap().is_some());

        // Simulate WAL replay - delete key1
        apply_delete(&mut store, key1.clone(), 1).unwrap();

        // Verify key1 is gone, key2 still exists
        assert_eq!(store.get(&key1).unwrap(), None);
        assert!(store.get(&key2).unwrap().is_some());
    }

    #[test]
    fn test_recovery_with_ttl() {
        let mut store = KvStore::new();

        // Simulate WAL replay - apply entry with TTL
        let key = b"ttl_key".to_vec();
        let value = KvValue::String("expires_soon".to_string());
        let value_bytes = serialize_value(&value).unwrap();
        let value_type = get_value_type_tag(&value);
        let ttl = Some(1); // 1 second TTL
        let version = 1;

        apply_set(
            &mut store,
            key.clone(),
            value_bytes,
            value_type,
            ttl,
            version,
        )
        .unwrap();

        // Verify the entry exists and can be retrieved
        let result = store.get(&key).unwrap();
        assert_eq!(result, Some(value));
    }

    #[test]
    fn test_empty_key_wal() {
        let record = V2WALRecord::KvSet {
            key: Vec::<u8>::new(),
            value_bytes: vec![1, 2, 3],
            value_type: 0,
            ttl_seconds: None,
            version: 1,
        };

        // Empty keys should serialize/deserialize correctly
        let serialized = V2WALSerializer::serialize(&record).unwrap();
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match deserialized {
            V2WALRecord::KvSet { key, .. } => {
                assert_eq!(key, Vec::<u8>::new());
            }
            _ => panic!("Wrong record type"),
        }
    }

    #[test]
    fn test_large_value_wal() {
        // Create a 1MB value
        let large_value = vec![0xAA; 1_048_576];

        let record = V2WALRecord::KvSet {
            key: b"large_key".to_vec(),
            value_bytes: large_value.clone(),
            value_type: 0,
            ttl_seconds: None,
            version: 1,
        };

        // Large values should serialize/deserialize correctly
        let serialized = V2WALSerializer::serialize(&record).unwrap();
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match deserialized {
            V2WALRecord::KvSet { value_bytes, .. } => {
                assert_eq!(value_bytes.len(), 1_048_576);
                assert_eq!(value_bytes, large_value);
            }
            _ => panic!("Wrong record type"),
        }
    }

    #[test]
    fn test_delete_with_no_old_value() {
        let record = V2WALRecord::KvDelete {
            key: b"no_old_value".to_vec(),
            old_value_bytes: None,
            old_value_type: 0,
            old_version: 0,
        };

        let serialized = V2WALSerializer::serialize(&record).unwrap();
        let deserialized = V2WALSerializer::deserialize(&serialized).unwrap();

        match deserialized {
            V2WALRecord::KvDelete {
                old_value_bytes, ..
            } => {
                assert_eq!(old_value_bytes, None);
            }
            _ => panic!("Wrong record type"),
        }
    }
}

//! KV store WAL integration
//!
//! This module provides helpers for integrating KvStore with the WAL system,
//! including serialization/deserialization, WAL logging, and recovery.

use crate::backend::native::v2::kv_store::store::KvStore;
use crate::backend::native::v2::kv_store::types::{KvStoreError, KvValue};
use crate::backend::native::v2::wal::record::V2WALRecord;
use crate::backend::native::NativeBackendError;
use serde_json;
use std::sync::Arc;

/// Value type tags for WAL serialization
pub const VALUE_TYPE_BYTES: u8 = 0;
pub const VALUE_TYPE_STRING: u8 = 1;
pub const VALUE_TYPE_INTEGER: u8 = 2;
pub const VALUE_TYPE_FLOAT: u8 = 3;
pub const VALUE_TYPE_BOOLEAN: u8 = 4;
pub const VALUE_TYPE_JSON: u8 = 5;

/// Convert KvValue to serialized bytes with type tag
pub fn serialize_value(value: &KvValue) -> Result<Vec<u8>, KvStoreError> {
    match value {
        KvValue::Bytes(data) => Ok(data.clone()),
        KvValue::String(s) => Ok(s.as_bytes().to_vec()),
        KvValue::Integer(n) => Ok(n.to_le_bytes().to_vec()),
        KvValue::Float(f) => Ok(f.to_le_bytes().to_vec()),
        KvValue::Boolean(b) => Ok(vec![*b as u8]),
        KvValue::Json(v) => serde_json::to_vec(v)
            .map_err(|e| KvStoreError::SerializationError(format!("JSON serialization failed: {}", e))),
    }
}

/// Convert serialized bytes back to KvValue using type tag
pub fn deserialize_value(bytes: &[u8], type_tag: u8) -> Result<KvValue, KvStoreError> {
    match type_tag {
        VALUE_TYPE_BYTES => Ok(KvValue::Bytes(bytes.to_vec())),
        VALUE_TYPE_STRING => {
            String::from_utf8(bytes.to_vec())
                .map(KvValue::String)
                .map_err(|e| KvStoreError::DeserializationError(format!("Invalid UTF-8: {}", e)))
        }
        VALUE_TYPE_INTEGER => {
            if bytes.len() != 8 {
                return Err(KvStoreError::DeserializationError(
                    format!("Invalid integer length: {}", bytes.len())
                ));
            }
            let val = i64::from_le_bytes(bytes.try_into().unwrap());
            Ok(KvValue::Integer(val))
        }
        VALUE_TYPE_FLOAT => {
            if bytes.len() != 8 {
                return Err(KvStoreError::DeserializationError(
                    format!("Invalid float length: {}", bytes.len())
                ));
            }
            let val = f64::from_le_bytes(bytes.try_into().unwrap());
            Ok(KvValue::Float(val))
        }
        VALUE_TYPE_BOOLEAN => {
            if bytes.len() != 1 {
                return Err(KvStoreError::DeserializationError(
                    format!("Invalid boolean length: {}", bytes.len())
                ));
            }
            Ok(KvValue::Boolean(bytes[0] != 0))
        }
        VALUE_TYPE_JSON => {
            serde_json::from_slice(bytes)
                .map(KvValue::Json)
                .map_err(|e| KvStoreError::DeserializationError(format!("JSON deserialization failed: {}", e)))
        }
        _ => Err(KvStoreError::DeserializationError(
            format!("Unknown value type tag: {}", type_tag)
        )),
    }
}

/// Get the type tag for a KvValue
pub fn get_value_type_tag(value: &KvValue) -> u8 {
    match value {
        KvValue::Bytes(_) => VALUE_TYPE_BYTES,
        KvValue::String(_) => VALUE_TYPE_STRING,
        KvValue::Integer(_) => VALUE_TYPE_INTEGER,
        KvValue::Float(_) => VALUE_TYPE_FLOAT,
        KvValue::Boolean(_) => VALUE_TYPE_BOOLEAN,
        KvValue::Json(_) => VALUE_TYPE_JSON,
    }
}

/// Apply a KvSet WAL record to a KvStore (used during recovery)
///
/// This bypasses the normal WAL write path to avoid infinite recursion during replay.
pub fn apply_set(
    store: &mut KvStore,
    key: Vec<u8>,
    value_bytes: Vec<u8>,
    value_type: u8,
    ttl_seconds: Option<u64>,
    version: u64,
) -> Result<(), KvStoreError> {
    // Deserialize value from bytes
    let value = deserialize_value(&value_bytes, value_type)?;

    // Use set_with_version to apply directly to store entries
    store.set_with_version(key, value, ttl_seconds, version)?;

    Ok(())
}

/// Apply a KvDelete WAL record to a KvStore (used during recovery)
///
/// This bypasses the normal WAL write path to avoid infinite recursion during replay.
pub fn apply_delete(
    store: &mut KvStore,
    key: Vec<u8>,
    _old_version: u64,
) -> Result<(), KvStoreError> {
    // Delete directly from store
    store.delete(&key)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_bytes() {
        let original = KvValue::Bytes(vec![1, 2, 3, 4, 5]);
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_string() {
        let original = KvValue::String("hello world".to_string());
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_integer() {
        let original = KvValue::Integer(-12345);
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_float() {
        let original = KvValue::Float(3.14159);
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_boolean() {
        let original = KvValue::Boolean(true);
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);

        let original = KvValue::Boolean(false);
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_json() {
        let original = KvValue::Json(serde_json::json!({"key": "value", "number": 42}));
        let tag = get_value_type_tag(&original);
        let serialized = serialize_value(&original).unwrap();
        let deserialized = deserialize_value(&serialized, tag).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_apply_set_creates_entry() {
        let mut store = KvStore::new();

        let key = b"test_key".to_vec();
        let value_bytes = b"test_value".to_vec();
        let value_type = VALUE_TYPE_BYTES;
        let ttl_seconds = Some(3600);
        let version = 12345;

        apply_set(&mut store, key.clone(), value_bytes, value_type, ttl_seconds, version).unwrap();

        let result = store.get(&key).unwrap();
        assert_eq!(result, Some(KvValue::Bytes(b"test_value".to_vec())));
    }

    #[test]
    fn test_apply_delete_removes_entry() {
        let mut store = KvStore::new();

        // First create an entry
        let key = b"test_key".to_vec();
        store.set(key.clone(), KvValue::Bytes(b"test_value".to_vec()), None).unwrap();

        // Verify it exists
        assert!(store.get(&key).unwrap().is_some());

        // Apply delete
        apply_delete(&mut store, key.clone(), 1).unwrap();

        // Verify it's gone
        assert_eq!(store.get(&key).unwrap(), None);
    }

    #[test]
    fn test_invalid_type_tag() {
        let result = deserialize_value(b"some data", 99);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_utf8_string() {
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
        let result = deserialize_value(&invalid_utf8, VALUE_TYPE_STRING);
        assert!(result.is_err());
    }
}

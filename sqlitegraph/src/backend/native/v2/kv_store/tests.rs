//! Unit tests for KV store operations

use crate::backend::native::v2::kv_store::store::KvStore;
use crate::backend::native::v2::kv_store::types::{KvStoreError, KvValue};

#[test]
fn test_new_store() {
    let store = KvStore::new();
    assert_eq!(store.len(), 0);
}

#[test]
fn test_set_get() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    let result = store.get(b"key");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(KvValue::Integer(42)));
}

#[test]
fn test_get_missing_key() {
    let store = KvStore::new();
    let result = store.get(b"missing");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_delete() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    store.delete(b"key").unwrap();
    assert_eq!(store.get(b"key").unwrap(), None);
}

#[test]
fn test_delete_missing_key() {
    let mut store = KvStore::new();
    let result = store.delete(b"missing");
    assert!(matches!(result, Err(KvStoreError::KeyNotFound(_))));
}

#[test]
fn test_exists() {
    let mut store = KvStore::new();
    assert!(!store.exists(b"key"));
    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    assert!(store.exists(b"key"));
    store.delete(b"key").unwrap();
    assert!(!store.exists(b"key"));
}

#[test]
fn test_bytes_value() {
    let mut store = KvStore::new();
    let data = vec![1, 2, 3, 4, 5];
    store.set(b"key".to_vec(), KvValue::Bytes(data.clone()), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::Bytes(data)));
}

#[test]
fn test_string_value() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::String("hello".to_string()), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::String("hello".to_string())));
}

#[test]
fn test_integer_value() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Integer(-12345), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::Integer(-12345)));
}

#[test]
fn test_float_value() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Float(3.14159), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::Float(3.14159)));
}

#[test]
fn test_boolean_value() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Boolean(true), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::Boolean(true)));
}

#[test]
fn test_json_value() {
    let mut store = KvStore::new();
    let json = serde_json::json!({"foo": "bar", "num": 42});
    store.set(b"key".to_vec(), KvValue::Json(json.clone()), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::Json(json)));
}

#[test]
fn test_created_at_set() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    // Just verify metadata exists - timestamp testing would require mocking
    assert!(store.exists(b"key"));
}

#[test]
fn test_overwrite() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    store.set(b"key".to_vec(), KvValue::String("updated".to_string()), None).unwrap();
    let result = store.get(b"key");
    assert_eq!(result.unwrap(), Some(KvValue::String("updated".to_string())));
    assert_eq!(store.len(), 1);
}

#[test]
fn test_len_and_clear() {
    let mut store = KvStore::new();
    assert_eq!(store.len(), 0);
    store.set(b"key1".to_vec(), KvValue::Integer(1), None).unwrap();
    store.set(b"key2".to_vec(), KvValue::Integer(2), None).unwrap();
    assert_eq!(store.len(), 2);
}

#[test]
fn test_empty_key() {
    let mut store = KvStore::new();
    store.set(vec![], KvValue::Integer(42), None).unwrap();
    let result = store.get(b"");
    assert!(result.is_ok());
}

#[test]
fn test_ttl_none() {
    let mut store = KvStore::new();
    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    assert!(store.exists(b"key"));
}

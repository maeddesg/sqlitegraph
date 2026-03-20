//! V3 Native KV Store - Core Types
//!
//! This module defines the types for V3's native key-value storage,
//! designed to integrate with V3's page-based architecture and WAL.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Value types supported by the KV store
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KvValue {
    /// Null value
    Null,
    /// 64-bit signed integer
    Integer(i64),
    /// 64-bit floating point
    Float(f64),
    /// UTF-8 string
    String(String),
    /// Boolean
    Boolean(bool),
    /// Raw bytes
    Bytes(Vec<u8>),
    /// JSON value
    Json(serde_json::Value),
}

impl KvValue {
    /// Get the type tag for WAL serialization
    pub fn type_tag(&self) -> u8 {
        match self {
            KvValue::Null => 0,
            KvValue::Integer(_) => 1,
            KvValue::Float(_) => 2,
            KvValue::String(_) => 3,
            KvValue::Boolean(_) => 4,
            KvValue::Bytes(_) => 5,
            KvValue::Json(_) => 6,
        }
    }

    /// Serialize value to bytes for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            KvValue::Null => vec![],
            KvValue::Integer(v) => v.to_le_bytes().to_vec(),
            KvValue::Float(v) => v.to_le_bytes().to_vec(),
            KvValue::String(s) => s.as_bytes().to_vec(),
            KvValue::Boolean(b) => vec![if *b { 1 } else { 0 }],
            KvValue::Bytes(b) => b.clone(),
            KvValue::Json(v) => serde_json::to_vec(v).unwrap_or_default(),
        }
    }

    /// Deserialize value from bytes using type tag
    pub fn from_bytes(bytes: &[u8], type_tag: u8) -> Option<Self> {
        match type_tag {
            0 => Some(KvValue::Null),
            1 if bytes.len() >= 8 => {
                let val = i64::from_le_bytes(bytes[0..8].try_into().ok()?);
                Some(KvValue::Integer(val))
            }
            2 if bytes.len() >= 8 => {
                let val = f64::from_le_bytes(bytes[0..8].try_into().ok()?);
                Some(KvValue::Float(val))
            }
            3 => String::from_utf8(bytes.to_vec()).ok().map(KvValue::String),
            4 if !bytes.is_empty() => Some(KvValue::Boolean(bytes[0] != 0)),
            5 => Some(KvValue::Bytes(bytes.to_vec())),
            6 => serde_json::from_slice(bytes).ok().map(KvValue::Json),
            _ => None,
        }
    }
}

/// Metadata for KV entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvMetadata {
    /// Creation timestamp (Unix epoch seconds)
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
    /// TTL in seconds (None = no expiration)
    pub ttl_seconds: Option<u64>,
    /// Version (LSN from WAL)
    pub version: u64,
}

impl KvMetadata {
    /// Create new metadata with current timestamp
    pub fn new(version: u64, ttl_seconds: Option<u64>) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            created_at: now,
            updated_at: now,
            ttl_seconds,
            version,
        }
    }

    /// Create metadata for recovery (preserves original timestamps)
    pub fn for_recovery(
        created_at: u64,
        updated_at: u64,
        ttl_seconds: Option<u64>,
        version: u64,
    ) -> Self {
        Self {
            created_at,
            updated_at,
            ttl_seconds,
            version,
        }
    }

    /// Check if entry is expired at given timestamp
    pub fn is_expired_at(&self, now: u64) -> bool {
        match self.ttl_seconds {
            // Use >= so that TTL=0 means immediately expired
            Some(ttl) => now >= self.updated_at.saturating_add(ttl),
            None => false,
        }
    }
}

/// A versioned KV entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvEntry {
    /// Key bytes
    pub key: Vec<u8>,
    /// Value
    pub value: KvValue,
    /// Entry metadata
    pub metadata: KvMetadata,
}

impl KvEntry {
    /// Create a new KV entry
    pub fn new(key: Vec<u8>, value: KvValue, version: u64, ttl_seconds: Option<u64>) -> Self {
        Self {
            key,
            value,
            metadata: KvMetadata::new(version, ttl_seconds),
        }
    }

    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.metadata.is_expired_at(now)
    }
}

/// Hash a key for B+Tree indexing
pub fn hash_key(key: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_value_roundtrip() {
        let values = vec![
            KvValue::Null,
            KvValue::Integer(42),
            KvValue::Integer(-123456789),
            KvValue::Float(3.14159),
            KvValue::String("hello world".to_string()),
            KvValue::Boolean(true),
            KvValue::Boolean(false),
            KvValue::Bytes(vec![0x01, 0x02, 0x03]),
            KvValue::Json(serde_json::json!({"key": "value", "num": 123})),
        ];

        for original in values {
            let type_tag = original.type_tag();
            let bytes = original.to_bytes();
            let recovered = KvValue::from_bytes(&bytes, type_tag);
            assert_eq!(
                Some(original),
                recovered,
                "Roundtrip failed for type_tag {}",
                type_tag
            );
        }
    }

    #[test]
    fn test_kv_metadata_expiration() {
        let meta = KvMetadata::new(1, Some(60)); // 60 second TTL
        assert!(!meta.is_expired_at(meta.updated_at + 30)); // Not expired after 30s
        assert!(meta.is_expired_at(meta.updated_at + 61)); // Expired after 61s
    }

    #[test]
    fn test_kv_metadata_no_expiration() {
        let meta = KvMetadata::new(1, None); // No TTL
        assert!(!meta.is_expired_at(u64::MAX)); // Never expires
    }

    #[test]
    fn test_key_hashing() {
        let key1 = b"test_key_1";
        let key2 = b"test_key_2";

        let hash1 = hash_key(key1);
        let hash1_dup = hash_key(key1);
        let hash2 = hash_key(key2);

        assert_eq!(hash1, hash1_dup, "Same key should produce same hash");
        assert_ne!(
            hash1, hash2,
            "Different keys should produce different hashes"
        );
    }

    #[test]
    fn test_kv_entry_expiration() {
        let entry = KvEntry::new(b"key".to_vec(), KvValue::Integer(1), 1, Some(0)); // Expired immediately
        assert!(entry.is_expired(), "Entry with 0 TTL should be expired");
    }
}

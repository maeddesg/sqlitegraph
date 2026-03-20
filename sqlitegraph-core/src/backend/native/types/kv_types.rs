//! Key-Value store types (migrated from V2 backend)
//!
//! These types are used across multiple backends (SQLite, V3) for KV operations.
//! Originally part of the V2 backend, they have been migrated to a shared location
//! for continued use after V2 removal.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Value types supported by the KV store
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KvValue {
    Bytes(Vec<u8>),
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Json(serde_json::Value),
}

/// Metadata for a KV entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvMetadata {
    pub created_at: u64,
    pub updated_at: u64,
    pub ttl_seconds: Option<u64>,
    pub version: u64,
}

/// A key-value entry with metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvEntry {
    pub key: Vec<u8>,
    pub value: KvValue,
    pub metadata: KvMetadata,
}

/// KV store errors
#[derive(Debug, Error, Clone, PartialEq)]
pub enum KvStoreError {
    #[error("Key not found: {0:?}")]
    KeyNotFound(Vec<u8>),

    #[error("Expired key: {0:?}")]
    ExpiredKey(Vec<u8>),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Invalid TTL: {0}")]
    InvalidTtl(String),

    #[error("KV recovery failed: {0}")]
    RecoveryFailed(String),
}

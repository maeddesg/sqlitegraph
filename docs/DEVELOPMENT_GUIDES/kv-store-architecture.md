# KV Store Architecture

**Last Updated:** 2026-02-03
**Version:** v1.4.2

This guide explains the internal architecture of SQLiteGraph's transactional Key-Value store for Native V2 backend.

---

## Overview

The KV store is an **in-memory HashMap-based** storage system built as a VIEW over Native V2 storage. It uses the existing WAL infrastructure for persistence and provides true MVCC snapshot isolation.

### Key Characteristics

| Characteristic | Value |
|----------------|-------|
| **Storage** | In-memory HashMap |
| **Key Type** | `Vec<u8>` (binary keys) |
| **Value Types** | Bytes, String, Integer, Float, Boolean, JSON |
| **Isolation** | MVCC snapshot isolation |
| **TTL** | Lazy cleanup (no background threads) |
| **Persistence** | Via WAL (plan 02) |

---

## Architecture Principles

### Design Philosophy

```
KV Store is NOT a separate storage system.
It's a VIEW over Native V2 infrastructure:
- Uses WAL for durability
- Uses snapshot_id for versioning
- Uses commit LSN for version assignment
```

### Key Decisions

1. **No internal version counter**: Versions come from WAL commit LSN
2. **Byte keys**: `Vec<u8>` for maximum flexibility
3. **Typed values**: `KvValue` enum with JSON for complex data
4. **Lazy TTL**: Cleanup on read, no background threads
5. **Multi-version**: Full history retained per key

---

## Module Structure

```
src/backend/native/v2/kv_store/
├── mod.rs          # Module exports and documentation
├── store.rs        # KvStore implementation (HashMap-based)
├── types.rs        # Data structures (KvValue, KvEntry, KvMetadata)
├── ttl.rs          # TTL helpers and lazy cleanup
├── wal.rs          # WAL integration (serialization, recovery)
├── tests.rs        # Unit tests
├── wal_tests.rs    # WAL integration tests
├── integration_tests.rs  #[cfg(test)] - comprehensive integration tests
└── snapshot_tests.rs     #[cfg(test)] - snapshot isolation tests
```

---

## Data Structures

### KvStore (Main Structure)

Located in `store.rs`:

```rust
#[derive(Debug, Default)]
pub struct KvStore {
    /// Multi-version storage: key → version history
    /// Each Vec<KvEntry> is sorted by version (ascending LSN)
    pub(crate) entries: RwLock<HashMap<Vec<u8>, Vec<KvEntry>>>,
}
```

### KvEntry (Versioned Entry)

Located in `types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvEntry {
    /// The key (Vec<u8> for flexibility)
    pub key: Vec<u8>,

    /// The value (typed enum)
    pub value: KvValue,

    /// Metadata including version, timestamps, TTL
    pub metadata: KvMetadata,
}
```

### KvMetadata (Version Metadata)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvMetadata {
    /// Creation time (Unix timestamp)
    pub created_at: u64,

    /// Last update time (Unix timestamp)
    pub updated_at: u64,

    /// TTL in seconds (None = never expires)
    pub ttl_seconds: Option<u64>,

    /// Version number (from WAL commit LSN)
    pub version: u64,
}
```

### KvValue (Typed Values)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KvValue {
    /// Raw bytes
    Bytes(Vec<u8>),

    /// UTF-8 string
    String(String),

    /// 64-bit integer
    Integer(i64),

    /// 64-bit float
    Float(f64),

    /// Boolean value
    Boolean(bool),

    /// JSON for complex structured data
    Json(serde_json::Value),
}
```

---

## Core Operations

### Set (Insert/Update)

```rust
impl KvStore {
    /// Set a key-value pair with optional TTL
    ///
    /// Creates a new version in the history. If the key already exists,
    /// the new version is appended to the history.
    pub fn set(
        &self,
        key: Vec<u8>,
        value: KvValue,
        ttl_seconds: Option<u64>,
    ) -> Result<(), KvStoreError> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = KvEntry {
            key: key.clone(),
            value,
            metadata: KvMetadata {
                created_at: now,
                updated_at: now,
                ttl_seconds,
                version: 0, // Assigned by WAL on commit
            },
        };

        let mut entries = self.entries.write();
        entries.entry(key).or_default().push(entry);

        Ok(())
    }
}
```

### Get (Latest Version)

```rust
/// Get a value by key (latest committed version)
///
/// TTL is checked lazily: expired entries return None
pub fn get(&self, key: &[u8]) -> Result<Option<KvValue>, KvStoreError> {
    let entries = self.entries.read();

    if let Some(versions) = entries.get(key) {
        // Get latest version (last element in Vec)
        if let Some(entry) = versions.last() {
            if ttl::is_expired(entry) {
                return Ok(None); // Lazy TTL cleanup
            }
            return Ok(Some(entry.value.clone()));
        }
    }

    Ok(None)
}
```

### Get at Snapshot (MVCC)

```rust
/// Get a value at a specific snapshot (MVCC isolation)
///
/// Uses binary search to find the latest version with version <= snapshot_id
pub fn get_at_snapshot(
    &self,
    key: &[u8],
    snapshot_id: SnapshotId,
) -> Result<Option<KvValue>, KvStoreError> {
    let entries = self.entries.read();
    let snapshot_lsn = snapshot_id.as_lsn();

    if let Some(versions) = entries.get(key) {
        // Snapshot at 0 means "see all data"
        if snapshot_lsn == 0 {
            if let Some(entry) = versions.last() {
                if ttl::is_expired(entry) {
                    return Ok(None);
                }
                return Ok(Some(entry.value.clone()));
            }
            return Ok(None);
        }

        // Binary search for latest version with version <= snapshot_lsn
        let idx = versions.partition_point(|e| e.metadata.version <= snapshot_lsn);

        if idx == 0 {
            return Ok(None); // All versions are newer than snapshot
        }

        let entry = &versions[idx - 1];

        // Check TTL lazily
        if ttl::is_expired(entry) {
            return Ok(None);
        }

        return Ok(Some(entry.value.clone()));
    }

    Ok(None)
}
```

---

## TTL (Time-To-Live)

### Lazy Cleanup Strategy

The KV store uses **lazy TTL cleanup** - expiration is checked on read, not proactively by background threads.

### TTL Check

Located in `ttl.rs`:

```rust
/// Check if an entry is expired (TTL exceeded)
///
/// Entry is expired if: current_time > created_at + ttl_seconds
pub fn is_expired(entry: &KvEntry) -> bool {
    if let Some(ttl) = entry.metadata.ttl_seconds {
        if ttl == 0 {
            return true; // TTL of 0 means "already expired"
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Check if current time > expiration time
        let expiration_time = entry.metadata.created_at.saturating_add(ttl);
        now > expiration_time
    } else {
        false // No TTL set - entry never expires
    }
}
```

### Manual Cleanup (Optional)

```rust
/// Explicit cleanup of all expired entries
///
/// This is NOT called automatically - users must call it explicitly
/// if they want to reclaim space from expired entries.
pub fn cleanup_expired_entries(store: &mut KvStore) -> usize {
    let mut entries = store.entries.write();
    let mut total_removed = 0;

    // Remove keys where ALL versions are expired
    let keys_to_remove: Vec<Vec<u8>> = entries
        .iter()
        .filter(|(_, versions)| versions.iter().all(|v| is_expired(v)))
        .map(|(key, _)| key.clone())
        .collect();

    for key in keys_to_remove {
        let count = entries.remove(&key).map_or(0, |v| v.len());
        total_removed += count;
    }

    // Filter expired versions from remaining keys
    for versions in entries.values_mut() {
        let original_len = versions.len();
        versions.retain(|v| !is_expired(v));
        total_removed += original_len - versions.len();
    }

    total_removed
}
```

---

## WAL Integration

### WAL Record Types

The KV store uses two WAL record types:

| Record Type | Code | Description |
|-------------|------|-------------|
| `KvSet` | 31 | Key-value set operation |
| `KvDelete` | 32 | Key delete operation |

### Serialization

Located in `wal.rs`:

```rust
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
            .map_err(|e| KvStoreError::SerializationError(format!("JSON: {}", e))),
    }
}

/// Convert serialized bytes back to KvValue
pub fn deserialize_value(bytes: &[u8], type_tag: u8) -> Result<KvValue, KvStoreError> {
    match type_tag {
        VALUE_TYPE_BYTES => Ok(KvValue::Bytes(bytes.to_vec())),
        VALUE_TYPE_STRING => String::from_utf8(bytes.to_vec())
            .map(KvValue::String)
            .map_err(|e| KvStoreError::DeserializationError(format!("UTF-8: {}", e))),
        VALUE_TYPE_INTEGER => {
            if bytes.len() != 8 {
                return Err(KvStoreError::DeserializationError("Invalid integer length".into()));
            }
            let val = i64::from_le_bytes(bytes.try_into().unwrap());
            Ok(KvValue::Integer(val))
        }
        VALUE_TYPE_FLOAT => {
            if bytes.len() != 8 {
                return Err(KvStoreError::DeserializationError("Invalid float length".into()));
            }
            let val = f64::from_le_bytes(bytes.try_into().unwrap());
            Ok(KvValue::Float(val))
        }
        VALUE_TYPE_BOOLEAN => {
            if bytes.len() != 1 {
                return Err(KvStoreError::DeserializationError("Invalid boolean length".into()));
            }
            Ok(KvValue::Boolean(bytes[0] != 0))
        }
        VALUE_TYPE_JSON => serde_json::from_slice(bytes)
            .map(KvValue::Json)
            .map_err(|e| KvStoreError::DeserializationError(format!("JSON: {}", e))),
        _ => Err(KvStoreError::DeserializationError(format!("Unknown type: {}", type_tag))),
    }
}
```

### Recovery

```rust
/// Apply a KvSet WAL record during recovery
///
/// This bypasses the normal WAL write path to avoid infinite recursion
pub fn apply_set(
    store: &mut KvStore,
    key: Vec<u8>,
    value_bytes: Vec<u8>,
    value_type: u8,
    ttl_seconds: Option<u64>,
    version: u64,
) -> Result<(), KvStoreError> {
    let value = deserialize_value(&value_bytes, value_type)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry = KvEntry {
        key: key.clone(),
        value,
        metadata: KvMetadata {
            created_at: now,
            updated_at: now,
            ttl_seconds,
            version,
        },
    };

    let mut entries = store.entries.write();
    entries.entry(key).or_default().push(entry);

    Ok(())
}
```

---

## Snapshot Isolation

### MVCC Model

The KV store provides true MVCC (Multi-Version Concurrency Control):

```
Timeline:
LSN 10: SET key1 = "value1"  → version 10
LSN 20: SET key1 = "value2"  → version 20
LSN 30: SET key1 = "value3"  → version 30

Snapshot at LSN 25 sees: version 20 (value2)
Snapshot at LSN 35 sees: version 30 (value3)
```

### Version History Structure

```
key → [KvEntry(version=10), KvEntry(version=20), KvEntry(version=30)]
       ↑                         ↑                    ↑
       oldest                 middle               newest
```

### Binary Search for Snapshot

```rust
// versions is sorted by version (ascending LSN)
let idx = versions.partition_point(|e| e.metadata.version <= snapshot_lsn);

if idx == 0 {
    return None; // All versions are newer than snapshot
}

let entry = &versions[idx - 1]; // Latest visible version
```

---

## Query API Enhancements

### Prefix Scan

```rust
/// Scan all keys with a given prefix
///
/// Returns matching key-value pairs, sorted by key
pub fn prefix_scan(
    &self,
    snapshot_id: SnapshotId,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, KvValue)>, KvStoreError> {
    let entries = self.entries.read();
    let snapshot_lsn = snapshot_id.as_lsn();

    let mut results = Vec::new();

    for (key, versions) in entries.iter() {
        // Filter by prefix
        if !key.starts_with(prefix) {
            continue;
        }

        // Find version visible at snapshot
        let entry = self.find_version_at_snapshot(versions, snapshot_lsn);

        if let Some(entry) = entry {
            // Check TTL
            if !ttl::is_expired(entry) {
                results.push((key.clone(), entry.value.clone()));
            }
        }
    }

    // Sort results lexicographically
    results.sort_by_key(|(k, _)| k.clone());

    Ok(results)
}
```

---

## Performance Characteristics

### Time Complexity

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| `set()` | O(1) | HashMap insert |
| `get()` | O(1) | HashMap lookup |
| `get_at_snapshot()` | O(log V) | V = versions per key |
| `prefix_scan()` | O(K log V) | K = total keys |
| `delete()` | O(1) | Logical delete |

### Space Complexity

| Component | Space |
|-----------|-------|
| Base storage | O(K × V) | K = keys, V = avg versions |
| Per-version overhead | ~100 bytes | metadata + Vec overhead |
| HashMap overhead | ~2x base | Rust HashMap factor |

### Memory Estimates

| Keys | Avg Versions | Est. Memory |
|-------|--------------|-------------|
| 1,000 | 2 | ~500 KB |
| 100,000 | 5 | ~100 MB |
| 1,000,000 | 10 | ~2 GB |

---

## Testing

### Test Files

| File | Description |
|------|-------------|
| `kv_store/tests.rs` | Unit tests for basic operations |
| `kv_store/wal_tests.rs` | WAL integration tests |
| `kv_store/snapshot_tests.rs` | Snapshot isolation tests |
| `kv_store/integration_tests.rs` | Comprehensive integration tests |

### Key Test Scenarios

```rust
#[test]
fn test_set_get() {
    let store = KvStore::new();

    store.set(b"key".to_vec(), KvValue::Integer(42), None).unwrap();
    let value = store.get(b"key").unwrap();

    assert_eq!(value, Some(KvValue::Integer(42)));
}

#[test]
fn test_snapshot_isolation() {
    let store = KvStore::new();

    // Version 1
    store.set(b"key".to_vec(), KvValue::Integer(1), None).unwrap();

    // Create snapshot at version 1
    let snapshot1 = SnapshotId::from_lsn(1);

    // Version 2
    store.set(b"key".to_vec(), KvValue::Integer(2), None).unwrap();

    // Snapshot 1 should see version 1
    let value1 = store.get_at_snapshot(b"key", snapshot1).unwrap();
    assert_eq!(value1, Some(KvValue::Integer(1)));

    // Current should see version 2
    let value_curr = store.get(b"key").unwrap();
    assert_eq!(value_curr, Some(KvValue::Integer(2)));
}

#[test]
fn test_ttl_expiration() {
    let store = KvStore::new();

    // Set with TTL of 1 second
    store.set(b"key".to_vec(), KvValue::Integer(42), Some(1)).unwrap();

    // Wait for expiration
    std::thread::sleep(Duration::from_secs(2));

    // Should return None (lazy cleanup)
    let value = store.get(b"key").unwrap();
    assert_eq!(value, None);
}

#[test]
fn test_prefix_scan() {
    let store = KvStore::new();

    store.set(b"agent:123:state".to_vec(), KvValue::String("active".into()), None).unwrap();
    store.set(b"agent:123:meta".to_vec(), KvValue::String("worker".into()), None).unwrap();
    store.set(b"other:key".to_vec(), KvValue::Integer(0), None).unwrap();

    let results = store.prefix_scan(SnapshotId::current(), b"agent:123:");

    assert_eq!(results.len(), 2);
}
```

---

## Common Patterns

### Using as Secondary Index

```rust
// Store inverted index: kind_name → node_id
let index_key = format!("index:kind:{}:{}", node.kind, node.name);
graph.kv_set(
    index_key.as_bytes(),
    KvValue::Integer(node.id),
    None
)?;

// Query: find all nodes of kind "Class" starting with "Test"
let prefix = b"index:kind:Class:test_";
let results = graph.kv_prefix_scan(snapshot.id(), prefix)?;

for (key, value) in results {
    if let KvValue::Integer(node_id) = value {
        let node = graph.get_node(snapshot.id(), node_id)?;
        println!("Found: {:?}", node.name);
    }
}
```

### Agent Messaging

```rust
// Send message to agent-123
let msg_key = format!("agent_to:agent-123:{}", timestamp);
graph.kv_set(
    msg_key.as_bytes(),
    KvValue::String(message_content),
    Some(3600) // 1 hour TTL
)?;

// Agent receives messages
let prefix = b"agent_to:agent-123:";
let messages = graph.kv_prefix_scan(snapshot.id(), prefix)?;

for (key, value) in messages {
    println!("Message: {:?}", value);
}
```

### Caching with TTL

```rust
// Cache expensive computation result
let cache_key = format!("cache:compute:{}", hash);
graph.kv_set(
    cache_key.as_bytes(),
    KvValue::Json(serde_json::to_value(result)?),
    Some(300) // 5 minute TTL
)?;

// Try cache first
if let Some(KvValue::Json(cached)) = graph.kv_get(cache_key.as_bytes())? {
    return Ok(serde_json::from_value(cached)?);
}

// Cache miss - compute and store
```

---

## Troubleshooting

### Issue: Memory growing unbounded

**Symptoms:** Memory usage keeps increasing

**Causes:**
1. Too many versions retained per key
2. Expired entries not cleaned up

**Solutions:**
1. Call `cleanup_expired_entries()` periodically
2. Implement version truncation for long-running systems
3. Use shorter TTL values

### Issue: Slow prefix scan

**Symptoms:** `prefix_scan()` takes too long

**Causes:**
1. Too many keys in store
2. Linear scan through all keys

**Solutions:**
1. Use more specific prefixes
2. Consider adding secondary index structure
3. Partition data across multiple KV stores

### Issue: Stale data at snapshot

**Symptoms:** `get_at_snapshot()` returns unexpected data

**Causes:**
1. Snapshot ID is too old (before data was written)
2. Version numbering issue

**Solutions:**
1. Verify snapshot_id is from after the write
2. Check WAL commit LSN assignment
3. Use `SnapshotId::current()` for latest data

---

## References

- **Source:** `src/backend/native/v2/kv_store/` - Full implementation
- **Related:** `docs/DEVELOPMENT_GUIDES/query-api-enhancements.md` - Query API usage
- **User Docs:** `MANUAL.md` Section 13 - Key-Value Store
- **WAL:** `src/backend/native/v2/wal/` - WAL system

---

## Design Trade-offs

| Decision | Rationale | Trade-off |
|----------|-----------|-----------|
| In-memory HashMap | Fast lookups, simple code | Limited by RAM |
| Lazy TTL | No background threads | Stale entries until accessed |
| Full history | True MVCC | Unbounded memory growth |
| Binary keys | Maximum flexibility | No built-in string operations |
| Typed values | Rich data support | Serialization overhead |

---

## Future Enhancements

Potential areas for extension (not currently planned):

1. **Persistent backend** - Disk-based KV for large datasets
2. **Version truncation** - Automatic old version cleanup
3. **Prefix index (trie)** - O(prefix) instead of O(K) prefix scan
4. **Transaction support** - Multi-key atomic operations
5. **Compression** - Compress large values

Note: These are **not currently planned** - the KV store is focused on in-memory performance and simplicity.

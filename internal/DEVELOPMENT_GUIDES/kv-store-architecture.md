# KV Store Architecture

**Last Updated:** 2026-02-12  
**Version:** v2.0.0

This guide explains SQLiteGraph's transactional Key-Value store architecture. The KV store is available on **all backends** (SQLite, V3, and V2).

---

## Overview

The KV store provides a typed key-value interface with MVCC snapshot isolation and optional TTL. Each backend has its own implementation optimized for that storage layer.

### Backend Comparison

| Backend | Storage | Initialization | Persistence |
|---------|---------|----------------|-------------|
| **SQLite** | SQL table (`kv_store`) | On first use | SQLite durability |
| **Native V3** | In-memory HashMap | **Lazy** - created on first KV operation | WAL (optional) |
| **Native V2** | In-memory HashMap | On backend creation | WAL |

### Key Characteristics

| Characteristic | Value |
|----------------|-------|
| **Key Type** | `Vec<u8>` (binary keys) |
| **Value Types** | Null, Bytes, String, Integer, Float, Boolean, JSON |
| **Isolation** | MVCC snapshot isolation |
| **TTL** | Lazy cleanup (no background threads) |
| **Multi-version** | Full history retained per key (V3/V2) |

---

## Architecture by Backend

### V3 Backend (Lazy Initialization)

**New in v2.0.0:** V3 uses lazy initialization for zero overhead when unused.

```rust
pub struct V3Backend {
    // ... other fields ...
    /// KV store - only created when first accessed
    kv_store: RwLock<Option<KvStore>>,
}
```

**Benefits:**
- Zero memory overhead if you don't use KV
- No HashMap allocation for pure graph workloads
- First access has small initialization cost

**Example:**
```rust
let backend = V3Backend::create("data.graph")?;

// Before any KV operation:
assert!(!backend.is_kv_initialized());  // false

// First KV operation triggers initialization:
backend.kv_set_v3(b"key".to_vec(), KvValue::Integer(42), None);
assert!(backend.is_kv_initialized());   // true
```

### SQLite Backend

The SQLite backend stores KV data in a SQL table created on first use:

```sql
CREATE TABLE IF NOT EXISTS kv_store (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    ttl_seconds INTEGER,
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

**Benefits:**
- Debuggable with SQL queries
- Survives process restarts (persistent)
- ACID via SQLite transactions

### V2 Backend (Deprecated)

V2 KV store is always allocated (not lazy). This was the motivation for V3's lazy approach.

---

## Data Structures

### KvValue (All Backends)

```rust
pub enum KvValue {
    Null,                           // Deleted/tombstone marker
    Bytes(Vec<u8>),                // Raw binary
    String(String),                // UTF-8 text
    Integer(i64),                  // 64-bit signed
    Float(f64),                    // 64-bit float
    Boolean(bool),                 // true/false
    Json(serde_json::Value),       // Complex structures
}
```

### V3 KvStore Structure

```rust
pub struct KvStore {
    /// key_hash → version history (Vec sorted by version)
    entries: RwLock<HashMap<u64, Vec<KvEntry>>>,
}

pub struct KvEntry {
    pub key: Vec<u8>,               // Original key
    pub value: KvValue,             // Value (Null = tombstone)
    pub metadata: KvMetadata,       // Timestamps, TTL, version
}
```

---

## Core Operations

### Get (V3 Native)

```rust
use sqlitegraph::backend::native::v3::{V3Backend, KvValue};
use sqlitegraph::snapshot::SnapshotId;

let backend = V3Backend::create("data.graph")?;
let snapshot = SnapshotId::current();

// Returns Option<KvValue>
match backend.kv_get_v3(snapshot, b"my_key") {
    Some(KvValue::String(s)) => println!("Value: {}", s),
    Some(KvValue::Integer(n)) => println!("Number: {}", n),
    Some(KvValue::Json(j)) => println!("JSON: {:?}", j),
    Some(KvValue::Null) => println!("Deleted (tombstone)"),
    None => println!("Key not found"),
}
```

### Set (V3 Native)

```rust
use sqlitegraph::backend::native::v3::KvValue;

// Simple value
backend.kv_set_v3(
    b"counter".to_vec(),
    KvValue::Integer(42),
    None,  // No TTL
);

// JSON value
backend.kv_set_v3(
    b"config".to_vec(),
    KvValue::Json(json!({
        "theme": "dark",
        "notifications": true
    })),
    Some(3600),  // TTL: 1 hour
);
```

### Generic Trait Methods (All Backends)

```rust
use sqlitegraph::backend::{GraphBackend, KvValue};

fn increment_counter(backend: &dyn GraphBackend) -> Result<(), SqliteGraphError> {
    let snapshot = SnapshotId::current();
    
    // Get current value
    let current = backend.kv_get(snapshot, b"counter")?
        .and_then(|v| match v {
            KvValue::Integer(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0);
    
    // Increment and set
    backend.kv_set(
        b"counter".to_vec(),
        KvValue::Integer(current + 1),
        None,
    )?;
    
    Ok(())
}

// Works with any backend
increment_counter(&sqlite_backend)?;
increment_counter(&v3_backend)?;
```

---

## MVCC Snapshot Isolation

All backends support MVCC reads:

```rust
let snapshot = SnapshotId::current();

// This read sees data committed at or before the snapshot
let value = backend.kv_get(snapshot, b"key");

// Concurrent writes don't affect this snapshot
backend.kv_set(b"key".to_vec(), new_value, None)?;  // Newer version

// Original snapshot still sees old value
let value2 = backend.kv_get(snapshot, b"key");  // Same as value
```

---

## TTL (Time-To-Live)

Set optional expiration in seconds:

```rust
// Expires after 60 seconds
backend.kv_set_v3(
    b"session_token".to_vec(),
    KvValue::String("abc123".to_string()),
    Some(60),
)?;

// After 60 seconds, kv_get returns None (lazy cleanup)
thread::sleep(Duration::from_secs(61));
assert!(backend.kv_get_v3(snapshot, b"session_token").is_none());
```

**Note:** Expired entries are cleaned up lazily on read. No background thread.

---

## WAL Persistence (V3/V2)

V3 and V2 backends can persist KV changes via WAL:

```rust
// Create with WAL enabled
let backend = V3Backend::create_with_wal("data.graph", true)?;

// All kv_set/kv_delete operations are logged
backend.kv_set_v3(b"key".to_vec(), KvValue::Integer(1), None);

// Recovery on reopen reads WAL and replays KV operations
let backend = V3Backend::open("data.graph")?;  // KV restored from WAL
```

---

## Module Structure

### V3 KV Store

```
src/backend/native/v3/kv_store/
├── mod.rs          # Module exports
├── store.rs        # KvStore implementation
├── types.rs        # KvValue, KvEntry, KvMetadata
├── wal.rs          # WAL integration
└── tests.rs        # Unit tests (24 tests)
```

### SQLite KV Store

Integrated into `src/backend/sqlite/impl_.rs`:
- `ensure_kv_table()` - Creates SQL table
- `kv_get()` - SQL SELECT
- `kv_set()` - SQL INSERT/UPDATE
- `kv_delete()` - SQL DELETE

---

## Use Cases

### Session Storage

```rust
// Store user session with TTL
backend.kv_set(
    format!("session:{}", session_id).into_bytes(),
    KvValue::Json(json!({
        "user_id": user_id,
        "login_time": now(),
    })),
    Some(3600),  // 1 hour TTL
)?;
```

### Configuration Cache

```rust
// Cache computed configuration
let config = expensive_config_computation();
backend.kv_set(
    b"app:config".to_vec(),
    KvValue::Json(config),
    None,  // Never expires
)?;
```

### HNSW Vector Storage

V3 uses KV store for HNSW vector persistence:

```rust
// V3VectorStorage stores vectors as JSON in KV
let storage = backend.create_hnsw_storage("embeddings").unwrap();
// Keys: hnsw:embeddings:vector:{vector_id}
```

---

## Testing

```bash
# V3 KV tests
cargo test --features native-v3 --lib backend::native::v3::kv_store

# 24 tests covering:
# - Basic get/set/delete
# - Snapshot isolation
# - TTL expiration
# - Prefix scan
# - WAL recovery
```

---

## See Also

- [Pub/Sub Implementation](pubsub-implementation.md) - Events on KV changes
- [Architecture](../../ARCHITECTURE.md) - Backend comparison
- [API Reference](../../API.md) - KV API details

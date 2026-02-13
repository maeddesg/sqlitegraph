# Query API Enhancements Guide

**Last Updated:** 2026-02-12  
**Version:** v2.0.0

This guide explains the query API for efficient data retrieval and pattern-based filtering.

---

## Overview

The query API provides efficient ways to query data without full graph scans:

| API | Purpose | Backends |
|-----|---------|----------|
| `kv_prefix_scan()` | Scan KV store by key prefix | SQLite, V3 |
| `kv_get()` | Get single KV entry | SQLite, V3 |
| `kv_set()` | Set KV entry | SQLite, V3 |
| `query_nodes_by_kind()` | Find nodes by kind | SQLite, V3, V2 (deprecated) |
| `query_nodes_by_name_pattern()` | Find nodes matching glob | SQLite, V3, V2 (deprecated) |

---

## Backend Support

| Backend | KV Store | Pub/Sub | Query Methods |
|---------|----------|---------|---------------|
| **SQLite** | ✅ SQL table | ✅ In-memory | Full support |
| **Native V3** | ✅ KV module | ✅ In-memory | Full support |
| **Native V2** | ✅ In-memory | ❌ | ⚠️ Deprecated |

---

## KV Store API

### Purpose

Key-Value storage for auxiliary data:
- **Session state**: Store agent state between operations
- **Secondary indexes**: Custom indexes not in core graph
- **Configuration**: Store algorithm parameters
- **Caching**: Cache expensive computations

### V3 Implementation

Located in `src/backend/v3/kv_store.rs`:

```rust
/// V3 KV store - lazily initialized
pub struct KvStore {
    data: RwLock<HashMap<Vec<u8>, KvEntry>>,
}

pub fn kv_get_v3(&self, key: &[u8]) -> Option<KvValue> {
    let kv = self.get_or_init_kv();
    kv.get(key)
}

pub fn kv_set_v3(
    &self,
    key: Vec<u8>,
    value: KvValue,
    expiry: Option<Instant>,
    ttl_version: u64,
) {
    let kv = self.get_or_init_kv();
    kv.set(key, value, expiry, ttl_version);
}
```

**Lazy Initialization:**
- KV store is `None` until first write
- First write initializes the store
- Small graphs pay no KV overhead

### SQLite Implementation

Located in `src/backend/sqlite/impl_.rs`:

```rust
fn kv_get(&self, key: &[u8]) -> Result<Option<KvValue>, SqliteGraphError> {
    let conn = self.graph.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT value FROM kv_store WHERE key = ?")?;
    
    let result = stmt.query_row([key], |row| {
        let bytes: Vec<u8> = row.get(0)?;
        Ok(KvValue::Binary(bytes))
    });
    
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn kv_prefix_scan(
    &self,
    _snapshot_id: SnapshotId,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, KvValue)>, SqliteGraphError> {
    let prefix_str = String::from_utf8_lossy(prefix);
    let pattern = format!("{}%", prefix_str);
    
    let conn = self.graph.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT key, value FROM kv_store WHERE key LIKE ? ORDER BY key"
    )?;
    
    let rows = stmt.query_map([&pattern], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    
    // Convert and collect results...
}
```

### Usage Example

```rust
use sqlitegraph::backend::{SqliteGraphBackend, V3Backend};
use sqlitegraph::types::KvValue;

// With V3 backend
let v3 = V3Backend::create("/tmp/test.graph")?;

// First write initializes KV store
v3.kv_set_v3(
    b"session:agent-1".to_vec(),
    KvValue::Json(json!({"status": "active"})),
    None,
    1,
);

// Subsequent reads
let value = v3.kv_get_v3(b"session:agent-1");
assert!(value.is_some());

// Prefix scan
let sessions = v3.kv_prefix_scan(SnapshotId::current(), b"session:")?;
```

---

## Query Nodes by Kind

### Purpose

Find all nodes with a given kind (e.g., all "Class" nodes, all "agent:*" nodes).

### API Signature

```rust
fn query_nodes_by_kind(
    &self,
    snapshot_id: SnapshotId,
    kind: &str,
) -> Result<Vec<i64>, SqliteGraphError>
```

### V3 Implementation

```rust
fn query_nodes_by_kind(
    &self,
    _snapshot_id: SnapshotId,
    kind: &str,
) -> Result<Vec<i64>, SqliteGraphError> {
    let node_store = self.node_store.read();
    let mut results = Vec::new();
    
    // O(N) scan - acceptable for MVP
    // Future: add kind index for O(1) lookups
    for node_id in 1..=node_store.count() {
        if let Ok(node) = node_store.get(node_id) {
            if node.kind == kind {
                results.push(node_id as i64);
            }
        }
    }
    
    Ok(results)
}
```

### SQLite Implementation

```rust
fn query_nodes_by_kind(
    &self,
    _snapshot_id: SnapshotId,
    kind: &str,
) -> Result<Vec<i64>, SqliteGraphError> {
    let conn = self.graph.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id FROM nodes WHERE kind = ?")?;
    
    let ids: Result<Vec<i64>, _> = stmt
        .query_map([kind], |row| row.get(0))?
        .collect();
    
    Ok(ids?)
}
```

### Usage Example

```rust
use sqlitegraph::backend::V3Backend;

let backend = V3Backend::create("/tmp/test.graph")?;

// Create some nodes
backend.insert_node(NodeSpec::new("agent-1", "agent"))?;
backend.insert_node(NodeSpec::new("agent-2", "agent"))?;
backend.insert_node(NodeSpec::new("function-1", "function"))?;

// Query by kind
let agents = backend.query_nodes_by_kind(SnapshotId::current(), "agent")?;
assert_eq!(agents.len(), 2);

let functions = backend.query_nodes_by_kind(SnapshotId::current(), "function")?;
assert_eq!(functions.len(), 1);
```

---

## Query Nodes by Name Pattern

### Purpose

Find nodes matching a glob pattern (e.g., `agent-*`, `test_*`).

### Pattern Syntax

| Pattern | Matches |
|---------|---------|
| `*` | Any sequence of characters |
| `?` | Any single character |
| `[abc]` | Any character in set |
| `[!abc]` | Any character not in set |

### Implementation

Located in `src/backend/pattern.rs`:

```rust
/// Match a string against a glob pattern
pub fn match_glob(pattern: &str, text: &str) -> bool {
    // Convert glob pattern to regex
    let regex_pattern = glob_to_regex(pattern);
    let regex = Regex::new(&format!("^{}$", regex_pattern)).unwrap();
    regex.is_match(text)
}

fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::new();
    for c in glob.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '[' => regex.push('['),
            ']' => regex.push(']'),
            '!' if regex.ends_with('[') => regex.push('^'),
            c if c.is_ascii_alphanumeric() => regex.push(c),
            c => regex.push_str(&regex::escape(&c.to_string())),
        }
    }
    regex
}
```

### Usage Example

```rust
// Find all test-related nodes
let tests = backend.query_nodes_by_name_pattern(
    SnapshotId::current(),
    "test_*"
)?;

// Find all agent nodes with numeric suffix
let agents = backend.query_nodes_by_name_pattern(
    SnapshotId::current(),
    "agent-[0-9]*"
)?;
```

---

## Performance Considerations

### KV Store

| Operation | V3 | SQLite | Notes |
|-----------|-----|--------|-------|
| Get | O(1) | O(1) with index | Both use hash maps |
| Set | O(1) | O(log N) | SQLite has write overhead |
| Prefix Scan | O(N) | O(N) with LIKE | No prefix index yet |

### Node Queries

| Operation | V3 | SQLite | Notes |
|-----------|-----|--------|-------|
| By Kind | O(N) | O(N) | Could add indexes |
| By Pattern | O(N) | O(N) | Regex scan on all |

**Future Optimizations:**
- Kind index for O(1) lookups
- Trie-based prefix scan
- Full-text search index

---

## Testing

```bash
# V3 query tests
cargo test --features native-v3 --lib backend::v3::tests::query_tests

# SQLite query tests  
cargo test --lib backend::sqlite::tests::query_tests

# Pattern matching tests
cargo test --lib backend::pattern::tests
```

---

## See Also

- [KV Store Architecture](kv-store-architecture.md)
- [Pub/Sub Implementation](pubsub-implementation.md)
- [API Reference](../../API.md) - Query API

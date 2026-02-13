# SQLiteGraph API Reference

**Version:** v2.0.0  
**Last Updated:** 2026-02-12

The complete API documentation is available at **[docs.rs/sqlitegraph](https://docs.rs/sqlitegraph)**.

This document provides a quick overview of the main API surface for all three backends.

---

## Backend Quick Reference

| Backend | Status | Module | Best For |
|---------|--------|--------|----------|
| **SQLite** | ✅ Stable | `backend::sqlite` | Debuggable, SQL ecosystem |
| **Native V3** | ✅ Production | `backend::native::v3` | Performance, unlimited scale |
| **Native V2** | ⚠️ Deprecated | `backend::native` | Do not use for new projects |

---

## Table of Contents

- [GraphBackend Trait (Unified API)](#graphbackend-trait-unified-api)
- [SQLite Backend API](#sqlite-backend-api)
- [Native V3 Backend API](#native-v3-backend-api)
- [Native V2 Backend API (Deprecated)](#native-v2-backend-api-deprecated)
- [Graph Algorithms API](#graph-algorithms-api)
- [HNSW Vector Search API](#hnsw-vector-search-api)
- [KV Store API](#kv-store-api)
- [Pub/Sub API](#pubsub-api)

---

## GraphBackend Trait (Unified API)

All backends implement `GraphBackend` - use this trait for backend-agnostic code:

```rust
use sqlitegraph::backend::{GraphBackend, NodeSpec, EdgeSpec};

fn create_user(backend: &dyn GraphBackend, name: &str) -> Result<i64, SqliteGraphError> {
    backend.insert_node(NodeSpec {
        kind: "User".to_string(),
        name: name.to_string(),
        file_path: None,
        data: serde_json::json!({"created": "now"}),
    })
}

// Works with any backend:
let sqlite = SqliteGraphBackend::in_memory()?;
let v3 = V3Backend::create("data.graph")?;

create_user(&sqlite, "Alice")?;
create_user(&v3, "Bob")?;
```

### Core Trait Methods

| Method | Description |
|--------|-------------|
| `insert_node(spec)` | Insert node, returns ID |
| `insert_edge(spec)` | Insert edge, returns ID |
| `neighbors(snapshot, node, query)` | Get neighbors with direction filter |
| `entity_ids()` | Get all node IDs |
| `subscribe(filter)` | Subscribe to events (Pub/Sub) |
| `kv_get(snapshot, key)` | Get KV value |
| `kv_set(key, value, ttl)` | Set KV value |

---

## SQLite Backend API

**Status:** Stable, mature, debuggable

```rust
use sqlitegraph::backend::sqlite::SqliteGraphBackend;

// In-memory (testing)
let backend = SqliteGraphBackend::in_memory()?;

// From existing SqliteGraph
let backend = SqliteGraphBackend::from_graph(graph);

// Access underlying graph for SQL queries
let graph = backend.graph();
```

### Pub/Sub (New in v2.0.0)

```rust
use sqlitegraph::backend::{SubscriptionFilter, PubSubEvent};

let (sub_id, rx) = backend.subscribe(SubscriptionFilter::all())?;

// Events emitted on insert_node/insert_edge
backend.insert_node(NodeSpec { ... })?; // Emits NodeChanged
```

### HNSW Vector Storage

```rust
use sqlitegraph::hnsw::storage::SQLiteVectorStorage;

let storage = SQLiteVectorStorage::new(index_id, conn);
```

---

## Native V3 Backend API

**Status:** Production-ready, recommended for new projects

```rust
use sqlitegraph::backend::native::v3::V3Backend;

// Create new database
let backend = V3Backend::create("data.graph")?;

// Open existing database
let backend = V3Backend::open("data.graph")?;

// Create with WAL enabled
let backend = V3Backend::create_with_wal("data.graph", true)?;
```

### Lazy Initialization Inspection

```rust
// Check if features have been initialized
assert!(!backend.is_kv_initialized());      // false until first kv_get/set
assert!(!backend.is_pubsub_initialized());  // false until first subscribe

backend.kv_set_v3(b"key".to_vec(), KvValue::Integer(42), None);
assert!(backend.is_kv_initialized());       // true now
```

### V3-Native KV API

V3 provides methods that work directly with V3 `KvValue` (no feature gates needed):

```rust
use sqlitegraph::backend::native::v3::KvValue;

// Get (returns Option<KvValue>)
let value = backend.kv_get_v3(SnapshotId::current(), b"my_key");

// Set
backend.kv_set_v3(b"my_key".to_vec(), KvValue::String("value".into()), None);

// Delete
backend.kv_delete_v3(b"my_key");
```

### HNSW Vector Storage

```rust
// Create storage backed by V3 KV
let storage = backend.create_hnsw_storage("embeddings").unwrap();
```

### Pub/Sub

```rust
let (sub_id, rx) = backend.subscribe(SubscriptionFilter::all())?;
```

---

## Native V2 Backend API (Deprecated)

**Status:** Deprecated, will be removed in v1.7.0

**Limitation:** Hard 2048 node limit (8MB node region)

**Migration:** Use V3 backend instead (same features, unlimited capacity)

```rust
// DEPRECATED - Do not use for new code
use sqlitegraph::{GraphConfig, open_graph};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?; // Uses V2 backend
```

---

## Graph Algorithms API

All algorithms work with any backend via `&dyn GraphBackend`:

```rust
use sqlitegraph::algo;

// With V3 backend
let v3 = V3Backend::create("data.graph")?;
let scores = algo::pagerank(&v3, 0.85, 50)?;

// With SQLite backend
let sqlite = SqliteGraphBackend::in_memory()?;
let scores = algo::pagerank(&sqlite, 0.85, 50)?;
```

### Algorithm Categories

| Category | Count | Examples |
|----------|-------|----------|
| Core Graph Theory | 5 | SCC, WCC, Topological Sort |
| CFG Analysis | 5 | Dominators, Control Dependence |
| Path Analysis | 4 | Shortest Path, Cycle Basis |
| Security | 4 | Taint Analysis, Sink Discovery |
| Program Analysis | 3 | Slicing, SCC Collapse |
| ... | ... | ... |

**Total: 35+ algorithms**

See [GRAPH_ALGORITHMS_GUIDE.md](docs/GRAPH_ALGORITHMS_GUIDE.md) for complete list.

---

## HNSW Vector Search API

### Creating an Index

**SQLite Backend:**
```rust
use sqlitegraph::hnsw::{HnswConfig, HnswIndex};

let config = HnswConfig::builder()
    .dimension(768)
    .distance_metric(DistanceMetric::Cosine)
    .build()?;

let index = HnswIndex::new_with_sqlite_storage("my_index", config, conn)?;
```

**V3 Backend:**
```rust
let storage = backend.create_hnsw_storage("my_index").unwrap();
let index = HnswIndex::new_with_storage("my_index", config, storage)?;
```

### Common Operations

```rust
// Insert
let vector = vec![0.1, 0.2, 0.3, /* ... 768 dims */];
let id = index.insert(&vector, Some(json!({"doc_id": "123"})))?;

// Search
let results = index.search(&query_vector, 10)?; // top 10
for (id, distance) in results {
    println!("ID: {}, Distance: {}", id, distance);
}
```

---

## KV Store API

### Availability by Backend

| Backend | Status | Notes |
|---------|--------|-------|
| **V3** | ✅ Full | Lazy initialization |
| **SQLite** | ✅ Full | SQL table |
| **V2** | ✅ Full | In-memory |

### V3 Native Methods (Recommended)

```rust
// Get
match backend.kv_get_v3(SnapshotId::current(), b"counter") {
    Some(KvValue::Integer(n)) => println!("Count: {}", n),
    _ => println!("Not found"),
}

// Set with TTL (60 seconds)
backend.kv_set_v3(
    b"session".to_vec(),
    KvValue::Json(json!({"user": "alice"})),
    Some(60),
);

// Delete
backend.kv_delete_v3(b"session");
```

### Generic Trait Methods

```rust
use sqlitegraph::backend::{GraphBackend, KvValue};

fn set_config(backend: &dyn GraphBackend) -> Result<(), SqliteGraphError> {
    backend.kv_set(
        b"config".to_vec(),
        KvValue::Json(json!({"version": "1.0"})),
        None,
    )
}
```

---

## Pub/Sub API

### Availability by Backend

| Backend | Status | Notes |
|---------|--------|-------|
| **V3** | ✅ Full | Lazy initialization |
| **SQLite** | ✅ Full | In-memory publisher |
| **V2** | ✅ Full | In-memory publisher |

**Note:** Before v2.0.0, Pub/Sub was documented as "V2 only" - this is now fixed.

### Basic Usage

```rust
use sqlitegraph::backend::{SubscriptionFilter, PubSubEvent};

// Subscribe
let filter = SubscriptionFilter {
    node_changes: true,
    edge_changes: false,
    kv_changes: false,
    snapshot_commits: false,
};
let (sub_id, rx) = backend.subscribe(filter)?;

// Receive events
std::thread::spawn(move || {
    while let Ok(event) = rx.recv() {
        match event {
            PubSubEvent::NodeChanged { node_id, snapshot_id } => {
                println!("Node {} changed at snapshot {}", node_id, snapshot_id);
            }
            PubSubEvent::EdgeChanged { edge_id, snapshot_id } => {
                println!("Edge {} changed", edge_id);
            }
            _ => {}
        }
    }
});

// Operations emit events
backend.insert_node(NodeSpec { ... })?; // Emits NodeChanged
backend.insert_edge(EdgeSpec { ... })?;  // Emits EdgeChanged

// Cleanup
backend.unsubscribe(sub_id)?;
```

### Event Types

```rust
pub enum PubSubEvent {
    NodeChanged { node_id: i64, snapshot_id: u64 },
    EdgeChanged { edge_id: i64, snapshot_id: u64 },
    KVChanged { key_hash: u64, snapshot_id: u64 },
    SnapshotCommitted { snapshot_id: u64 },
}
```

---

## Error Types

```rust
pub enum SqliteGraphError {
    ConnectionError(String),
    SchemaError(String),
    QueryError(String),
    NotFound(String),
    InvalidInput(String),
    TransactionError(String),
    ValidationError(String),
    Unsupported(String),  // Feature not supported by backend
    NativeError(NativeBackendError),
}
```

**Common Error Cases:**
- `Unsupported` - Backend doesn't support feature (e.g., KV on older SQLite)
- `NotFound` - Entity/edge doesn't exist
- `InvalidInput` - Invalid parameters (e.g., wrong vector dimension)

---

## Full Documentation

- **API docs:** https://docs.rs/sqlitegraph
- **Architecture:** [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- **Algorithms:** [docs/GRAPH_ALGORITHMS_GUIDE.md](docs/GRAPH_ALGORITHMS_GUIDE.md)
- **Changelog:** [CHANGELOG.md](CHANGELOG.md)

---

**Note:** This API reference is accurate for v2.0.0. We document deprecations and limitations honestly - check backend-specific sections for feature availability.

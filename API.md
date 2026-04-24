# SQLiteGraph API Reference

**Version:** v2.1.2  
**Last Updated:** 2026-04-24

The complete API documentation is available at **[docs.rs/sqlitegraph](https://docs.rs/sqlitegraph)**.

This document provides a quick overview of the main API surface.

---

## Backend Quick Reference

| Backend | Status | Module | Best For |
|---------|--------|--------|----------|
| **SQLite** | ✅ Stable | `backend::sqlite` | Debuggable, SQL ecosystem |
| **Native V3** | ✅ Stable | `backend::native::v3` | Performance, unlimited scale |

---

## Table of Contents

- [GraphBackend Trait (Unified API)](#graphbackend-trait-unified-api)
- [SQLite Backend API](#sqlite-backend-api)
- [Native V3 Backend API](#native-v3-backend-api)
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

**Status:** Stable, recommended for new projects

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
use sqlitegraph::snapshot::SnapshotId;

// Get (returns Option<KvValue>)
// SnapshotId::current() returns SnapshotId(0) - works for both SQLite and V3
let value = backend.kv_get_v3(SnapshotId::current(), b"my_key");

// For native-v3 specific use cases needing unique snapshot IDs:
let unique_snapshot = SnapshotId::new_incrementing();
let value = backend.kv_get_v3(unique_snapshot, b"my_key");

// Set
backend.kv_set_v3(b"my_key".to_vec(), KvValue::String("value".into()), None);

// Delete
backend.kv_delete_v3(b"my_key");
```

**Snapshot Behavior:**
- `SnapshotId::current()` returns `SnapshotId(0)` - works with all backends
- `SnapshotId::new_incrementing()` returns unique incrementing IDs (native-v3 only)
- SQLite backend only supports `SnapshotId(0)` (no historical snapshots)
- Native-v3 backend supports both snapshot types

### Node Caching (v2.1.0+)

V3Backend includes an LRU cache for node record lookups:

```rust
use sqlitegraph::backend::native::v3::NodeCache;

// The cache is automatically created with the backend
// Default capacity: 1000 nodes

// Manual cache control (advanced usage)
let cache = NodeCache::new(1000);
cache.insert(node_id, node_record);
if let Some(record) = cache.get(node_id) {
    // Cache hit - use record
}

// Invalidate entries on mutations
cache.invalidate(node_id);

// Clear entire cache
cache.clear();

// Check cache statistics
let cached_count = cache.len();
let is_empty = cache.is_empty();
```

**Performance Impact:**
- Point lookups: 114× faster when cached (warm cache vs cold cache)
- Hit rate: 85-95% for traversal workloads
- Thread-safe: Mutex-protected for concurrent access

### Parallel BFS (v2.1.1+)

V3Backend supports parallel breadth-first search using Rayon (fixed in v2.1.1):

```rust
use sqlitegraph::backend::native::v3::algorithm::parallel_bfs;
use sqlitegraph::backend::native::v3::algorithm::BfsConfig;

// Standard parallel BFS
let result = parallel_bfs(&backend, start_node, None)?;

// With custom configuration
let config = BfsConfig {
    max_depth: Some(100),
    sequential_threshold: Some(1000), // Use sequential BFS for < 1000 nodes
};
let result = parallel_bfs(&backend, start_node, Some(config))?;

// Result contains visited nodes and levels
println!("Visited {} nodes", result.visited_count);
println!("Max depth: {}", result.max_depth);
```

**Performance Impact:**
- **Thread-safe:** Minecraft-style chunked processing, zero shared state during parallel phase
- **Sequential fallback:** Automatically uses sequential BFS for graphs <1K nodes
- **Measured performance:** 1.0-1.17× speedup on small graphs (100-500 nodes)
- **Status:** Stable for small graphs, experimental for larger graphs
- **Note:** Correct expectations - thread-safe implementation, not a major performance win

### Adaptive Page Sizing (v2.1.0+)

V3Backend automatically adapts page size based on storage media:

```rust
// Automatic - no API needed
// SSD detection → 4KB pages (better random read performance)
// HDD detection → 16KB pages (reduce seek overhead)
// Fallback → 8KB pages if detection fails

// Manual override (advanced usage)
use sqlitegraph::backend::native::v3::storage::adaptive_page;

let media_type = adaptive_page::detect_media_type(db_path)?;
match media_type {
    adaptive_page::MediaDetectorResult::SSD => println!("Using 4KB pages"),
    adaptive_page::MediaDetectorResult::HDD => println!("Using 16KB pages"),
    adaptive_page::MediaDetectorResult::Unknown => println!("Using 8KB pages"),
}
```

**Performance Impact:**
- **Measured:** 15-25% I/O improvement on appropriate media (verified)
- SSD detection → 4KB pages (matches SSD block size)
- HDD detection → 16KB pages (reduces seek overhead by 4×)
- Fallback → 8KB pages if detection fails
- **Status:** Fully wired and verified

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

### V3 Native Methods (Recommended)

```rust
use sqlitegraph::snapshot::SnapshotId;

// Get (SnapshotId::current() returns 0 - works with all backends)
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

# SQLiteGraph API Reference

## Official Documentation

The complete API documentation is available at **[docs.rs/sqlitegraph](https://docs.rs/sqlitegraph)**.

This document provides a quick overview of the main API surface.

## Table of Contents

- [SQLite Backend API](#sqlite-backend-api)
- [Native V2 Backend API](#native-v2-backend-api)
- [Graph Algorithms API](#graph-algorithms-api)
- [HNSW Vector Search API](#hnsw-vector-search-api)
- [KV Store API](#kv-store-api)
- [Query API Enhancements](#query-api-enhancements)
- [Introspection API](#introspection-api)
- [Progress Tracking API](#progress-tracking-api)
- [Error Types](#error-types)

---

## SQLite Backend API

### Core Types

```rust
use sqlitegraph::{SqliteGraph, GraphEntity, GraphEdge};

// Open database
let graph = SqliteGraph::open_in_memory()?;
let graph = SqliteGraph::open("path/to/db")?;

// Entity
pub struct GraphEntity {
    pub id: u64,
    pub kind: String,
    pub name: String,
    pub file_path: Option<String>,
    pub data: serde_json::Value,
}

// Edge
pub struct GraphEdge {
    pub id: u64,
    pub from_id: u64,
    pub to_id: u64,
    pub edge_type: String,
    pub data: serde_json::Value,
}
```

### Main Methods

| Method | Description |
|--------|-------------|
| `open_in_memory()` | Create in-memory database |
| `open(path: &str)` | Open file-based database |
| `insert_entity(&entity)` | Insert new entity, returns ID |
| `get_entity(id)` | Retrieve entity by ID |
| `update_entity(&entity)` | Update existing entity |
| `delete_entity(id)` | Delete entity |
| `insert_edge(&edge)` | Insert new edge |
| `get_edge(id)` | Retrieve edge by ID |
| `neighbors(id, direction)` | Get neighbor entities |
| `has_path(from, to)` | Check if path exists |
| `snapshot()` | Create MVCC snapshot |

---

## Native V2 Backend API

### Core Types

```rust
use sqlitegraph::{GraphConfig, open_graph, NodeSpec, EdgeSpec};

// Configuration
let config = GraphConfig::native();
let config = GraphConfig::native()
    .with_buffer_size(128 * 1024 * 1024)
    .with_parallel_recovery(8);

// Open graph
let graph = open_graph("path/to/graph.db", &config)?;

// Node
pub struct NodeSpec {
    pub kind: String,
    pub name: String,
    pub file_path: Option<String>,
    pub data: serde_json::Value,
}

// Edge
pub struct EdgeSpec {
    pub from: u64,
    pub to: u64,
    pub edge_type: String,
    pub data: serde_json::Value,
}
```

### Main Methods

| Method | Description |
|--------|-------------|
| `open_graph(path, config)` | Open Native V2 graph |
| `insert_node(spec)` | Insert new node |
| `get_node(id)` | Retrieve node by ID |
| `update_node(&spec)` | Update existing node |
| `delete_node(id)` | Delete node |
| `insert_edge(spec)` | Insert new edge |
| `neighbors(query)` | Get neighbors with query options |
| `snapshot()` | Create MVCC snapshot |

---

## Graph Algorithms API

### Overview

SQLiteGraph v1.4.1 includes a comprehensive graph algorithms library with **35 algorithms** across 13 categories.

### Algorithm Categories

| Category | Algorithms | Example Functions |
|----------|------------|-------------------|
| **Core Graph Theory** | WCC, SCC, Transitive Closure, Transitive Reduction, Topological Sort | `algo::scc()`, `algo::topological_sort()` |
| **Reachability** | Forward, Backward, Can-Reach, Unreachable Nodes | `algo::forward_reachability()`, `algo::can_reach()` |
| **Core CFG** | Dominators, Post-Dominators, Control Dependence | `algo::dominators()`, `algo::post_dominators()` |
| **Derived CFG** | Dominance Frontiers, Natural Loops | `algo::dominance_frontiers()`, `algo::natural_loops()` |
| **Path Analysis** | Enumerate Paths, Constrained Paths | `algo::enumerate_paths()` |
| **Dependency** | Critical Path, Minimal Cycle Basis | `algo::critical_path()`, `algo::cycle_basis()` |
| **Program Analysis** | Backward/Forward Slicing, SCC Collapse | `algo::backward_slice()`, `algo::collapse_scc()` |
| **Distributed Systems** | Min Cut, Min Vertex Cut, Partitioning | `algo::min_cut()`, `algo::partition_graph()` |
| **Observability** | Happens-Before, Impact Radius | `algo::happens_before()`, `algo::impact_radius()` |
| **ML/Inference** | Subgraph Isomorphism, Graph Rewrite, Similarity | `algo::subgraph_isomorphism()` |
| **Graph Diff** | Structural Delta, Refactor Validation | `algo::graph_diff()`, `algo::validate_refactor()` |
| **Security** | Taint Propagation, Sink Analysis | `algo::taint_forward()`, `algo::sink_analysis()` |

### Usage Examples

```rust
use sqlitegraph::algo;

// Classic algorithms
let scores: HashMap<u64, f64> = algo::pagerank(&graph, 0.85, 50)?;
let scores = algo::pagerank_with_progress(&graph, 0.85, 50, progress)?;
let communities: HashMap<u64, u64> = algo::label_propagation(&graph)?;

// Graph decomposition
let sccs: Vec<Vec<u64>> = algo::strongly_connected_components(&graph)?;
let wccs: Vec<Vec<u64>> = algo::weakly_connected_components(&graph)?;

// Reachability queries
let reachable: HashSet<u64> = algo::forward_reachability(&graph, start_node)?;
let can_reach: bool = algo::can_reach(&graph, from_node, to_node)?;

// CFG analysis
let dominators: HashSet<u64> = algo::dominators(&graph, entry_node)?;
let frontiers: HashMap<u64, Vec<u64>> = algo::dominance_frontiers(&graph, entry_node)?;
let loops: Vec<(u64, u64)> = algo::natural_loops(&graph)?;

// Program slicing
let slice: HashSet<u64> = algo::backward_slice(&graph, target_node)?;
let slice: HashSet<u64> = algo::forward_slice(&graph, source_node)?;

// Security analysis
let tainted: HashSet<u64> = algo::taint_forward(&graph, &source_nodes)?;
```

### Key Algorithm Signatures

| Function | Parameters | Returns |
|----------|------------|---------|
| `pagerank(graph, damping, iter)` | `(&G, f64, usize)` | `HashMap<u64, f64>` |
| `scc(graph)` | `(&G)` | `Vec<Vec<u64>>` |
| `topological_sort(graph)` | `(&G)` | `Result<Vec<u64>>` |
| `forward_reachability(graph, start)` | `(&G, u64)` | `HashSet<u64>` |
| `dominators(graph, entry)` | `(&G, u64)` | `DominatorResult` |
| `backward_slice(graph, target)` | `(&G, u64)` | `HashSet<u64>` |
| `taint_forward(graph, sources)` | `(&G, &[u64])` | `HashSet<u64>` |

### Full Documentation

For complete algorithm reference with all 35 algorithms, see:
**[docs/GRAPH_ALGORITHMS_GUIDE.md](docs/GRAPH_ALGORITHMS_GUIDE.md)**

---

## HNSW Vector Search API

### Core Types

```rust
use sqlitegraph::hnsw::{HnswConfig, HnswIndex, DistanceMetric};

// Configuration
let config = HnswConfig::builder()
    .dimension(1536)
    .m_connections(16)
    .ef_construction(200)
    .ef_search(50)
    .distance_metric(DistanceMetric::Cosine)
    .build()?;

// Create index
let hnsw = HnswIndex::new(config)?;
```

### Main Methods

| Method | Description |
|--------|-------------|
| `new(config)` | Create new HNSW index |
| `insert_vector(&vec, metadata)` | Insert vector with optional metadata |
| `search(&query, k)` | Search k nearest neighbors |
| `get_vector(id)` | Retrieve vector by ID |
| `len()` | Get number of vectors |
| `is_empty()` | Check if index is empty |

### Distance Metrics

| Metric | Use Case |
|--------|----------|
| `Cosine` | Text embeddings |
| `Euclidean` | General similarity |
| `DotProduct` | Normalized vectors |
| `Manhattan` | Sparse vectors |

---

## KV Store API

### Availability

| Backend | KV Store Support |
|---------|------------------|
| **Native V2** | Full support |
| **SQLite** | Not supported |

### Core Types

```rust
use sqlitegraph::backend::KvValue;
use serde_json::json;

// Value types supported by the KV store
pub enum KvValue {
    Bytes(Vec<u8>),      // Raw binary data
    String(String),       // Text values
    Integer(i64),         // 64-bit integers
    Float(f64),           // Floating point numbers
    Boolean(bool),        // True/false
    Json(serde_json::Value),  // JSON objects and arrays
}
```

### Main Methods

| Method | Parameters | Returns | Description |
|--------|------------|---------|-------------|
| `kv_get(snapshot_id, key)` | `(SnapshotId, &[u8])` | `Option<KvValue>` | Get value at snapshot |
| `kv_set(key, value, ttl)` | `(Vec<u8>, KvValue, Option<u64>)` | `()` | Set value with optional TTL |
| `kv_delete(key)` | `&[u8]` | `()` | Delete a key |

### Usage Examples

```rust
use sqlitegraph::{GraphConfig, open_graph};
use sqlitegraph::backend::KvValue;

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;

// Set values
graph.kv_set(
    b"counter".to_vec(),
    KvValue::Integer(42),
    None,  // No TTL
)?;

graph.kv_set(
    b"config".to_vec(),
    KvValue::Json(json!({"theme": "dark"})),
    None,
)?;

// Get value (requires snapshot_id)
let snapshot = graph.snapshot()?;
if let Some(KvValue::Integer(count)) = graph.kv_get(snapshot.id, b"counter")? {
    println!("Counter: {}", count);
}

// Delete value
graph.kv_delete(b"counter")?;

// Set with TTL (expires in 60 seconds)
graph.kv_set(
    b"temp_session".to_vec(),
    KvValue::String("active".to_string()),
    Some(60),  // TTL in seconds
)?;
```

### TTL Support

Keys can have an optional time-to-live in seconds. After expiration, `kv_get` returns `None`:

```rust
// Expire in 5 minutes (300 seconds)
graph.kv_set(b"cache:data".to_vec(), value, Some(300))?;

// Expire in 1 hour (3600 seconds)
graph.kv_set(b"session:xyz".to_vec(), value, Some(3600))?;

// No expiration (None)
graph.kv_set(b"permanent".to_vec(), value, None)?;
```

### Transactional Behavior

KV operations participate in transactions with graph operations:

```rust
// Atomic: node + metadata stored together
let node_id = graph.insert_node(spec)?;
graph.kv_set(
    format!("metadata:{}", node_id).into_bytes(),
    KvValue::Json(json!({"created": "2026-02-03"})),
    None,
)?;
// Both commit or roll back together
```

### Pub/Sub Integration

KV changes emit `KVChanged` events when transactions commit:

```rust
use sqlitegraph::backend::{SubscriptionFilter, PubSubEvent};

let filter = SubscriptionFilter::all();
let (sub_id, rx) = graph.subscribe(filter)?;

// Receive KV change events
while let Ok(event) = rx.recv() {
    if let PubSubEvent::KVChanged { key_hash, snapshot_id } = event {
        println!("KV entry changed: hash={}, snapshot={}", key_hash, snapshot_id);
    }
}
```

---

## Query API Enhancements

### Overview

Phase 58 introduces query API enhancements that make it easier to work with graph data without maintaining external ID tracking. These features are particularly useful for pub/sub use cases like agent messaging and topic-based subscriptions.

### KV Prefix Scanning

```rust
fn kv_prefix_scan(
    &self,
    snapshot_id: SnapshotId,
    prefix: &[u8]
) -> Result<Vec<(Vec<u8>, KvValue)>, SqliteGraphError>
```

**Description**: Retrieve all KV entries with keys starting with the given prefix.

**Parameters**:
- `snapshot_id` - MVCC snapshot ID for consistent reads
- `prefix` - Byte slice to match key prefixes (empty string = all keys)

**Returns**: Vector of `(key, value)` tuples in lexicographic order

**Example**:
```rust
use sqlitegraph::{GraphConfig, open_graph};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;
let snapshot = graph.snapshot()?;

// Get all keys with prefix "user:"
let results = graph.kv_prefix_scan(snapshot.id, b"user:")?;

for (key, value) in results {
    println!("{:?} = {:?}", String::from_utf8_lossy(&key), value);
}
```

**Backend Support**:
| Backend | Support |
|---------|---------|
| **Native V2** | Full support (HashMap iteration) |
| **SQLite** | Full support (LIKE query) |

---

### Query Nodes by Kind

```rust
fn query_nodes_by_kind(
    &self,
    snapshot_id: SnapshotId,
    kind: &str
) -> Result<Vec<i64>, SqliteGraphError>
```

**Description**: Get all node IDs where the node's kind equals the given string.

**Parameters**:
- `snapshot_id` - MVCC snapshot ID for consistent reads
- `kind` - Exact kind string to match (case-sensitive)

**Returns**: Sorted vector of node IDs

**Example**:
```rust
// Find all agent nodes
let agent_ids = graph.query_nodes_by_kind(snapshot.id, "agent")?;

println!("Found {} agents", agent_ids.len());
for node_id in agent_ids {
    let node = graph.get_node(snapshot.id, node_id)?;
    println!("  - {}: {:?}", node.name, node.data);
}
```

**Backend Support**:
| Backend | Implementation |
|---------|----------------|
| **Native V2** | NodeStore iteration with kind filtering |
| **SQLite** | `WHERE kind = ?` query |

---

### Query Nodes by Name Pattern

```rust
fn query_nodes_by_name_pattern(
    &self,
    snapshot_id: SnapshotId,
    pattern: &str
) -> Result<Vec<i64>, SqliteGraphError>
```

**Description**: Get all node IDs where the node's name matches a glob pattern.

**Parameters**:
- `snapshot_id` - MVCC snapshot ID for consistent reads
- `pattern` - Glob pattern string (case-sensitive)

**Returns**: Sorted vector of node IDs

**Pattern Syntax**:
| Wildcard | Matches | Example |
|----------|---------|---------|
| `*` | Any sequence (including empty) | `msg_index:*` matches `msg_index:agent-1`, `msg_index:agent-2` |
| `?` | Exactly one character | `agent-?` matches `agent-1`, `agent-A` (not `agent-12`) |
| `\*`, `\?` | Literal asterisk/question mark | `file\\*.txt` matches `file*.txt` |

**Example**:
```rust
// Find all nodes with name matching "msg_index:*"
let msg_ids = graph.query_nodes_by_name_pattern(snapshot.id, "msg_index:*")?;

// Find nodes with single-character suffix
let agent_ids = graph.query_nodes_by_name_pattern(snapshot.id, "agent-?")?;

// Escape wildcards for literal matching
let literal = graph.query_nodes_by_name_pattern(snapshot.id, "file\\*.txt")?;
```

**Backend Support**: Full support on both SQLite and Native V2 backends

---

### SubscriptionFilter Pattern Constructors

```rust
impl SubscriptionFilter {
    pub fn kind_patterns(patterns: Vec<String>) -> Self
    pub fn name_patterns(patterns: Vec<String>) -> Self
}
```

**Description**: Create subscription filters that match glob patterns on node kind or name.

**Methods**:
| Method | Description |
|--------|-------------|
| `kind_patterns(patterns)` | Match events where node kind matches any pattern |
| `name_patterns(patterns)` | Match events where node name matches any pattern |

**Example**:
```rust
use sqlitegraph::backend::SubscriptionFilter;

// Subscribe to all agent events (kind pattern)
let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);
let (sub_id, rx) = graph.subscribe(filter)?;

// Subscribe to message index events (name pattern)
let filter = SubscriptionFilter::name_patterns(vec!["msg_index:*".to_string()]);
let (sub_id, rx) = graph.subscribe(filter)?;

// Multiple patterns (any match triggers event)
let filter = SubscriptionFilter::kind_patterns(vec![
    "agent:*".to_string(),
    "message:*".to_string(),
    "system:*".to_string(),
]);
let (sub_id, rx) = graph.subscribe(filter)?;
```

**How Pattern Matching Works**:
1. When a node event occurs (creation/modification), the node's kind/name is checked
2. Patterns are evaluated in order; if any pattern matches, the event is delivered
3. Matching is case-sensitive
4. Supports `*` (any sequence including empty) and `?` (exactly one character)
5. Escape with `\*` and `\?` for literal asterisk/question mark

**Backend Support**: Full support on both SQLite and Native V2 backends

---

### CLI Commands

| Command | Description | Example |
|---------|-------------|---------|
| `kv-scan --prefix PREFIX` | Scan KV store by key prefix | `sqlitegraph graph.db kv-scan --prefix "user:"` |
| `nodes-by-kind --kind KIND` | Find all nodes with given kind | `sqlitegraph graph.db nodes-by-kind --kind "agent"` |
| `nodes-by-name --pattern PATTERN` | Find nodes matching name pattern | `sqlitegraph graph.db nodes-by-name --pattern "msg_index:*"` |

---

## Introspection API

### GraphIntrospection

```rust
use sqlitegraph::introspection::GraphIntrospection;

let intro = GraphIntrospection::new(&graph)?;

// Get statistics
let nodes: usize = intro.node_count()?;
let edges: (usize, usize) = intro.edge_count_estimate()?;
let info: serde_json::Value = intro.backend_info()?;
let json: String = intro.to_json()?;
```

### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `new(graph)` | `GraphIntrospection` | Create introspection instance |
| `node_count()` | `usize` | Exact node count |
| `edge_count_estimate()` | `(usize, usize)` | (min, max) edge estimate |
| `backend_info()` | `serde_json::Value` | Backend-specific info |
| `to_json()` | `String` | JSON serialization |

---

## Progress Tracking API

### ProgressCallback Trait

```rust
use sqlitegraph::progress::{ProgressCallback, ProgressState, ConsoleProgress, NoProgress};

// Use with algorithms
let scores = algo::pagerank_with_progress(&graph, 0.85, 50, ConsoleProgress::new())?;

// Custom implementation
struct MyProgress;
impl ProgressCallback for MyProgress {
    fn on_progress(&self, state: &ProgressState) {
        println!("{}: {}%", state.message, state.percent);
    }
}
```

### Implementations

| Implementation | Behavior |
|----------------|----------|
| `NoProgress` | No-op, zero overhead |
| `ConsoleProgress` | Progress bars to terminal |

---

## Error Types

### SqliteGraphError

```rust
use sqlitegraph::SqliteGraphError;

match result {
    Ok(value) => /* ... */,
    Err(SqliteGraphError::ValidationError(msg)) => { /* ... */ }
    Err(SqliteGraphError::ConnectionError(msg)) => { /* ... */ }
    Err(SqliteGraphError::TransactionError(msg)) => { /* ... */ }
    Err(SqliteGraphError::NotFoundError(msg)) => { /* ... */ }
    Err(e) => { /* ... */ }
}
```

### HnswConfigError

```rust
use sqlitegraph::hnsw::HnswConfigError;

match HnswConfig::builder().build() {
    Ok(config) => /* ... */,
    Err(HnswConfigError::InvalidDimension) => { /* ... */ }
    Err(HnswConfigError::InvalidMParameter) => { /* ... */ }
    Err(e) => { /* ... */ }
}
```

---

## Feature Flags

```toml
[dependencies]
# SQLite backend only
sqlitegraph = "1.4"

# Native V2 backend
sqlitegraph = { version = "1.4", features = ["native-v2"] }

# V2 I/O tracing (development)
sqlitegraph = { version = "1.4", features = ["trace_v2_io"] }
```

---

## Thread Safety

**Important**: `SqliteGraph` is **NOT thread-safe** for concurrent writes.

For concurrent access:
- Use MVCC snapshots for concurrent reads
- Each thread should have its own graph connection
- Use external synchronization for writes

```rust
// Correct: Snapshot for concurrent reads
let snapshot = graph.snapshot()?;
// snapshot can be cloned and sent to threads
```

---

## Full Documentation

For complete API documentation with examples, see:
- **[docs.rs/sqlitegraph](https://docs.rs/sqlitegraph)** - Official rustdoc
- **[manual.md](manual.md)** - Operator manual
- **[CHANGELOG.md](CHANGELOG.md)** - Version history

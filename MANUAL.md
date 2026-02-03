# SQLiteGraph Manual

Comprehensive usage guide for SQLiteGraph with dual backend architecture (SQLite and Native V2).

---

## 1. Quick Start

### Installation

```toml
[dependencies]
sqlitegraph = "1.4"

# For Native V2 backend (with pub/sub support)
sqlitegraph = { version = "1.4", features = ["native-v2"] }
```

### Basic Usage

```rust
use sqlitegraph::{SqliteGraph, GraphEntity};

let graph = SqliteGraph::open_in_memory()?;

let entity = GraphEntity {
    id: 0,
    kind: "User".to_string(),
    name: "Alice".to_string(),
    file_path: None,
    data: serde_json::json!({"age": 30}),
};

let id = graph.insert_entity(&entity)?;
println!("Created entity: {}", id);
```

---

## 2. Backend Selection

### SQLite Backend (Default)

**Use**: General purpose, ACID transactions, existing SQLite data

```rust
use sqlitegraph::{SqliteGraph, GraphEntity, GraphEdge};

let graph = SqliteGraph::open_in_memory()?;
// SQLite operations with full ACID compliance
```

### Native V2 Backend

**Use**: High-performance scenarios, large graphs, traversal-heavy workloads

```rust
use sqlitegraph::{GraphConfig, open_graph, NodeSpec, EdgeSpec};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;
// Clustered adjacency for locality
```

### Backend Comparison

| Characteristic | SQLite Backend | Native V2 Backend |
|----------------|----------------|-------------------|
| **Performance** | Standard SQLite | 10x faster for traversals |
| **Transactions** | Full ACID | Atomic commits, WAL |
| **Memory Usage** | SQLite overhead | Configurable buffers |
| **Use Cases** | General purpose | High performance, large graphs |

---

## 3. Core Operations

### Entity Management (SQLite Backend)

```rust
use sqlitegraph::{SqliteGraph, GraphEntity};

let graph = SqliteGraph::open_in_memory()?;

// Create entity
let entity = GraphEntity {
    id: 0,
    kind: "User".to_string(),
    name: "Alice".to_string(),
    file_path: None,
    data: serde_json::json!({"age": 30}),
};

let entity_id = graph.insert_entity(&entity)?;
let retrieved = graph.get_entity(entity_id)?;

// Update entity
let mut updated_entity = retrieved;
updated_entity.name = "Alice Smith".to_string();
graph.update_entity(&updated_entity)?;
```

### Node Management (Native V2 Backend)

```rust
use sqlitegraph::{GraphConfig, open_graph, NodeSpec, EdgeSpec};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;

// Create node
let node_spec = NodeSpec {
    kind: "User".to_string(),
    name: "Alice".to_string(),
    file_path: None,
    data: serde_json::json!({"age": 30}),
};

let node_id = graph.insert_node(node_spec)?;

// Create edge
let edge_spec = EdgeSpec {
    from: node_id,
    to: node_id,
    edge_type: "self_ref".to_string(),
    data: serde_json::json!({"type": "demo"}),
};

let edge_id = graph.insert_edge(edge_spec)?;
```

---

## 4. Graph Algorithms

### Overview

SQLiteGraph v1.4.1 includes a comprehensive graph algorithms library with **35 algorithms** across 13 categories:

| Category | Algorithms |
|----------|------------|
| **Core Graph Theory** | WCC, SCC, Transitive Closure, Transitive Reduction, Topological Sort |
| **Reachability** | Forward, Backward, Can-Reach, Unreachable Nodes |
| **Core CFG** | Dominators, Post-Dominators, Control Dependence |
| **Derived CFG** | Dominance Frontiers, Natural Loops |
| **Path Analysis** | Enumerate Paths, Enumerate Paths Constrained |
| **Dependency** | Critical Path, Minimal Cycle Basis |
| **Program Analysis** | Backward Slice, Forward Slice, SCC Collapse |
| **Distributed Systems** | Min Cut, Min Vertex Cut, Graph Partitioning |
| **Observability** | Happens-Before, Impact Radius |
| **ML/Inference** | Subgraph Isomorphism, Graph Rewrite, Structural Similarity |
| **Graph Diff** | Graph Diff, Validate Refactor |
| **Security** | Taint Forward, Taint Backward, Sink Analysis, Discover Sources/Sinks |

### Usage Examples

```rust
use sqlitegraph::algo;

// Core graph theory
let sccs = algo::strongly_connected_components(&graph)?;
let sorted = algo::topological_sort(&graph)?;

// Reachability
let reachable = algo::forward_reachability(&graph, start_node)?;

// CFG analysis
let dominators = algo::dominators(&graph, entry_node)?;
let loops = algo::natural_loops(&graph)?;

// Program slicing
let slice = algo::backward_slice(&graph, target_node)?;

// Security analysis
let tainted = algo::taint_forward(&graph, source_nodes)?;

// With progress tracking (for supported algorithms)
use sqlitegraph::progress::ConsoleProgress;
let scores = algo::pagerank_with_progress(&graph, 0.85, 50, ConsoleProgress::new())?;
```

### Full Documentation

For complete algorithm reference with examples, complexity analysis, and CLI commands, see:

**[docs/GRAPH_ALGORITHMS_GUIDE.md](docs/GRAPH_ALGORITHMS_GUIDE.md)**

### Quick Reference

| Category | Module | Example Use |
|----------|--------|-------------|
| **Core** | `algo::wcc`, `algo::scc` | Graph decomposition |
| **Reachability** | `algo::forward_reachability` | "What can this node reach?" |
| **CFG** | `algo::dominators`, `algo::natural_loops` | Control flow analysis |
| **Slicing** | `algo::backward_slice` | Program debugging |
| **Security** | `algo::taint_forward` | Security analysis |
| **ML** | `algo::subgraph_isomorphism` | Pattern matching |

---

## 5. Testing

### Running Tests

```bash
# All tests
cargo test --workspace

# With Native V2 backend
cargo test --workspace --features native-v2

# Specific test patterns
cargo test '*pagerank*'
cargo test '*wal*'
```

### Test Coverage

**v1.4.1 Test Results:**
- 180+ graph algorithm tests passing (35 algorithms across 13 categories)
- 59 pubsub tests passing (event emission, filtering, multiple subscribers)
- 42 WAL tests passing (recovery, corruption, checkpoints)
- 53 concurrent MVCC tests passing (snapshots, stress testing)
- 134 HNSW tests passing
- 65 MVCC lifecycle tests passing

**Total**: 530+ tests passing

---

## 6. Performance

### Native V2 Performance

Based on actual benchmarks (Phase 3, 7):

| Operation | Performance |
|-----------|-------------|
| **Node Insert** | ~50K ops/sec |
| **Edge Insert** | ~100K ops/sec (bulk) |
| **Neighbor Query** | <1ms (clustered) |
| **Vector Search** | <1ms with 95%+ accuracy |

### Parallel WAL Recovery

```rust
use sqlitegraph::{GraphConfig, open_graph};

// Default: 4 threads
let config = GraphConfig::native();
let graph = open_graph("large.db", &config)?;

// Custom: 8 threads
let config = GraphConfig::native().with_parallel_recovery(8);
let graph = open_graph("large.db", &config)?;
```

**Performance**:
- 2-3x speedup for 500+ transactions
- 1.5-2x speedup for 50-100 transactions

---

## 7. Error Handling

### Common Error Types

```rust
use sqlitegraph::SqliteGraphError;

match graph.insert_entity(&entity) {
    Ok(id) => println!("Created: {}", id),
    Err(SqliteGraphError::ValidationError(msg)) => {
        eprintln!("Validation failed: {}", msg);
    }
    Err(SqliteGraphError::ConnectionError(msg)) => {
        eprintln!("Connection failed: {}", msg);
    }
    Err(err) => eprintln!("Error: {}", err),
}
```

### Debug Features

```toml
# Enable V2 I/O tracing
sqlitegraph = { version = "1.3", features = ["trace_v2_io"] }
```

```bash
# Run with debug output
RUST_LOG=debug cargo run --features trace_v2_io
```

---

## 8. Vector Search (HNSW)

### Basic HNSW Usage

```rust
use sqlitegraph::hnsw::{HnswConfig, DistanceMetric, HnswIndex};

let config = HnswConfig::builder()
    .dimension(1536)
    .distance_metric(DistanceMetric::Cosine)
    .build()?;

let hnsw = HnswIndex::new(config)?;

// Insert vector
let vector_id = hnsw.insert_vector(&embedding, Some(metadata))?;

// Search
let results = hnsw.search(&query, k)?;
```

### Distance Metrics

| Metric | Best For | Speed |
|--------|----------|-------|
| **Cosine** | Text embeddings | Fast |
| **Euclidean** | General similarity | Medium |
| **Dot Product** | Normalized vectors | Fastest |
| **Manhattan** | Sparse vectors | Slow |

### CLI Commands

```bash
# Create index
sqlitegraph --backend sqlite --db mygraph.db hnsw-create \
    --dimension 768 --distance-metric cosine

# Insert vectors
sqlitegraph --backend sqlite --db mygraph.db hnsw-insert \
    --index-name vectors --input vectors.json

# Search
sqlitegraph --backend sqlite --db mygraph.db hnsw-search \
    --index-name vectors --input query.json --k 10

# List indexes
sqlitegraph --backend sqlite --db mygraph.db hnsw-list
```

---

## 9. KV Store (Key-Value Storage)

### Overview

The Native V2 backend includes a transactional key-value store for storing arbitrary data alongside your graph. The KV store participates in transactions and emits events through the pub/sub system.

### Availability

| Backend | KV Store Support |
|---------|------------------|
| **Native V2** | Full support |
| **SQLite** | Not supported |

### Basic Usage

```rust
use sqlitegraph::{GraphConfig, open_graph};
use sqlitegraph::backend::KvValue;

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;

// Set a value
graph.kv_set(
    b"user:123:name".to_vec(),
    KvValue::String("Alice".to_string()),
    None,  // No TTL
)?;

// Get a value (requires snapshot_id)
let snapshot = graph.snapshot()?;
if let Some(KvValue::String(name)) = graph.kv_get(snapshot.id, b"user:123:name")? {
    println!("User name: {}", name);
}

// Delete a value
graph.kv_delete(b"user:123:name")?;
```

### Value Types

The `KvValue` enum supports multiple data types:

| Type | Rust Value | Example |
|------|------------|---------|
| `Bytes` | `Vec<u8>` | Raw binary data |
| `String` | `String` | Text values |
| `Integer` | `i64` | 64-bit integers |
| `Float` | `Float` | Floating point numbers |
| `Boolean` | `bool` | True/false |
| `Json` | `serde_json::Value` | JSON objects, arrays |

```rust
use sqlitegraph::backend::KvValue;
use serde_json::json;

// Different value types
graph.kv_set(b"counter".to_vec(), KvValue::Integer(42), None)?;
graph.kv_set(b"price".to_vec(), KvValue::Float(19.99), None)?;
graph.kv_set(b"active".to_vec(), KvValue::Boolean(true), None)?;
graph.kv_set(b"config".to_vec(), KvValue::Json(json!({
    "theme": "dark",
    "notifications": true
})), None)?;
graph.kv_set(b"binary".to_vec(), KvValue::Bytes(vec![0x00, 0xFF, 0xAA]), None)?;
```

### TTL (Time-To-Live)

Keys can be set with an optional TTL in seconds. After expiration, the key returns `None`.

```rust
// Set a key that expires in 60 seconds
graph.kv_set(
    b"temp_session".to_vec(),
    KvValue::String("active".to_string()),
    Some(60),  // Expires in 60 seconds
)?;

// Set a key that expires in 1 hour
graph.kv_set(
    b"cache:data".to_vec(),
    KvValue::Bytes(cached_data),
    Some(3600),  // 1 hour
)?;
```

### Transactional Behavior

KV operations are **atomic** with graph operations within the same transaction:

```rust
// Create a node and store metadata atomically
let node_id = graph.insert_node(NodeSpec {
    kind: "User".to_string(),
    name: "alice".to_string(),
    file_path: None,
    data: json!({"age": 30}),
})?;

// Store metadata in KV - commits with the transaction
graph.kv_set(
    format!("user_metadata:{}", node_id).into_bytes(),
    KvValue::Json(json!({
        "created_at": "2026-02-03",
        "verified": true
    })),
    None,
)?;

// If commit fails, both node and KV data are rolled back
```

### Use Cases

#### 1. Secondary Indexes

```rust
// Index users by email for fast lookup
let node_id = graph.insert_node(user_spec)?;
graph.kv_set(
    format!("index:email:{}", user_email).into_bytes(),
    KvValue::Integer(node_id),
    None,
)?;

// Later, find user by email
if let Some(KvValue::Integer(node_id)) = graph.kv_get(snapshot.id, b"index:email:alice@example.com")? {
    let user = graph.get_node(snapshot.id, node_id)?;
}
```

#### 2. Caching

```rust
// Cache expensive computation results
graph.kv_set(
    b"cache:expensive_result".to_vec(),
    KvValue::Json(json!(result)),
    Some(300),  // Cache for 5 minutes
)?;
```

#### 3. Counters and Aggregates

```rust
// Track counts (read-modify-write pattern)
let count = match graph.kv_get(snapshot.id, b"counter:requests")? {
    Some(KvValue::Integer(n)) => n + 1,
    _ => 1,
};
graph.kv_set(b"counter:requests".to_vec(), KvValue::Integer(count), None)?;
```

#### 4. Configuration

```rust
// Store application configuration
graph.kv_set(
    b"config:max_connections".to_vec(),
    KvValue::Integer(100),
    None,
);
graph.kv_set(
    b"config:debug_mode".to_vec(),
    KvValue::Boolean(false),
    None,
);
```

### Pub/Sub Integration

KV changes emit `KVChanged` events for subscribers:

```rust
use sqlitegraph::backend::{SubscriptionFilter, PubSubEvent};

let filter = SubscriptionFilter::all();
let (sub_id, rx) = graph.subscribe(filter)?;

// In a separate thread
while let Ok(event) = rx.recv() {
    if let PubSubEvent::KVChanged { key_hash, snapshot_id } = event {
        println!("KV changed: hash={}, snapshot={}", key_hash, snapshot_id);
    }
}
```

### Metadata

Each KV entry has internal metadata (not directly exposed):
- `created_at` - Unix timestamp when key was created
- `updated_at` - Unix timestamp of last update
- `ttl_seconds` - TTL if set
- `version` - Monotonically increasing version number

### Limitations

- **Native V2 only**: SQLite backend does not support KV operations
- **Byte keys**: Keys are `Vec<u8>` - use string encoding for text keys
- **No snapshots**: Can't query historical KV values, only current snapshot
- **Full enumeration**: No API to enumerate all keys without prefix (use `kv_prefix_scan(b"")` for all keys)

### KV Prefix Scanning

The `kv_prefix_scan()` method enables efficient retrieval of all KV entries with a given prefix:

```rust
use sqlitegraph::{GraphConfig, open_graph};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;

// Get all keys with prefix "user:"
let snapshot = graph.snapshot()?;
let results = graph.kv_prefix_scan(snapshot.id, b"user:")?;

for (key, value) in results {
    println!("{:?} = {:?}", String::from_utf8_lossy(&key), value);
}

// Get all KV entries (empty prefix)
let all_entries = graph.kv_prefix_scan(snapshot.id, b"")?;
```

**Features:**
- Prefix matching for hierarchical key organization
- Results returned in lexicographic order
- MVCC snapshot isolation respected
- TTL filtering for expired entries

**Use Cases:**
- Secondary index enumeration (e.g., `index:user:*` → all user IDs)
- Hierarchical data retrieval (e.g., `cache:region:*` → all regional caches)
- Namespace-based key management

**CLI:**
```bash
# Scan all keys with prefix "user:"
sqlitegraph --backend native-v2 --db mygraph.db kv-scan --prefix "user:"

# Scan all KV entries
sqlitegraph --backend native-v2 --db mygraph.db kv-scan --prefix ""
```

---

## 10. Query API Enhancements (Phase 58)

### Overview

Phase 58 introduces query API enhancements that make it easier to work with graph data without maintaining external ID tracking. These features are particularly useful for pub/sub use cases like agent messaging and topic-based subscriptions.

### Query Nodes by Kind

Find all nodes with a specific kind:

```rust
use sqlitegraph::{GraphConfig, open_graph};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;

// Find all agent nodes
let snapshot = graph.snapshot()?;
let agent_ids = graph.query_nodes_by_kind(snapshot.id, "agent")?;

println!("Found {} agents", agent_ids.len());
for node_id in agent_ids {
    let node = graph.get_node(snapshot.id, node_id)?;
    println!("  - {}: {:?}", node.name, node.data);
}
```

**Features:**
- Direct kind filtering without full graph scan
- Works on both SQLite and Native V2 backends
- Returns sorted node IDs for consistent output
- MVCC snapshot isolation respected

**CLI:**
```bash
# Find all nodes with kind "agent"
sqlitegraph mygraph.db nodes-by-kind --kind "agent"

# Find all message nodes
sqlitegraph mygraph.db nodes-by-kind --kind "message"
```

### Query Nodes by Name Pattern

Find nodes using glob patterns (`*` matches any sequence, `?` matches single character):

```rust
// Find all nodes with name matching "msg_index:*"
let msg_ids = graph.query_nodes_by_name_pattern(snapshot.id, "msg_index:*")?;

// Find nodes with pattern "agent-?" (single digit)
let agent_ids = graph.query_nodes_by_name_pattern(snapshot.id, "agent-?")?;

// Escape wildcards for literal matching
let literal = graph.query_nodes_by_name_pattern(snapshot.id, "file\\*.txt")?;
```

**Pattern Syntax:**
| Pattern | Matches | Does Not Match |
|---------|---------|----------------|
| `msg_index:*` | `msg_index:agent-1`, `msg_index:agent-2` | `Message_Index:agent-1` |
| `agent-?` | `agent-1`, `agent-A` | `agent-12`, `agent-` |
| `\*test\?` | `*test?` | `test`, `123testX` |

**CLI:**
```bash
# Find nodes matching "msg_index:*"
sqlitegraph mygraph.db nodes-by-name --pattern "msg_index:*"

# Find nodes with single-character suffix
sqlitegraph mygraph.db nodes-by-name --pattern "agent-?"
```

### Pub/Sub Pattern Filters

Subscribe to events matching glob patterns on node kind or name:

```rust
use sqlitegraph::backend::SubscriptionFilter;

// Subscribe to all agent events (kind pattern)
let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);
let (sub_id, rx) = graph.subscribe(filter)?;

// Subscribe to message index events (name pattern)
let filter = SubscriptionFilter::name_patterns(vec!["msg_index:*".to_string()]);
let (sub_id, rx) = graph.subscribe(filter)?;

// Multiple patterns
let filter = SubscriptionFilter::kind_patterns(vec![
    "agent:*".to_string(),
    "message:*".to_string(),
    "system:*".to_string(),
]);
let (sub_id, rx) = graph.subscribe(filter)?;
```

**How Pattern Matching Works:**
1. When a node event occurs (creation/modification), the node's kind/name is checked
2. Patterns are evaluated in order; if any pattern matches, the event is delivered
3. Matching is case-sensitive
4. Supports `*` (any sequence, including empty) and `?` (exactly one character)
5. Escape with `\*` and `\?` for literal asterisk/question mark

**Use Cases:**
- **Agent Messaging**: Subscribe to `agent:*` to receive all agent events
- **Topic-Based Pub/Sub**: Subscribe to `msg_index:agent-*` for specific agent messages
- **Hierarchical Organization**: Subscribe to `cache:region-*` for regional cache events
- **Dynamic Discovery**: Find nodes by pattern without maintaining ID registries

### Query API Use Cases

#### 1. Agent Messaging System

```rust
// Create message queue node
let msg_node_id = graph.insert_node(NodeSpec {
    kind: "message_queue".to_string(),
    name: "msg_index:agent-123".to_string(),
    file_path: None,
    data: json!({"owner": "agent-123"}),
})?;

// Subscribe to this agent's messages
let filter = SubscriptionFilter::name_patterns(vec!["msg_index:agent-123".to_string()]);
let (sub_id, rx) = graph.subscribe(filter)?;

// Or subscribe to all agents' messages
let filter = SubscriptionFilter::name_patterns(vec!["msg_index:*".to_string()]);
let (sub_id, rx) = graph.subscribe(filter)?;
```

#### 2. Dynamic Entity Discovery

```rust
// Find all agents without maintaining ID registry
let agent_ids = graph.query_nodes_by_kind(snapshot.id, "agent")?;

for agent_id in agent_ids {
    let agent = graph.get_node(snapshot.id, agent_id)?;
    println!("Active agent: {}", agent.name);
}
```

#### 3. Hierarchical KV Indexing

```rust
// Index users by email
graph.kv_set(
    format!("index:email:{}", email).into_bytes(),
    KvValue::Integer(user_id),
    None,
)?;

// Later, enumerate all users in the index
let all_users = graph.kv_prefix_scan(snapshot.id, b"index:email:")?;
for (key, value) in all_users {
    let email = String::from_utf8_lossy(&key).replace("index:email:", "");
    println!("User: {} -> ID: {:?}", email, value);
}
```

#### 4. Topic-Based Subscriptions

```rust
// Subscribe to all cache events
let cache_filter = SubscriptionFilter::kind_patterns(vec!["cache:*".to_string()]);
let (sub_id, rx) = graph.subscribe(cache_filter)?;

// Subscribe to specific cache region
let region_filter = SubscriptionFilter::name_patterns(vec!["cache:region-us-west".to_string()]);
let (sub_id, rx) = graph.subscribe(region_filter)?;
```

### Backend Support

| Feature | Native V2 | SQLite |
|---------|-----------|--------|
| `kv_prefix_scan()` | Full support | Full support |
| `query_nodes_by_kind()` | Full support | Full support |
| `query_nodes_by_name_pattern()` | Full support | Full support |
| Pattern subscription filters | Full support | Full support |

### Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| `kv_prefix_scan()` | O(K) where K = keys scanned | Faster with specific prefixes |
| `query_nodes_by_kind()` | O(N) where N = total nodes | Full scan required |
| `query_nodes_by_name_pattern()` | O(N) where N = total nodes | Full scan + pattern match |
| Pattern matching | O(P) where P = patterns | Per-event, cheap |

**Optimization Tips:**
- Use specific prefixes for `kv_prefix_scan()` to avoid scanning all keys
- Consider maintaining a separate index if querying by kind/name frequently
- Use kind-based filtering before name patterns for hierarchical filtering

---

## 11. Developer Tools (Phase 9)

### Introspection API

```rust
use sqlitegraph::introspection::GraphIntrospection;

let intro = GraphIntrospection::new(&graph)?;
println!("Nodes: {}", intro.node_count()?);
println!("Edges (estimated): {}", intro.edge_count_estimate()?);
println!("JSON: {}", intro.to_json()?);
```

### Progress Tracking

```rust
use sqlitegraph::progress::{ProgressCallback, ConsoleProgress};

// No-op (default)
let scores = algo::pagerank_with_progress(&graph, 0.85, 50, NoProgress)?;

// Console progress bars
let scores = algo::pagerank_with_progress(&graph, 0.85, 50, ConsoleProgress::new())?;
```

### CLI Debug Commands

```bash
# Statistics
sqlitegraph --backend sqlite --db mygraph.db debug-stats

# Dump graph
sqlitegraph --backend sqlite --db mygraph.db debug-dump --output graph.json

# Trace operations
sqlitegraph --backend sqlite --db mygraph.db debug-trace
```

---

## 12. Safety & Integrity

### Safety Checks

```rust
use sqlitegraph::run_safety_checks;

let report = run_safety_checks(&graph)?;
if report.has_orphans() {
    eprintln!("Warning: {} orphan edges", report.orphan_count());
}
```

### V2 WAL Recovery

The Native V2 backend includes WAL recovery with:
- Transaction rollback for all operations
- Edge cascade cleanup on node deletion
- Cluster reference cleanup

**Tested**: 42 WAL tests passing (recovery, corruption, checkpoints)

---

## 13. CLI Usage

### Available Commands

```bash
# Status
sqlitegraph --command status --database mygraph.db

# List entities
sqlitegraph --command list --database mygraph.db

# Export/Import
sqlitegraph --command dump-graph --output backup.json --database mygraph.db
sqlitegraph --command load-graph --input backup.json --database mygraph.db

# Safety check
sqlitegraph --command safety-check --database mygraph.db

# Graph algorithms (35+ algorithms available)
sqlitegraph --backend sqlite --db mygraph.db pagerank --progress
sqlitegraph --backend sqlite --db mygraph.db betweenness --progress
sqlitegraph --backend sqlite --db mygraph.db louvain --progress

# CFG analysis
sqlitegraph --backend sqlite --db mygraph.db dominators --entry 1
sqlitegraph --backend sqlite --db mygraph.db natural-loops --entry 1

# Program slicing
sqlitegraph --backend sqlite --db mygraph.db backward-slice --target 42
sqlitegraph --backend sqlite --db mygraph.db forward-slice --source 1

# Reachability
sqlitegraph --backend sqlite --db mygraph.db forward-reach --start 1
sqlitegraph --backend sqlite --db mygraph.db can-reach --from 1 --to 100

# For complete CLI reference, see docs/GRAPH_ALGORITHMS_GUIDE.md
```

---

## 14. Migration

### SQLite to Native V2

```rust
// Before (SQLite)
let graph = SqliteGraph::open("data.db")?;
let entity = GraphEntity { /* fields */ };
let id = graph.insert_entity(&entity)?;

// After (Native V2)
let config = GraphConfig::native();
let graph = open_graph("data.db", &config)?;
let node_spec = NodeSpec { /* similar fields */ };
let id = graph.insert_node(node_spec)?;
```

### Key Differences

| Aspect | SQLite Backend | Native V2 Backend |
|--------|----------------|-------------------|
| **Data Types** | `GraphEntity`/`GraphEdge` | `NodeSpec`/`EdgeSpec` |
| **Edge Fields** | `from_id`/`to_id` | `from`/`to` |
| **Construction** | `SqliteGraph::open()` | `open_graph(&config)` |

---

## 15. Troubleshooting

### Common Issues

**Compilation Errors:**
- Add feature flags: `--features native-v2`
- Check backend-specific data types

**Runtime Issues:**
- Run integrity checks: `safety-check` command
- Check buffer configuration for large graphs

**Performance:**
- Use Native V2 for traversals
- Enable parallel WAL recovery for large databases

### Getting Help

```bash
# Check test status
cargo test --lib 2>&1 | tail -5

# Run specific test with output
cargo test test_name -- --nocapture

# Check compilation
cargo check --features native-v2
```

---

## 16. Pub/Sub Events (Phase 44)

### Overview

The Native V2 backend includes an in-process publish/subscribe system for receiving notifications when graph data changes. Events are emitted when transactions commit and carry only identifiers (not full data payloads).

### Availability

| Backend | Pub/Sub Support |
|---------|-----------------|
| **Native V2** | Full support |
| **SQLite** | Not supported (returns `Unsupported` error) |

### Event Types

Four event types are emitted on transaction commit:

| Event Type | Fields | Description |
|------------|--------|-------------|
| `NodeChanged` | `node_id`, `snapshot_id` | A node was created or modified |
| `EdgeChanged` | `edge_id`, `snapshot_id` | An edge was created or modified |
| `KVChanged` | `key_hash`, `snapshot_id` | A KV entry was created, modified, or deleted |
| `SnapshotCommitted` | `snapshot_id` | A transaction was committed |

**Important**: Events are emitted on **commit only**, not on rollback.

### Basic Usage

```rust
use sqlitegraph::{GraphConfig, open_graph};
use sqlitegraph::backend::{SubscriptionFilter, PubSubEvent};

let cfg = GraphConfig::native();
let graph = open_graph("graph.db", &cfg)?;

// Subscribe to all events
let filter = SubscriptionFilter::all();
let (subscriber_id, rx) = graph.subscribe(filter)?;

// In a separate task/thread, receive events
std::thread::spawn(move || {
    while let Ok(event) = rx.recv() {
        match event {
            PubSubEvent::NodeChanged { node_id, snapshot_id } => {
                println!("Node {} changed in snapshot {}", node_id, snapshot_id);
            }
            PubSubEvent::EdgeChanged { edge_id, snapshot_id } => {
                println!("Edge {} changed in snapshot {}", edge_id, snapshot_id);
            }
            PubSubEvent::KVChanged { key_hash, snapshot_id } => {
                println!("KV hash {} changed in snapshot {}", key_hash, snapshot_id);
            }
            PubSubEvent::SnapshotCommitted { snapshot_id } => {
                println!("Transaction committed: snapshot {}", snapshot_id);
            }
        }
    }
});

// Unsubscribe when done
graph.unsubscribe(subscriber_id)?;
```

### Filtering Events

You can filter events by type and/or specific entity IDs:

```rust
use sqlitegraph::backend::{SubscriptionFilter, PubSubEventType};

// Subscribe only to node events
let node_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Node]);
let (id, rx) = graph.subscribe(node_filter)?;

// Subscribe only to specific node IDs
let specific_nodes = SubscriptionFilter::nodes(vec![1, 2, 3]);
let (id, rx) = graph.subscribe(specific_nodes)?;

// Subscribe to node AND edge events
let multi_filter = SubscriptionFilter::event_types(vec![
    PubSubEventType::Node,
    PubSubEventType::Edge,
]);
let (id, rx) = graph.subscribe(multi_filter)?;
```

### SubscriptionFilter API

| Method | Description |
|--------|-------------|
| `all()` | Match all events |
| `nodes(ids)` | Match only specific node IDs |
| `edges(ids)` | Match only specific edge IDs |
| `keys(hashes)` | Match only specific key hashes |
| `event_types(types)` | Match specific event types |

### ID-Only Design

Events carry only identifiers, not full entity data. This design:

- **Reduces overhead**: Events are lightweight (just IDs)
- **Ensures consistency**: Consumers read from a specific snapshot
- **Decouples schema**: Event structure doesn't change when entities change

To read actual data, use the provided `snapshot_id`:

```rust
// Event gives you the ID
if let PubSubEvent::NodeChanged { node_id, snapshot_id } = event {
    // Query the graph for actual data at that snapshot
    let node = graph.get_node_at_snapshot(node_id, snapshot_id)?;
}
```

### Limitations

The pub/sub system is **minimal and best-effort**:

- **In-Process Only**: No networking or IPC support
- **No Persistence**: Events are lost if the process crashes
- **No Delivery Guarantees**: Events dropped if channel is full or receiver is gone
- **No Ordering**: Subscribers may receive events in different orders
- **Native V2 Only**: SQLite backend does not support pub/sub

### Thread Safety

- Multiple threads can safely subscribe/unsubscribe concurrently
- Each subscriber gets their own channel
- `Publisher` uses `Arc<Mutex<>>` for internal synchronization

### Query API Limitations

**Important**: The GraphBackend API does not provide methods to query nodes by name and kind directly.

```rust
// What you CAN do:
fn insert_node(NodeSpec) -> Result<i64>  // Creates node, returns ID
fn get_node(snapshot_id, node_id) -> Result<GraphEntity>  // Requires ID

// What you CANNOT do (no such API):
// fn get_nodes_by_name_kind(name, kind) -> Result<Vec<Node>>
// fn find_node(name, kind) -> Result<Option<Node>>
```

**Workarounds**:

1. **Track node IDs yourself**: When creating a node, store its ID in your own tracking structure or in the KV store:

```rust
// Create node and track the ID
let node_id = graph.insert_node(NodeSpec {
    kind: "message_index".to_string(),
    name: "msg_index:agent-123".to_string(),
    file_path: None,
    data: serde_json::json!({}),
})?;

// Store ID in KV for later lookup
graph.kv_set(
    b"msg_index_id:agent-123",
    KvValue::Integer(node_id),
    None,
)?;
```

2. **Use KV as an index**: For message queues, user sessions, or other dynamic entities, maintain an index in KV:

```rust
// Index pattern: store entity IDs in KV
// Key: "index:{entity_type}:{entity_name}" -> node_id
// Data: "index_data:{entity_type}:{entity_name}" -> JSON with actual state

graph.kv_set(
    b"index:messages:agent-123",
    KvValue::Integer(node_id),
    None,
)?;
```

3. **Use SubscriptionFilter.nodes()**: When subscribing to events for specific nodes, you need their IDs. Plan your node creation to return IDs to interested parties.

**Why this design?**

- **Simplicity**: Avoids complex query engine implementation
- **Performance**: Direct ID lookup is O(1)
- **Explicit tracking**: Users control their own index strategies
- **Extensibility**: KV store provides flexible indexing options

### Test Coverage

**v1.4.1 Test Results:**
- 59 pubsub tests passing (integration + module tests)
- Tests cover: event emission, filtering, multiple subscribers, unsubscribe

---

## Architecture Status

**v1.4.1 Features:**
- Graph Algorithms Library: 35 production algorithms across 13 categories
  - Core Graph Theory: WCC, SCC, Transitive Closure, Transitive Reduction, Topological Sort
  - Reachability: Forward, Backward, Can-Reach, Unreachable Nodes
  - CFG Analysis: Dominators, Post-Dominators, Control Dependence, Dominance Frontiers, Natural Loops
  - Path Analysis: Enumerate Paths, Dominance-Constrained Path Enumeration
  - Dependency Systems: Critical Path, Minimal Cycle Basis
  - Program Analysis: Backward/Forward Slicing, SCC Collapse for Call Graphs
  - Distributed Systems: Min Cut, Min Vertex Cut, Graph Partitioning
  - Observability: Happens-Before Analysis, Impact Radius
  - ML/Inference: Subgraph Isomorphism, Graph Rewriting, Structural Similarity
  - Graph Diff: Structural Delta, Refactor Validation
  - Security: Taint Propagation (Forward/Backward), Sink Analysis, Source/Sink Discovery
  - CLI commands for all 35 algorithms with progress tracking

**v1.2 Features:**
- Pub/Sub Events: In-process event notification for graph changes
  - Four event types (NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted)
  - Channel-based delivery with filtering
  - Best-effort delivery (no blocking on commit path)
  - Native V2 backend only

**v1.1 Features:**
- Native V2 Backend: Full ACID transaction correctness
  - Atomicity: Complete rollback for all operations
  - Consistency: Runtime validation (cluster overlap, checkpoint state)
  - Isolation: Transaction coordinator with deadlock detection
  - Durability: All checkpoint strategies functional
- Memory Safety: All unsafe transmute sites eliminated (Arc<RwLock<GraphFile>>)
- Connection Pooling: r2d2 pool for SQLite backend (4-5x throughput)
- Data Management: Migration API, Backup/Restore APIs, v3 file format

**v1.0 Features:**
- Native V2 Backend: Clustered adjacency with WAL
- Dual Backend Support: Unified API
- Graph Algorithms: 4 production algorithms
- HNSW Vector Search: Full persistence support
- MVCC Snapshots: Read isolation
- Developer Tools: Introspection, progress tracking, CLI

**Test Coverage:** 530+ tests passing (v1.4.1, including 180+ algorithm tests and 59 pubsub tests)

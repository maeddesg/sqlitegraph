# SQLiteGraph Architecture

**Last Updated:** 2026-01-26
**Version:** v1.2.0

This document describes the architecture of SQLiteGraph from a developer's perspective. For user-facing documentation, see [README.md](../README.md) and [MANUAL.md](../MANUAL.md).

---

## Table of Contents

1. [High-Level Overview](#high-level-overview)
2. [Directory Structure](#directory-structure)
3. [Core Components](#core-components)
4. [Backend Architecture](#backend-architecture)
5. [Data Flow](#data-flow)
6. [Key Design Decisions](#key-design-decisions)
7. [Module Reference](#module-reference)

---

## High-Level Overview

SQLiteGraph is an embedded graph database with dual storage backends:

```
┌─────────────────────────────────────────────────────────────┐
│                     User Application                        │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                   SqliteGraph (Unified API)                 │
│  - GraphEntity, GraphEdge, NodeSpec, EdgeSpec               │
│  - insert_entity, insert_edge, neighbors, traversal         │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┴───────────────┐
          ▼                               ▼
┌─────────────────────┐         ┌─────────────────────┐
│  SQLite Backend     │         │  Native V2 Backend  │
│  (ACID, Ecosystem)  │         │  (Performance)      │
├─────────────────────┤         ├─────────────────────┤
│ - SQL storage       │         │ - Binary file format│
│ - Full transactions │         │ - Clustered edges   │
│ - Raw SQL access    │         │ - WAL logging       │
└─────────────────────┘         │ - Memory-mapped I/O │
                                └─────────────────────┘
```

### Key Architectural Principles

1. **Dual Backend Strategy**: SQLite for features, Native V2 for performance
2. **Unified API**: Same interface regardless of backend choice
3. **MVCC Isolation**: Snapshot-based reads without blocking writers
4. **Clustered Storage**: V2 stores edges in clusters for I/O locality
5. **Pluggable Algorithms**: Graph algorithms, HNSW vector search

---

## Directory Structure

```
sqlitegraph/
├── Cargo.toml                 # Main library manifest
├── src/
│   ├── lib.rs                 # Public API exports
│   ├── error.rs               # Error types
│   ├── graph/                 # Core graph database
│   ├── backend/               # Storage abstraction
│   │   ├── mod.rs             # GraphBackend trait
│   │   ├── sqlite/            # SQLite backend implementation
│   │   └── native/            # Native backend
│   │       ├── v1/            # Legacy V1 format
│   │       └── v2/            # Current V2 format
│   │           ├── wal/       # Write-Ahead Logging
│   │           ├── node/      # Node storage
│   │           ├── edge/      # Edge storage (clustered)
│   │           ├── kv/        # Transactional KV store
│   │           └── pubsub/    # Pub/Sub event system
│   ├── algo/                  # Graph algorithms
│   ├── hnsw/                  # Vector similarity search
│   ├── pattern_engine/        # Triple pattern matching
│   ├── mvcc.rs                # MVCC snapshot system
│   ├── query/                 # High-level query interface
│   ├── introspection/         # Debug/introspection APIs
│   ├── cache/                 # LRU-K adjacency cache
│   ├── config.rs              # Configuration types
│   └── debug.rs               # Debug logging (feature-gated)
├── tests/                     # Integration tests
│   └── helpers/               # Test utilities
└── benches/                   # Criterion benchmarks

sqlitegraph-cli/
├── src/
│   ├── main.rs                # CLI entry point
│   └── client.rs              # Backend wrapper
└── Cargo.toml                 # CLI manifest
```

---

## Core Components

### 1. GraphBackend Trait

The `GraphBackend` trait defines the abstraction layer between the unified API and storage backends.

**Location:** `src/backend/mod.rs`

```rust
pub trait GraphBackend: Send + Sync {
    // Node operations
    fn insert_node(&self, spec: NodeSpec) -> Result<u64, SqliteGraphError>;
    fn get_node(&self, id: u64) -> Result<NodeData, SqliteGraphError>;
    fn update_node(&self, spec: NodeSpec) -> Result<(), SqliteGraphError>;
    fn delete_node(&self, id: u64) -> Result<(), SqliteGraphError>;

    // Edge operations
    fn insert_edge(&self, edge: EdgeSpec) -> Result<u64, SqliteGraphError>;
    fn get_edge(&self, id: u64) -> Result<EdgeData, SqliteGraphError>;
    fn delete_edge(&self, id: u64) -> Result<(), SqliteGraphError>;

    // Traversal
    fn neighbors(&self, query: NeighborQuery) -> Result<Vec<Neighbor>, SqliteGraphError>;

    // Snapshots
    fn snapshot(&self) -> Result<Box<dyn GraphSnapshot>, SqliteGraphError>;

    // Pub/Sub (Native V2 only)
    fn subscribe(&self, filter: SubscriptionFilter)
        -> Result<(SubscriberId, Receiver<PubSubEvent>), SqliteGraphError>;
    fn unsubscribe(&self, id: SubscriberId) -> Result<(), SqliteGraphError>;
}
```

### 2. MVCC Snapshot System

**Location:** `src/mvcc.rs`

Provides multi-version concurrency control for read isolation.

```rust
pub struct SnapshotManager {
    inner: Arc<SnapshotState>,
}

pub struct SnapshotState {
    current: ArcSwap<SnapshotData>,  // Lock-free atomic update
}

pub trait GraphSnapshot {
    fn get_node(&self, id: u64) -> Result<NodeData, SqliteGraphError>;
    fn neighbors(&self, query: NeighborQuery) -> Result<Vec<Neighbor>, SqliteGraphError>;
}
```

**Guarantees:**
- Readers never block writers
- Writers never block readers
- Each snapshot sees a consistent, isolated view
- No dirty reads within snapshots

### 3. Cache Layer

**Location:** `src/cache/`

LRU-K cache for adjacency lists to reduce I/O.

```rust
pub struct AdjacencyCache {
    // LRU-K caching with configurable K (default: 2)
    // Separate caches for incoming/outgoing edges
}
```

---

## Backend Architecture

### SQLite Backend

**Location:** `src/backend/sqlite/`

| Aspect | Implementation |
|--------|----------------|
| **Storage** | SQLite database file |
| **Schema** | `entities` table, `edges` table |
| **Transactions** | Full ACID via SQLite |
| **Concurrency** | Database-level locks |
| **Raw SQL** | Available via `conn()` method |

**Schema:**
```sql
CREATE TABLE entities (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    file_path TEXT,
    data JSON
);

CREATE TABLE edges (
    id INTEGER PRIMARY KEY,
    from_id INTEGER NOT NULL,
    to_id INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    data JSON,
    FOREIGN KEY (from_id) REFERENCES entities(id),
    FOREIGN KEY (to_id) REFERENCES entities(id)
);
```

### Native V2 Backend

**Location:** `src/backend/native/v2/`

The Native V2 backend is a custom file format optimized for graph workloads.

#### File Format

```
graph.db (main file)
├── Header (512 bytes)
│   ├── Magic number: "SQLGv2"
│   ├── Version: 2
│   ├── Node region offset
│   ├── Edge cluster region offset
│   ├── WAL region offset
│   └── Checkpoint state
├── Node Region (max 8MB, ~2048 nodes)
│   ├── Node slots (256 bytes each)
│   └── Node data (kind, name, JSON)
├── Edge Cluster Region
│   ├── Clusters (64KB each)
│   │   ├── Cluster header
│   │   └── Sequential edge records
│   └── Free block bitmap
└── WAL Region
    ├── WAL header
    └── WAL records (variable size)

graph.db.wal (write-ahead log)
├── WAL header
└── WAL records (append-only)
```

#### Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| **V2GraphFile** | `graph_file.rs` | Memory-mapped file operations |
| **V2NodeStore** | `node/` | Node storage with slot allocation |
| **V2EdgeStore** | `edge/` | Clustered edge storage |
| **V2WALManager** | `wal/manager.rs` | Transaction logging and recovery |
| **V2KVStore** | `kv/` | Transactional key-value storage |
| **Publisher** | `pubsub/publisher.rs` | Event broadcasting |
| **TransactionCoordinator** | `transaction/` | MVCC transaction management |

#### Clustered Edge Storage

Edges are stored in clusters (64KB blocks) for I/O locality:

```
Cluster Layout:
┌─────────────────────────────────────┐
│ Cluster Header (64 bytes)           │
│ - cluster_id: u64                   │
│ - edge_count: u32                   │
│ - next_cluster: Option<u64>         │
├─────────────────────────────────────┤
│ Edge Record 1 (variable)            │
│ - from_id: u64                      │
│ - to_id: u64                        │
│ - edge_type: string                 │
│ - data: JSON                        │
├─────────────────────────────────────┤
│ Edge Record 2 (variable)            │
├─────────────────────────────────────┤
│ ...                                 │
└─────────────────────────────────────┘
```

**Benefits:**
- Sequential writes within cluster
- Better cache locality
- Reduced random I/O

#### WAL (Write-Ahead Logging)

**Location:** `src/backend/native/v2/wal/`

```rust
pub enum V2WALRecord {
    InsertNode { node_id: u64, spec: NodeSpec },
    UpdateNode { node_id: u64, spec: NodeSpec },
    DeleteNode { node_id: u64 },
    InsertEdge { edge_id: u64, spec: EdgeSpec },
    DeleteEdge { edge_id: u64 },
    KVPut { key_hash: u64, value: Vec<u8> },
    KVDelete { key_hash: u64 },
    BeginCheckpoint,
    EndCheckpoint,
}
```

**Recovery Process:**
1. Open main file and WAL
2. Read WAL from beginning
3. Reapply committed transactions
4. Discard uncommitted transactions
5. Trigger checkpoint if needed

#### Pub/Sub Events

**Location:** `src/backend/native/v2/pubsub/`

Events emitted on commit only (not rollback):

```rust
pub enum PubSubEvent {
    NodeChanged { node_id: u64, snapshot_id: u64 },
    EdgeChanged { edge_id: u64, snapshot_id: u64 },
    KVChanged { key_hash: u64, snapshot_id: u64 },
    SnapshotCommitted { snapshot_id: u64 },
}
```

**Design:** ID-only events — consumers read actual data from snapshot APIs.

---

## Data Flow

### Read Path (Node Query)

```
User Code
    │
    ▼
graph.get_node(id)
    │
    ▼
GraphBackend::get_node(id)
    │
    ├─────────────────┐
    ▼                 ▼
SQLite Backend    Native V2 Backend
    │                 │
    ▼                 ▼
SELECT * FROM      V2NodeStore::get()
entities           │
WHERE id = ?        ▼
    │           Check node slot
    ▼                 │
Return entity      ├────────────┐
                    ▼            ▼
                Cache hit?    Read from
                    │           graph.db
                    ▼
                Return node
```

### Write Path (Insert Edge)

```
User Code
    │
    ▼
graph.insert_edge(spec)
    │
    ▼
Begin Transaction
    │
    ▼
GraphBackend::insert_edge(spec)
    │
    ├─────────────────┐
    ▼                 ▼
SQLite Backend    Native V2 Backend
    │                 │
    ▼                 ▼
BEGIN              V2WALManager::begin()
    │                 │
INSERT INTO edges   │
VALUES (...)        ▼
    │           Write WAL record
    │                 │
    │                 ▼
COMMIT            V2WALManager::commit()
    │                 │
    │                 ├──────────────┐
    │                 ▼              ▼
    │            Append to       Emit events
    │            graph.db.wal     (pubsub)
    │                 │
    ▼                 ▼
Return             Return
```

### Traversal Path (BFS)

```
User Code
    │
    ▼
algo::bfs(&graph, start, max_depth)
    │
    ▼
For each level:
    │
    ▼
graph.neighbors(node_id, query)
    │
    ├─────────────────┐
    ▼                 ▼
SQLite Backend    Native V2 Backend
    │                 │
    ▼                 ▼
SELECT to_id       V2EdgeStore::get_cluster()
FROM edges         │
WHERE from_id = ?  ▼
    │           Read cluster (64KB)
    │                 │
    ▼                 ▼
Return list       Iterate cluster
                    (sequential I/O)
                        │
                        ▼
                    Return list
```

---

## Key Design Decisions

### 1. Dual Backend Architecture

**Decision:** Support both SQLite and Native V2 backends.

**Rationale:**
- SQLite: Proven reliability, ecosystem tools, ACID guarantees
- Native V2: Optimized for graph workloads, clustered edges, WAL

**Trade-offs:**
- Complexity: Must maintain two storage engines
- Testing: Double the test surface
- Benefit: Users choose based on workload

### 2. Clustered Edge Storage

**Decision:** Store edges in 64KB clusters instead of individual records.

**Rationale:**
- Graph traversals follow adjacency lists
- Sequential I/O is 10-100x faster than random I/O
- Clusters keep related edges together

**Trade-offs:**
- Fragmentation: Clusters can become partially full
- Rebalancing: May need to move edges between clusters
- Benefit: 1.6-10x faster for star patterns

### 3. ID-Only Pub/Sub Events

**Decision:** Events carry only IDs, not full entity data.

**Rationale:**
- Decouples event schema from entity schema
- Reduces event overhead (no JSON serialization)
- Consumers read from snapshot for consistency

**Trade-offs:**
- Extra read: Consumers must query graph for data
- Complexity: Consumers need snapshot handling
- Benefit: No event breaking on schema changes

### 4. MVCC-Lite

**Decision:** Snapshot-based reads without full transaction isolation.

**Rationale:**
- Full MVCC is complex (locking, validation, conflict resolution)
- SQLite handles transactions internally
- Readers need consistent views, not write conflicts

**Trade-offs:**
- Write-write conflicts: Not handled at application level
- Stale reads: Snapshots may be behind current state
- Benefit: Simpler implementation, sufficient for embedded use

### 5. 8MB Node Region Limit

**Decision:** Reserve 8MB for node storage (~2048 nodes max).

**Rationale:**
- Fixed offset in header allows direct seeking
- 256 bytes per node slot (generous for names + JSON)
- Keeps header simple

**Trade-offs:**
- Scalability: Limited to ~2K nodes
- Fragmentation: Large slots waste space
- Benefit: Simple format, fast lookup
- **Note:** This is a known limitation; future versions may address this.

---

## Module Reference

### Graph Algorithms (`src/algo/`)

| Algorithm | Complexity | File | Description |
|-----------|------------|------|-------------|
| PageRank | O(k × \|E\|) | `pagerank.rs` | Importance ranking |
| Betweenness | O(\|V\| × \|E\|) | `betweenness.rs` | Bridge nodes |
| Label Propagation | O(k × \|E\|) | `label_prop.rs` | Fast clustering |
| Louvain | O(\|E\| log \|V\|) | `louvain.rs` | Modularity optimization |
| BFS | O(\|V\| + \|E\|) | `bfs.rs` | Breadth-first search |
| Connected Components | O(\|V\| + \|E\|) | `components.rs` | Weak connectivity |

### HNSW Vector Search (`src/hnsw/`)

| Component | File | Description |
|-----------|------|-------------|
| Index | `index.rs` | Main HNSW index |
| Config | `config.rs` | Builder for HNSW parameters |
| Distance | `distance.rs` | Distance metric implementations |
| Storage | `storage.rs` | SQLite-backed vector storage |
| Search | `search.rs` | ANN search algorithm |

### Introspection (`src/introspection/`)

| API | Purpose |
|-----|---------|
| `GraphIntrospection::node_count()` | Exact node count |
| `GraphIntrospection::edge_count_estimate()` | Min/max edge estimate |
| `GraphIntrospection::backend_info()` | Backend-specific stats |
| `GraphIntrospection::to_json()` | Full state export |

---

## Performance Characteristics

### Backend Comparison

| Operation | SQLite | Native V2 | Ratio |
|-----------|--------|-----------|-------|
| Node Insert (100) | 3.63 ms | 1.14 ms | 3.2x faster |
| Edge Insert (star) | 7.18 ms | 3.85 ms | 1.9x faster |
| BFS Star (100) | 7.28 ms | 4.68 ms | 1.6x faster |
| BFS Chain (500) | 24.98 ms | 266.50 ms | 10.7x **slower** |
| 1-Hop Query (100) | 6.93 ms | 3.87 ms | 1.8x faster |

**Key Finding:** Native V2 excels at star patterns but has chain traversal regression due to cluster lookup overhead.

### Memory Usage

| Component | Approx. Size |
|-----------|--------------|
| V2NodeStore | ~2048 nodes × 256 bytes = 512 KB (max) |
| Edge cluster | 64 KB |
| WAL record | 40-100 bytes |
| Publisher | ~200 bytes base + ~100 bytes per subscriber |

---

## Extending SQLiteGraph

### Adding a New Graph Algorithm

1. Create file in `src/algo/`
2. Accept `&dyn GraphBackend` trait object
3. Return `HashMap<u64, T>` for node-scoped results
4. Add tests in `tests/algo_tests.rs`
5. Add CLI command in `sqlitegraph-cli/src/main.rs`

### Adding a New Distance Metric

1. Implement `DistanceMetric` trait in `src/hnsw/distance.rs`
2. Add variant to `DistanceMetric` enum
3. Implement SIMD-optimized computation
4. Add tests in `tests/hnsw_tests.rs`

---

## Further Reading

- [TESTING.md](TESTING.md) - Testing patterns and utilities
- [DEBUGGING.md](DEBUGGING.md) - Debugging and profiling guide
- [README.md](../README.md) - User-facing documentation
- [MANUAL.md](../MANUAL.md) - Operator manual
- [API.md](../API.md) - Quick API reference

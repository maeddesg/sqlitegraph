# SQLiteGraph Architecture

**Last Updated:** 2026-02-12
**Version:** v1.6.0

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

SQLiteGraph is an embedded graph database with **three** storage backends:

```
┌─────────────────────────────────────────────────────────────────────┐
│                        User Application                             │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    GraphBackend Trait (Unified API)                 │
│     - insert_node, insert_edge, neighbors, subscribe, etc.          │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
           ┌────────────────────┼────────────────────┐
           │                    │                    │
           ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  SQLite Backend  │  │   Native V3      │  │  Native V2       │
│  (Stable)        │  │   (Production)   │  │  (Deprecated)    │
├──────────────────┤  ├──────────────────┤  ├──────────────────┤
│ - SQL storage    │  │ - Binary format  │  │ - Binary format  │
│ - ACID via SQL   │  │ - B+Tree index   │  │ - Clustered      │
│ - Debuggable     │  │ - Unlimited      │  │ - 2048 limit     │
│                  │  │   capacity       │  │                  │
└──────────────────┘  └──────────────────┘  └──────────────────┘
```

### Backend Status

| Backend | Status | Use Case |
|---------|--------|----------|
| **SQLite** | ✅ Stable | Debuggable, familiar SQL ecosystem |
| **Native V3** | ✅ Production | High performance, unlimited scale |
| **Native V2** | ⚠️ Deprecated | Being removed in v1.7.0 |

### Key Architectural Principles

1. **Unified Backend Trait**: Same API works with all backends
2. **Lazy Initialization**: V3 backend doesn't allocate until features are used
3. **Honest Engineering**: Document limitations, deprecate when better alternatives exist
4. **MVCC Isolation**: Snapshot-based reads without blocking writers
5. **Pluggable Components**: Algorithms, storage, and indexing are decoupled

---

## Directory Structure

```
sqlitegraph/
├── Cargo.toml                 # Main library manifest
├── src/
│   ├── lib.rs                 # Public API exports
│   ├── error.rs               # Error types
│   ├── backend/               # Storage abstraction
│   │   ├── mod.rs             # GraphBackend trait + generic types
│   │   ├── sqlite/            # SQLite backend (stable)
│   │   │   ├── impl_.rs       # SqliteGraphBackend
│   │   │   └── pubsub_tests.rs
│   │   └── native/            # Native backends
│   │       ├── v3/            # V3 backend (production)
│   │       │   ├── backend.rs # V3Backend implementation
│   │       │   ├── btree/     # B+Tree index
│   │       │   ├── kv_store/  # Lazy KV storage
│   │       │   ├── pubsub/    # Publisher implementation
│   │       │   └── wal/       # Write-ahead logging
│   │       └── v2/            # V2 backend (deprecated)
│   ├── algo/                  # Graph algorithms (backend-agnostic)
│   │   ├── backend/           # Generic algorithms for &dyn GraphBackend
│   │   └── ...                # 35+ algorithms
│   ├── hnsw/                  # Vector similarity search
│   │   ├── storage.rs         # VectorStorage trait + SQLite implementation
│   │   ├── v3_storage.rs      # V3 vector storage
│   │   └── ...
│   └── ...
└── docs/
```

---

## Core Components

### 1. GraphBackend Trait

The `GraphBackend` trait is the core abstraction. All backends implement this trait.

**Location:** `src/backend/mod.rs`

```rust
pub trait GraphBackend: Send + Sync {
    // Core graph operations
    fn insert_node(&self, node: NodeSpec) -> Result<i64, SqliteGraphError>;
    fn insert_edge(&self, edge: EdgeSpec) -> Result<i64, SqliteGraphError>;
    fn neighbors(&self, snapshot_id: SnapshotId, node: i64, query: NeighborQuery) 
        -> Result<Vec<i64>, SqliteGraphError>;
    
    // Key-Value operations (optional - backends return error if unsupported)
    fn kv_get(&self, snapshot_id: SnapshotId, key: &[u8]) -> Result<Option<KvValue>, SqliteGraphError>;
    fn kv_set(&self, key: Vec<u8>, value: KvValue, ttl_seconds: Option<u64>) -> Result<(), SqliteGraphError>;
    
    // Pub/Sub (works on all backends now)
    fn subscribe(&self, filter: SubscriptionFilter) 
        -> Result<(u64, Receiver<PubSubEvent>), SqliteGraphError>;
    fn unsubscribe(&self, subscriber_id: u64) -> Result<bool, SqliteGraphError>;
}
```

**Generic Pub/Sub Types:** The trait includes generic `PubSubEvent` and `SubscriptionFilter` types that work across all backends.

### 2. Backend Algorithms

**Location:** `src/algo/backend/`

Generic algorithm implementations that work with any `GraphBackend`:

```rust
// centrality.rs
pub fn pagerank(graph: &dyn GraphBackend, damping: f64, iterations: usize) 
    -> Result<Vec<(i64, f64)>, SqliteGraphError>;

pub fn betweenness_centrality(graph: &dyn GraphBackend) 
    -> Result<Vec<(i64, f64)>, SqliteGraphError>;

// graph_ops.rs  
pub fn strongly_connected_components(graph: &dyn GraphBackend) 
    -> Result<SccResult, SqliteGraphError>;

pub fn shortest_path(graph: &dyn GraphBackend, start: i64, end: i64) 
    -> Result<Option<Vec<i64>>, SqliteGraphError>;
```

All 35+ algorithms are backend-agnostic.

### 3. Vector Storage Abstraction

**Location:** `src/hnsw/storage.rs`

```rust
pub trait VectorStorage {
    fn store_vector(&mut self, vector: &[f32], metadata: Option<Value>) -> Result<u64, HnswError>;
    fn get_vector(&self, id: u64) -> Result<Option<Vec<f32>>, HnswError>;
    fn delete_vector(&mut self, id: u64) -> Result<(), HnswError>;
    fn vector_count(&self) -> Result<usize, HnswError>;
}
```

**Implementations:**
- `SQLiteVectorStorage` - SQL table storage
- `V3VectorStorage` - KV store storage (new)
- `InMemoryVectorStorage` - RAM only

---

## Backend Architecture

### SQLite Backend

**Location:** `src/backend/sqlite/`

**Status:** Stable, mature, debuggable

| Aspect | Implementation |
|--------|----------------|
| **Storage** | SQLite database file |
| **Schema** | `entities`, `edges`, `kv_store`, `hnsw_vectors` tables |
| **Transactions** | Full ACID via SQLite |
| **Concurrency** | Database-level locks |
| **Pub/Sub** | In-memory publisher with event emission |

**Key Features:**
- SQL-accessible for debugging
- Mature, well-tested
- Creates tables on-demand
- No node limits

### Native V3 Backend (Production)

**Location:** `src/backend/native/v3/`

**Status:** Mature, recommended for new projects

| Aspect | Implementation |
|--------|----------------|
| **Storage** | Binary file (.graph) |
| **Index** | B+Tree for O(log n) node lookups |
| **Max Nodes** | Unlimited |
| **KV Store** | Lazy-initialized in-memory HashMap |
| **Pub/Sub** | Lazy-initialized Publisher |
| **WAL** | Optional for durability |

#### File Format

```
db.graph
├── Header (1KB)
│   ├── Magic: "SQLGV3"
│   ├── Version: 3
│   ├── Root index page
│   └── Node count
├── B+Tree Index
│   ├── Internal nodes (keys → page_ids)
│   └── Leaf nodes (node_id → page_id)
├── Node Pages
│   └── Variable-size node records
├── Edge Clusters
│   └── Edge adjacency data
└── Free space bitmap
```

#### Lazy Initialization

V3 uses `Option<T>` for optional features:

```rust
pub struct V3Backend {
    // Core (always present)
    btree: RwLock<BTreeManager>,
    node_store: RwLock<NodeStore>,
    edge_store: RwLock<V3EdgeStore>,
    
    // Optional (lazy initialized)
    kv_store: RwLock<Option<KvStore>>,      // Created on first kv_get/set
    publisher: RwLock<Option<Publisher>>,   // Created on first subscribe
}
```

**Benefits:**
- Zero overhead if KV/PubSub not used
- Memory efficient for simple graph workloads
- Full features available when needed

#### Pub/Sub Implementation

```rust
impl GraphBackend for V3Backend {
    fn subscribe(&self, filter: SubscriptionFilter) -> Result<...> {
        // Lazy initialize publisher
        if self.publisher.read().is_none() {
            *self.publisher.write() = Some(Publisher::new());
        }
        // ... subscribe logic
    }
}
```

### Native V2 Backend (Deprecated)

**Location:** `src/backend/native/v2/`

**Status:** Deprecated, will be removed in v1.7.0

| Aspect | Implementation |
|--------|----------------|
| **Max Nodes** | ~2048 (8MB region limit) |
| **Status** | Do not use for new projects |

**Migration Path:**
- V2 → SQLite: Export/import via JSON
- V2 → V3: Direct migration tools (planned)

---

## Key Design Decisions

### 1. Why Lazy Initialization in V3?

**Problem:** V2 always allocated KV store and Publisher even when unused.

**Solution:** V3 uses `Option<T>` with lazy initialization.

**Trade-offs:**
- ✅ Zero memory overhead for unused features
- ✅ Slightly more complex code (Option handling)
- ✅ First access has initialization cost

### 2. Why Generic Pub/Sub Types?

**Problem:** Pub/Sub types were tied to `native-v2` feature, breaking compilation without it.

**Solution:** Moved generic `PubSubEvent` and `SubscriptionFilter` to `backend/mod.rs`.

**Result:** All backends can implement Pub/Sub with same types.

### 3. Why Backend-Agnostic Algorithms?

**Problem:** Old algorithms used `&SqliteGraph`, requiring SQLite backend.

**Solution:** New algorithms use `&dyn GraphBackend`.

**Result:** Same algorithms work with SQLite, V2, and V3 backends.

### 4. Why Deprecate V2?

**Reasons:**
- Hard 2048 node limit (architectural constraint)
- V3 has same features with unlimited capacity
- Maintenance burden of two native backends
- V3 has cleaner architecture (B+Tree vs clustered)

**Timeline:**
- v1.6.0: V2 deprecated but still available
- v1.7.0: V2 removed, V3 becomes primary native backend

---

## Module Reference

### Backend Modules

| Module | Path | Purpose |
|--------|------|---------|
| `GraphBackend` | `src/backend/mod.rs` | Core trait definition |
| `SqliteGraphBackend` | `src/backend/sqlite/impl_.rs` | SQLite implementation |
| `V3Backend` | `src/backend/native/v3/backend.rs` | V3 implementation |
| `PubSubEvent` | `src/backend/mod.rs` | Generic event types |

### Algorithm Modules

| Module | Path | Algorithms |
|--------|------|------------|
| `centrality` | `src/algo/backend/centrality.rs` | PageRank, Betweenness |
| `graph_ops` | `src/algo/backend/graph_ops.rs` | SCC, Shortest Path, Topological Sort |
| `traversal` | `src/algo/backend/traversal.rs` | BFS, DFS, k-hop |

### HNSW Modules

| Module | Path | Purpose |
|--------|------|---------|
| `SQLiteVectorStorage` | `src/hnsw/storage.rs` | SQL-backed vector storage |
| `V3VectorStorage` | `src/hnsw/v3_storage.rs` | KV-backed vector storage |

---

## Testing Strategy

Each backend has comprehensive tests:

```bash
# SQLite backend
cargo test --lib backend::sqlite

# V3 backend  
cargo test --features native-v3 --lib backend::native::v3

# Backend-agnostic algorithms
cargo test --features native-v3 --lib algo::backend

# HNSW storage
cargo test --features native-v3 --lib hnsw::v3_storage
```

**Test Philosophy:**
- TDD for all new features
- No stubs or mocks - test real implementations
- Integration tests for backend switching

---

## Future Directions

1. **V3 becomes primary native backend** (v1.7.0)
2. **V2 removal** - Simplifies codebase
3. **Migration tools** - V2 → V3 direct conversion
4. **Performance benchmarks** - Comprehensive V3 vs SQLite comparison
5. **Distributed features** - Optional clustering layer

---

**Note:** This architecture document is honest about limitations and deprecation. We don't hide technical debt or pretend all backends are equal. Choose based on your needs.

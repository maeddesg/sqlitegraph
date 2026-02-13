# HNSW Vector Store Internals

**Last Updated:** 2026-02-12  
**Version:** v1.6.0

This guide explains SQLiteGraph's HNSW (Hierarchical Navigable Small World) vector search implementation, available on **all backends** (SQLite, V3, V2).

---

## Overview

HNSW is a graph-based algorithm for approximate nearest neighbor (ANN) search. It builds a multi-layer graph structure where lower layers are dense (many connections) and higher layers are sparse (few connections), enabling O(log N) search complexity.

### Backend Support

| Backend | Storage | Implementation |
|---------|---------|----------------|
| **SQLite** | SQL table (`hnsw_vectors`) | `SQLiteVectorStorage` |
| **Native V3** | KV store (`hnsw:{index}:vector:{id}`) | `V3VectorStorage` |
| **Native V2** | In-memory only | `InMemoryVectorStorage` |

### Key Characteristics

| Characteristic | Value |
|----------------|-------|
| **Search Time** | O(log N) average case |
| **Memory Usage** | 2-3x vector data size |
| **Build Time** | O(N log N) |
| **Accuracy** | 95%+ recall for typical workloads |
| **Algorithm** | Hierarchical Navigable Small World |

---

## Module Structure

```
src/hnsw/
├── index.rs           # Main HnswIndex API and orchestration
├── index_api.rs       # Public API methods (insert, search, query)
├── index_internal.rs  # Internal search/insert algorithms
├── index_persist.rs   # Persistence and recovery
├── layer.rs           # Layer data structure and management
├── neighborhood.rs    # k-NN search algorithms
├── multilayer.rs      # Multi-layer mode with level distributor
├── distance_metric.rs # Distance metric definitions
├── distance_functions.rs # SIMD-optimized distance computation
├── storage.rs         # Vector storage abstraction + SQLite implementation
├── v3_storage.rs      # V3 backend vector storage (NEW in v1.6.0)
├── config.rs          # Configuration builder
├── errors.rs          # Error types
├── simd.rs            # SIMD intrinsics for AVX2
├── serialization.rs   # (De)serialization for persistence
└── batch_filter.rs    # Batch filtering operations
```

---

## Data Structures

### HnswIndex (Main Orchestrator)

Located in `index.rs`:

```rust
pub struct HnswIndex {
    /// Name of this index (for multi-index support)
    pub(crate) name: String,

    /// HNSW configuration parameters
    pub(crate) config: HnswConfig,

    /// Layer management (0 = base layer, higher = smaller layers)
    pub(crate) layers: Vec<HnswLayer>,

    /// Vector storage backend (SQLite, V3, or in-memory)
    pub(crate) storage: Box<dyn VectorStorage>,

    /// Entry points for navigating the hierarchical structure
    pub(crate) entry_points: Vec<u64>,

    /// Number of vectors currently indexed
    pub(crate) vector_count: usize,

    /// Neighborhood search engine
    pub(crate) search_engine: NeighborhoodSearch,

    /// Level distributor for exponential level assignment (multi-layer mode)
    pub(crate) level_distributor: Option<LevelDistributor>,

    /// Multi-layer node manager for tracking layer assignments
    pub(crate) multi_layer_manager: Option<MultiLayerNodeManager>,
}
```

### VectorStorage Trait

The storage abstraction that enables multiple backends:

```rust
pub trait VectorStorage {
    fn store_vector(&mut self, vector: &[f32], metadata: Option<Value>) 
        -> Result<u64, HnswError>;
    fn get_vector(&self, id: u64) -> Result<Option<Vec<f32>>, HnswError>;
    fn delete_vector(&mut self, id: u64) -> Result<(), HnswError>;
    fn vector_count(&self) -> Result<usize, HnswError>;
}
```

**Implementations:**
- `SQLiteVectorStorage` - SQL table storage
- `V3VectorStorage` - KV store storage (NEW in v1.6.0)
- `InMemoryVectorStorage` - RAM only

---

## Storage Backends

### SQLiteVectorStorage

Located in `storage.rs`:

```rust
pub struct SQLiteVectorStorage {
    index_id: i64,
    conn: Connection,
}

// Stores vectors in table:
// CREATE TABLE hnsw_vectors (
//     id INTEGER PRIMARY KEY,
//     index_id INTEGER,
//     vector_data BLOB,
//     metadata JSON,
//     created_at INTEGER,
//     updated_at INTEGER
// );
```

**Usage:**
```rust
let conn = /* SQLite connection */;
let storage = SQLiteVectorStorage::new(index_id, conn);
let index = HnswIndex::new_with_storage("my_index", config, Box::new(storage))?;
```

### V3VectorStorage (NEW in v1.6.0)

Located in `v3_storage.rs`:

```rust
/// V3 backend vector storage using KV store
pub struct V3VectorStorageHandle {
    backend_ptr: *const V3Backend,  // SAFETY: Valid as long as backend lives
    index_name: String,
    next_id: AtomicU64,
    count: AtomicUsize,
}

// Stores vectors in KV store with keys:
// hnsw:{index_name}:vector:{vector_id}
// Value: JSON-serialized StoredVectorRecord
```

**Usage:**
```rust
let backend = V3Backend::create("data.graph")?;
let storage = backend.create_hnsw_storage("embeddings").unwrap();
let index = HnswIndex::new_with_storage("embeddings", config, storage)?;
```

**Note:** V3VectorStorage uses an unsafe pointer to the backend. The storage must not outlive the backend.

### InMemoryVectorStorage

For temporary/non-persistent indexes:

```rust
let storage = InMemoryVectorStorage::new();
let index = HnswIndex::new_with_storage("temp", config, Box::new(storage))?;
// Vectors lost when index dropped
```

---

## Configuration

### HnswConfig

```rust
pub struct HnswConfig {
    /// Vector dimension (required)
    pub dimension: usize,

    /// Max connections per node (default: 16)
    pub m: usize,

    /// Candidate list size during construction (default: 200)
    pub ef_construction: usize,

    /// Candidate list size during search (default: 50)
    pub ef_search: usize,

    /// Max layer count (default: 16)
    pub ml: usize,

    /// Distance metric (default: Cosine)
    pub distance_metric: DistanceMetric,
    
    /// Enable multi-layer mode (default: false)
    pub enable_multilayer: bool,
}
```

### Builder Pattern

```rust
let config = HnswConfig::builder()
    .dimension(768)
    .m_connections(16)
    .ef_construction(200)
    .ef_search(50)
    .distance_metric(DistanceMetric::Cosine)
    .build()?;
```

---

## Algorithm Overview

### Search

1. Start at top layer entry point
2. Greedy descent: find closest neighbor to query
3. Move down to next layer, repeat
4. At base layer (layer 0), expand to ef_search candidates
5. Return k nearest from candidates

```
Query: q
        │
        ▼
    Layer 3: EP ──► closest ──► descend
                  │
                  ▼
    Layer 2: entry ──► closest ──► descend
                     │
                     ▼
    Layer 1: entry ──► closest ──► descend
                     │
                     ▼
    Layer 0: entry ──► ef_search candidates ──► return k best
```

### Insert

1. Assign random level (exponential distribution)
2. Search for nearest neighbors at each layer (up to assigned level)
3. Connect to m closest neighbors at each layer
4. Store vector in storage backend

---

## Distance Metrics

| Metric | Use Case | Range |
|--------|----------|-------|
| **Cosine** | Text embeddings, normalized vectors | [0, 2] |
| **Euclidean** | General similarity | [0, ∞) |
| **DotProduct** | Pre-normalized embeddings | [-1, 1] |
| **Manhattan** | Sparse vectors | [0, ∞) |

All metrics use SIMD optimizations (AVX2 when available).

---

## Multi-Layer Mode (Experimental)

When `enable_multilayer = true`:

- Uses `LevelDistributor` for deterministic level assignment
- `MultiLayerNodeManager` tracks layer assignments
- Allows explicit control over layer structure

Default mode assigns levels randomly (Malkov & Yashunin algorithm).

---

## Persistence

### SQLite Backend

Vectors stored in SQL table survive process restarts. Graph structure (layers, edges) is rebuilt from vectors on load.

### V3 Backend

Vectors stored in KV store. WAL persistence optional.

### In-Memory

No persistence - vectors lost when index dropped.

---

## Testing

```bash
# V3 vector storage tests
cargo test --features native-v3 --lib hnsw::v3_storage_tests

# 9 tests covering:
# - store_and_get
# - store_with_explicit_id
# - vector_count
# - list_vectors (documented limitation)
# - delete_vector
# - batch_store
# - get_nonexistent
# - with_graph_operations

# General HNSW tests
cargo test --lib hnsw::tests
```

---

## Limitations

1. **list_vectors() in V3** - Requires KV prefix scan (not fully implemented)
2. **V3VectorStorage lifetime** - Must not outlive V3Backend
3. **No edge persistence** - Layer structure rebuilt from vectors on load
4. **Build time** - O(N log N) can be slow for large datasets

---

## See Also

- [Adding a Distance Metric](adding-a-distance-metric.md)
- [Architecture](../../ARCHITECTURE.md)
- [API Reference](../../API.md) - HNSW API

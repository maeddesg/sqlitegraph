# HNSW Vector Store Internals

**Last Updated:** 2026-02-03
**Version:** v1.4.2

This guide explains the internal architecture of SQLiteGraph's HNSW (Hierarchical Navigable Small World) vector search implementation.

---

## Overview

HNSW is a graph-based algorithm for approximate nearest neighbor (ANN) search. It builds a multi-layer graph structure where lower layers are dense (many connections) and higher layers are sparse (few connections), enabling O(log N) search complexity.

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
├── index.rs           # Main HNSW index API and orchestration
├── index_api.rs       # Public API methods (insert, search, query)
├── index_internal.rs  # Internal search/insert algorithms
├── index_persist.rs   # Persistence and recovery
├── layer.rs           # Layer data structure and management
├── neighborhood.rs    # k-NN search algorithms
├── multilayer.rs      # Multi-layer mode with level distributor
├── distance_metric.rs # Distance metric definitions
├── distance_functions.rs # SIMD-optimized distance computation
├── storage.rs         # Vector storage abstraction
├── config.rs          # Configuration builder
├── errors.rs          # Error types
├── simd.rs            # SIMD intrinsics for AVX2
├── serialization.rs  # (De)serialization for persistence
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

    /// Vector storage backend (in-memory or SQLite)
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

### HnswLayer (Single Layer)

Located in `layer.rs`:

```rust
pub struct HnswLayer {
    /// Layer level (0 = base layer)
    level: u8,

    /// Maximum connections per node in this layer
    max_connections: usize,

    /// Nodes in this layer: node_id -> connections
    nodes: Vec<HashSet<u64>>,

    /// Entry points for efficient navigation
    entry_points: Vec<u64>,

    /// Total number of vectors indexed in the layer
    vector_count: usize,
}
```

### Layer Connectivity Pattern

```
Layer 0 (Base):    M connections per node (dense)
Layer 1:           M/2 connections
Layer 2:           M/4 connections
Layer N:           1 connection (sparse)
```

Where `M` is configured via `HnswConfig::m` (default: 16).

---

## HNSW Algorithm

### Search Algorithm (Greedy Search)

The search process starts at the top layer and descends:

```
1. Start at entry point in highest layer
2. Perform greedy search within current layer:
   - Maintain candidate list (nearest neighbors found so far)
   - For each candidate, explore its connections
   - Add unvisited neighbors to candidate list
   - Stop when no closer candidates exist
3. Move to next lower layer, starting from best candidate
4. Repeat until layer 0 (base layer)
5. Return k nearest neighbors from layer 0
```

### Code: Search Entry Point

Located in `neighborhood.rs`:

```rust
impl NeighborhoodSearch {
    /// Search for k-nearest neighbors using layer-by-layer traversal
    pub fn search(
        &self,
        query: &[f32],
        k: usize,
        ef: usize,  // candidate list size
        layers: &[HnswLayer],
        entry_points: &[u64],
        storage: &dyn VectorStorage,
    ) -> SearchResult {
        let mut candidates = BinaryHeap::new();
        let mut visited = HashSet::new();

        // Start from top layer's entry point
        let mut current_ep = entry_points.last().copied();

        // Descend through layers
        for layer in layers.iter().rev() {
            if let Some(ep) = current_ep {
                // Greedy search in this layer
                let result = self.greedy_search(
                    query, layer, ep, ef, &mut visited, storage
                );

                // Use best candidate as entry point for next layer
                current_ep = result.best_neighbor;
            }
        }

        // Final k-NN selection from layer 0
        self.select_k_nearest(candidates, k)
    }
}
```

### Insertion Algorithm

When inserting a new vector:

```
1. Assign vector ID from storage (1-based)
2. Determine max layer for this vector (exponential distribution)
3. For each layer from 0 to max_layer:
   a. Find nearest neighbors in that layer
   b. Select top efConstruction candidates
   c. Bidirectionally connect to selected candidates
   d. Prune connections if exceeding max_connections
4. Update entry points if vector assigned to new highest layer
```

### Code: Insert Entry Point

Located in `index_internal.rs`:

```rust
impl HnswIndex {
    /// Insert a vector into the HNSW index
    pub fn insert_vector(
        &mut self,
        vector: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<u64, HnswError> {
        // Validate dimension
        if vector.len() != self.config.dimension {
            return Err(HnswError::DimensionMismatch {
                expected: self.config.dimension,
                actual: vector.len(),
            });
        }

        // Store vector and get assigned ID (1-based)
        let vector_id = self.storage.insert(vector, metadata)?;

        // Determine max layer using exponential distribution
        let max_layer = self.assign_layer_level();

        // Insert into each layer
        for layer_level in 0..=max_layer {
            self.insert_at_layer(vector_id, vector, layer_level, max_layer)?;
        }

        // Update entry points
        self.update_entry_points(vector_id, max_layer);

        self.vector_count += 1;
        Ok(vector_id)
    }
}
```

---

## Multi-Layer Mode

### Global vs Local IDs

In multi-layer mode, there's a distinction between:

- **Global IDs**: 1-based IDs from vector storage (persistent)
- **Local IDs**: 0-based IDs within each layer (ephemeral)

### LayerMappings

Located in `multilayer.rs`:

```rust
pub struct LayerMappings {
    /// Global ID (1-based) → Vec<Option<LocalID>> per layer
    global_to_local: HashMap<u64, Vec<Option<u64>>>,

    /// Local ID → Global ID per layer
    local_to_global: Vec<HashMap<u64, u64>>,

    /// Next available local ID for each layer
    next_local_id: Vec<usize>,
}
```

### Level Assignment Distribution

Vectors are assigned to layers using exponential distribution:

```
P(level = ℓ) = m^(-ℓ)
```

Where:
- `m` is the base connectivity parameter
- `ℓ` is the layer level

### Code: LevelDistributor

```rust
pub struct LevelDistributor {
    /// Base M parameter for distribution
    base_m: f64,

    /// Maximum number of layers
    max_layers: usize,

    /// Seeded RNG for deterministic results
    rng: StdRng,
}

impl LevelDistributor {
    /// Assign a layer level using exponential distribution
    pub fn assign_level(&mut self) -> usize {
        let mut level = 0;
        while level < self.max_layers - 1 {
            // Probability check: continue with probability 1/m
            if self.rng.gen::<f64>() < (1.0 / self.base_m) {
                level += 1;
            } else {
                break;
            }
        }
        level
    }
}
```

---

## Distance Metrics

### Supported Metrics

Located in `distance_metric.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DistanceMetric {
    /// Cosine similarity (1 - cosine)
    Cosine,

    /// Euclidean (L2) distance
    Euclidean,

    /// Dot product (1 - dot_product)
    DotProduct,

    /// Manhattan (L1) distance
    Manhattan,
}
```

### SIMD Optimization

Located in `simd.rs`:

```rust
/// SIMD-accelerated cosine distance for AVX2
#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn cosine_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    // Process 8 floats (256 bits) at a time
    // Uses _mm256_loadu_ps, _mm256_fmadd_ps, etc.
}
```

SIMD is used for:
- **Cosine distance**: AVX2 for dot product + magnitude computation
- **Euclidean distance**: AVX2 for squared differences
- **Dot product**: AVX2 for accumulated multiplication
- **Manhattan**: No SIMD (simple absolute difference)

---

## Vector Storage

### Storage Abstraction

```rust
pub trait VectorStorage: Send + Sync {
    /// Insert a vector and return assigned ID (1-based)
    fn insert(&mut self, vector: Vec<f32>, metadata: Option<Value>)
        -> Result<u64, HnswError>;

    /// Get vector by ID
    fn get(&self, id: u64) -> Option<&[f32]>;

    /// Get metadata for vector
    fn get_metadata(&self, id: u64) -> Option<&Value>;

    /// Get total vector count
    fn len(&self) -> usize;

    /// Check if empty
    fn is_empty(&self) -> bool;
}
```

### Implementations

| Storage | Description | Use Case |
|---------|-------------|----------|
| `InMemoryVectorStorage` | HashMap-based in-memory | Testing, ephemeral data |
| `SQLiteVectorStorage` | SQLite-backed persistence | Production data |

---

## Configuration

### HnswConfig Builder

```rust
pub struct HnswConfig {
    /// Vector dimension
    pub dimension: usize,

    /// Distance metric
    pub distance_metric: DistanceMetric,

    /// Max connections per node (base layer)
    pub m: usize,

    /// Max layer count
    pub ml: usize,

    /// efConstruction: candidate list size during build
    pub ef_construction: usize,

    /// efSearch: candidate list size during search
    pub ef_search: usize,

    /// Enable multi-layer mode
    pub enable_multilayer: bool,

    /// Seed for deterministic level distribution
    pub multilayer_deterministic_seed: Option<u64>,
}
```

### Default Values

| Parameter | Default | Description |
|-----------|---------|-------------|
| `m` | 16 | Max connections per node (base layer) |
| `ml` | 5 | Maximum number of layers |
| `ef_construction` | 200 | Candidate list size during build |
| `ef_search` | 50 | Candidate list size during search |
| `dimension` | 128 | Vector dimension (must match data) |

---

## Performance Characteristics

### Build Time

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Single insert | O(log N × M × ef) | M = max connections |
| Batch insert (N vectors) | O(N log N × M × ef) | With level assignment |

### Search Time

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| k-NN search | O(log N × ef) | ef = candidate list size |
| Layer traversal | O(L) | L = number of layers |

### Memory Usage

| Component | Memory |
|-----------|--------|
| Vector data | N × D × 4 bytes (f32) |
| Graph edges | ~2 × N × M × 8 bytes (connections) |
| Metadata | N × ~100 bytes (optional) |
| **Total** | ~2-3x vector data size |

---

## Testing

### Test Files

| File | Description |
|------|-------------|
| `hnsw/tests.rs` | Unit tests for HNSW index |
| `tests/hnsw_tests.rs` | Integration tests |
| `hnsw/simd.rs` | SIMD validation tests |

### Key Test Scenarios

```rust
#[test]
fn test_insert_and_search() {
    let mut hnsw = create_test_index();

    // Insert vectors
    let id1 = hnsw.insert_vector(&vec![1.0, 0.0], None).unwrap();
    let id2 = hnsw.insert_vector(&vec![0.0, 1.0], None).unwrap();

    // Search
    let results = hnsw.search(&vec![1.0, 0.0], 5).unwrap();

    assert_eq!(results[0].0, id1); // Exact match
}

#[test]
fn test_snapshot_isolation() {
    // Test that searches use consistent snapshot
}

#[test]
fn test_persistence() {
    // Test save/load from SQLite
}
```

---

## Common Patterns

### Creating an Index

```rust
use sqlitegraph::hnsw::{HnswIndex, HnswConfig, DistanceMetric};

let config = HnswConfig::builder()
    .dimension(768)  // OpenAI embedding size
    .distance_metric(DistanceMetric::Cosine)
    .m(16)
    .ef_construction(200)
    .ef_search(50)
    .build()?;

let hnsw = HnswIndex::new("embeddings", config)?;
```

### Inserting with Metadata

```rust
let vector_id = hnsw.insert_vector(
    &embedding_vector,
    Some(json!({
        "document_id": "doc123",
        "chunk_index": 0,
    }))
)?;
```

### Batch Insert

```rust
for (i, vector) in vectors.iter().enumerate() {
    hnsw.insert_vector(vector, Some(json!({"index": i})))?;
}
```

### Search with Results

```rust
let results = hnsw.search(&query_vector, 10)?;

for (vector_id, distance) in results {
    let metadata = hnsw.get_metadata(vector_id)?;
    println!("ID: {}, Distance: {}, Meta: {}", vector_id, distance, metadata);
}
```

---

## Troubleshooting

### Issue: Low recall accuracy

**Symptoms:** Search results missing expected nearest neighbors

**Solutions:**
1. Increase `ef_search` (candidate list size)
2. Increase `ef_construction` and rebuild index
3. Increase `m` (max connections)
4. Check dimension matches vector data

### Issue: Slow build time

**Symptoms:** Index construction takes too long

**Solutions:**
1. Reduce `ef_construction`
2. Reduce `m` (fewer connections)
3. Reduce `ml` (fewer layers)
4. Use batch insert instead of single inserts

### Issue: High memory usage

**Symptoms:** Memory usage exceeds expectations

**Solutions:**
1. Reduce `m` (fewer connections per node)
2. Reduce `ef_construction`
3. Disable metadata storage
4. Consider different ANN algorithm for very large datasets

### Issue: Dimension mismatch

**Symptoms:** `HnswError::DimensionMismatch`

**Solution:** Ensure all vectors have the same dimension as configured

---

## References

- **Source:** `src/hnsw/` - Full implementation
- **Paper:** Malkov, Yashunin, "Efficient and robust approximate nearest neighbor search using Hierarchical Navigable Small World graphs" (2018)
- **Related:** `docs/DEVELOPMENT_GUIDES/adding-a-distance-metric.md` - Adding custom metrics
- **User Docs:** `MANUAL.md` Section 8 - HNSW usage

---

## Future Enhancements

Potential areas for extension (not currently planned):

1. **Parallel construction** - Multi-threaded index building
2. **Dynamic index updates** - Efficient deletion and update
3. **Quantization** - Product quantization for compression
4. **GPU acceleration** - CUDA-based distance computation
5. **Alternative storage** - Redis-backed vector storage

Note: These are **not currently planned** - the HNSW implementation is focused on correctness and single-machine performance.

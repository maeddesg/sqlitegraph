# Graph Database Storage Architecture Comparison

**Date:** 2026-02-09
**Purpose:** Compare SQLiteGraph native-v2/v3 designs with industry graph databases

---

## Executive Summary

| Database | Storage Format | Max Scale | Lookup | Best For |
|----------|---------------|-----------|--------|----------|
| **SQLiteGraph V2** | Fixed 4KB slots | 2,048 nodes | O(1) | Small graphs |
| **SQLiteGraph V3** | B+tree + 16KB pages | 4B+ nodes | O(log n) | Any scale |
| **Neo4j** | Fixed records (9-15B) | Unlimited* | O(1) | General purpose |
| **RedisGraph** | GraphBLAS (CSR/CSC) | Memory-bound | O(1) | Analytics |
| **TigerGraph** | CSR-like + native parallel | Distributed | O(1) | Real-time analytics |

---

## External Validation: V3 Design Direction

**Consensus (2026):** B+tree + clustered edges is the correct general-purpose compromise.

From multiple AI sources and research:

> "B+tree + 16KB pages is the right move for 2025–2026:
> - page-based
> - cache-friendly
> - mutation-friendly
> - no CSR rebuilds
> - no fixed-slot ceiling
> - compatible with MVCC
> - compatible with WAL
> - compatible with mmap
>
> Yes, O(log n) instead of O(1), but:
> - log₂(4B) ≈ 32 (very small)
> - page fanout reduces this further
> - real-world latency is dominated by I/O anyway
>
> Anyone obsessing over theoretical O(1) here is doing paper math, not systems engineering."

### Why O(1) vs O(log n) Doesn't Matter

| Concern | Reality |
|---------|----------|
| **log₂(4B) = 32** | Max 32 page lookups - tiny |
| **Page fanout** | Real B+tree has 100+ children → 2-3 levels max |
| **I/O dominates** | RAM cache hits make CPU irrelevant |
| **Cache locality** | 16KB pages are more cache-friendly than scattered pointers |

### The "Paper Math" Trap

```
Theoretical:  O(1) < O(log n)
Real world:    page_cache_hit >> CPU_cycle

100 ns RAM lookup  vs  10 μs disk I/O
= 100x difference

CPU optimization:      -5 ns
O(log n) vs O(1):      +50 ns  (worst case, uncached)

Conclusion: Focus on I/O, not theoretical complexity
```

---

## 1. Neo4j Storage Architecture

### File Structure
```
data/databases/graph.db/
├── neostore.nodestore.db         # Node records
├── neostore.relationshipstore.db # Relationship records
├── neostore.propertystore.db     # Property records
├── neostore.labelstore.db        # Label/index records
└── neostore.schemastore.db       # Schema metadata
```

### Record Format (Fixed-Size)

**Earlier Versions (9 bytes):**
- Total: 9 bytes per record
- Enables O(1) random access

**Later Versions (15 bytes):**
```
┌──────────┬───────────┬───────────┬────────────┐
│ In Use   │ First Rel │ First Prop │ Next/Prev   │
│ (1 byte) │ (4 bytes) │ (4 bytes) │ (6 bytes)   │
└──────────┴───────────┴───────────┴────────────┘
```

### Key Design Decisions

| Aspect | Choice | Rationale |
|--------|--------|-----------|
| **Record Size** | Fixed (9-15 bytes) | O(1) lookup: `offset = record_size × node_id` |
| **Relationships** | Separate store | Clustered by start node |
| **Properties** | Separate store | Dynamic, variable-length |
| **IDs** | Reusable | Slot reuse on delete |

### 2024-2025 Evolution

**Block Format (2024):**
- Next-generation storage engine
- Better hardware utilization
- Improved scalability

**Infinigraph (2025):**
- Distributed architecture
- 100TB+ operational workloads
- Scalable beyond single-machine limits

### Comparison with SQLiteGraph V2

| | Neo4j | SQLiteGraph V2 |
|---|---|---|
| **Record Size** | 9-15 bytes | 4,096 bytes (full slot) |
| **Node Storage** | Metadata only | Full node data in slot |
| **Edges** | Separate relationship store | Clustered in node slot |
| **Properties** | Separate dynamic store | Serialized in node record |
| **Scale** | Unlimited | 2,048 nodes (hard limit) |
| **Lookup** | O(1) via record offset | O(1) via slot offset |

**Key Difference:** Neo4j stores only metadata in fixed slots; SQLiteGraph V2 stores entire node data (4KB). This enables V2's zero-copy reads but creates the 2048 limit.

---

## 2. RedisGraph Storage Architecture

### Core: GraphBLAS (GraphBLAS)

RedisGraph uses **Compressed Sparse Row (CSR)** / **Compressed Sparse Column (CSC)** format:

```
CSR Representation:
┌─────────────────────────────────────────────┐
│ Column Pointer: [0, 2, 5, 7, ...]          │
│ Row Indices:    [0, 2, 0, 1, 2, 1, 2, ...] │
│ Values:         [w1, w2, w3, ...]          │
└─────────────────────────────────────────────┘
```

### Matrix-Based Operations

- Graph as adjacency matrix
- Linear algebra operations (matrix multiplication)
- Excellent for: batch analytics, PageRank, centrality
- Poor for: single-edge insertions, dynamic updates

### Performance Characteristics

| Operation | CSR Performance |
|-----------|-----------------|
| **BFS/Traversal** | Fast (memory-sequential) |
| **Edge Insertion** | Slow (requires CSR rebuild) |
| **Single-hop lookup** | O(degree) scan required |
| **Batch Analytics** | Excellent (GPU-friendly) |

### Comparison with SQLiteGraph

| | RedisGraph | SQLiteGraph V2 | SQLiteGraph V3 |
|---|---|---|---|
| **Format** | CSR/CSC (matrix) | Fixed slots | B+tree + pages |
| **Best For** | Analytics | Small graphs | Any workload |
| **Edge Insertion** | Slow (rebuild) | Fast (append) | Fast (B+tree) |
| **Traversal** | Fast (scan) | Fast (clustered) | Fast (clustered) |
| **Memory** | In-memory only | Disk-backed | Disk-backed |
| **Scale** | RAM-limited | 2,048 nodes | 4B+ nodes |

---

## 3. TigerGraph Storage Architecture

### Native Parallel Graph (NPG)

TigerGraph uses **CSR-like storage** with native parallel processing:

```
Vertex-Centric Storage:
┌─────────────────────────────────────┐
│ Vertex 1: [edges_out, edges_in]    │
│ Vertex 2: [edges_out, edges_in]    │
│ Vertex 3: [edges_out, edges_in]    │
└─────────────────────────────────────┘
```

### Key Innovations

1. **Native Parallel Processing (MPP)**
   - SIMD instructions
   - Multi-core parallel traversal
   - GPU acceleration support

2. **Compressed Adjacency**
   - Delta encoding for sequential edges
   - Variable-length integer encoding
   - 2-10x compression vs raw adjacency lists

3. **Schema-Enforced**
   - Static typing enables optimization
   - Known edge types at compile time

### Performance Claims

| Metric | TigerGraph | Competitors |
|--------|------------|-------------|
| **Traversal** | 10-100x faster | Than Neo4j |
| **Loading** | 10x faster | Than competitors |
| **Storage** | Up to 10x smaller | Than adjacency lists |

### Comparison with SQLiteGraph

| | TigerGraph | SQLiteGraph V2 | SQLiteGraph V3 |
|---|---|---|---|
| **Storage** | Compressed CSR | Fixed slots | B+tree + pages |
| **Parallelism** | Native MPP | Single-threaded | Single-threaded |
| **Schema** | Static required | Schemaless | Schemaless |
| **Edges** | Compressed | Uncompressed | Uncompressed |
| **Best For** | Real-time analytics | Small OLTP | General OLTP |
| **Language** | GSQL (SQL-like) | Rust API | Rust API |

---

## 4. Academic Research: CSR vs Adjacency List

### LiveGraph (Tsinghua University, 2020)

**Key Finding:** Pure CSR is inefficient for dynamic workloads.

| Format | Pros | Cons |
|--------|------|------|
| **Adjacency List** | Fast updates | High memory, poor cache |
| **CSR** | Memory-efficient, cache-friendly | Slow updates (O(E) rebuild) |

**LiveGraph Solution:** Hybrid with versioning for fast reads + writes.

### LSMGraph (ArXiv, Nov 2024)

**Key Finding:** LSM-tree + CSR hybrid provides superior performance.

- Writes buffer in memtable (like LSM)
- Reads query CSR for cache efficiency
- Achieves both fast updates AND fast traversal

### BACH: Bridging Adjacency List and CSR (VLDB 2024)

**Key Finding:** Adaptive format selection based on workload.

- Hot vertices: CSR format (cache-friendly)
- Cold vertices: Adjacency list (memory-efficient)
- Dynamic migration between formats

### PCSR: Packed Compressed Sparse Row (102 citations)

**Key Innovation:** In-place mutations without full rebuild.

- Packed arrays for CSR
- Local rebalancing on insert/delete
- O(√n) amortized update cost

---

## 5. Comparison with SQLiteGraph Designs

### SQLiteGraph V2: Current Design

```
┌─────────────────────────────────────────────┐
│ Fixed 4KB Slots (2048 max)                  │
│                                             │
│ Slot 1: [Node Header | Properties | Edges]  │
│ Slot 2: [Node Header | Properties | Edges]  │
│ Slot 3: [Node Header | Properties | Edges]  │
│ ...                                         │
│ Slot 2048: [Node Header | Properties | Edges]│
└─────────────────────────────────────────────┘

offset = node_data_offset + ((node_id - 1) × 4096)
```

**Pros:**
- O(1) lookup via direct addressing
- Zero-copy reads (data in slot)
- Clustered edges for fast traversal
- Simple implementation

**Cons:**
- Hard 2048 node limit
- Wasted space for small nodes
- No dynamic allocation

**Similar to:**
- Neo4j's fixed-record approach
- But with much larger records (4KB vs 9-15B)

### SQLiteGraph V3: Planned Design

```
┌─────────────────────────────────────────────┐
│ B+tree Index (NodeId → PageId)              │
│                                             │
│ Page 1: [~64 compressed nodes, 16KB]        │
│ Page 2: [~64 compressed nodes, 16KB]        │
│ Page 3: [~64 compressed nodes, 16KB]        │
│ ...                                         │
│ Page N: [~64 compressed nodes, 16KB]        │
└─────────────────────────────────────────────┘
         │                    │
         ▼                    ▼
    O(log n) lookup    Unlimited scale
```

**Pros:**
- Unlimited scale (4B+ nodes)
- Page-level compression (~64 nodes/page vs 1/slot)
- LRU cache mitigates O(log n) cost
- Transactional page allocation

**Cons:**
- O(log n) lookup vs O(1) in V2
- More complex implementation
- Cache misses on cold data

**Similar to:**
- LSMGraph's page-based approach
- Traditional database B+tree indexing

---

## 6. Key Design Trade-offs

### Trade-off: Fixed vs Variable Records

| | Fixed Records (V2, Neo4j) | Variable/Paged (V3, LSMGraph) |
|---|---|---|
| **Lookup** | O(1) direct addressing | O(log n) tree traversal |
| **Space** | Wasted for small records | Efficient packing |
| **Updates** | In-place overwrite | Page management |
| **Scale** | Limited by reserved space | Unlimited |
| **Complexity** | Simple | Complex (B+tree, cache) |

### Trade-off: Edge Storage

| | In-Node (V2) | Separate Store (Neo4j) | CSR (RedisGraph) |
|---|---|---|---|
| **Traversal** | Fast (clustered) | Fast (indexed) | Fast (scan) |
| **Updates** | Fast (local) | Medium (2 stores) | Slow (rebuild) |
| **Compression** | None | Possible | High |
| **Best For** | OLTP | General | Analytics |

### Trade-off: Memory vs Disk

| | In-Memory (RedisGraph) | Disk-Backed (SQLiteGraph, Neo4j) |
|---|---|---|
| **Scale** | RAM-limited | Storage-limited |
| **Latency** | Nanosecond | Microsecond |
| **Durability** | Optional | WAL/ACID |
| **Cost** | High (RAM) | Low (SSD) |

---

## 7. Recommendations for SQLiteGraph V3

### Adopt from Industry

1. **From LSMGraph (2024):**
   - Hybrid LSM + CSR for write path
   - Memtable buffering for batch edge inserts
   - Flush to sorted pages on commit

2. **From PCSR:**
   - Packed page format for compression
   - In-place mutations where possible
   - Local page rebalancing

3. **From Neo4j Block Format:**
   - Page checksums for corruption detection
   - Version-aware page reads (MVCC)

4. **From TigerGraph:**
   - Edge compression (delta encoding)
   - SIMD-friendly traversal where possible

### Keep from SQLiteGraph V2

1. **Clustered edge storage** - proven fast for traversal
2. **Zero-copy snapshot reads** - unique advantage
3. **WAL-based transactions** - working well
4. **HNSW vector search** - backend-agnostic, already works

### Avoid from Industry

1. **Pure CSR** - too slow for dynamic workloads
2. **Pure adjacency lists** - too memory-heavy
3. **GraphBLAS-only** - analytics-only, not general purpose

---

## 8. V3 Design Validation

### Research Supports B+tree + Pages

| Paper | Finding | Alignment with V3 |
|-------|---------|-------------------|
| **LSMGraph (2024)** | LSM + CSR hybrid superior | B+tree similar LSM benefits |
| **LiveGraph (2020)** | Pure CSR inefficient for updates | V3 avoids pure CSR |
| **BACH (2024)** | Adaptive format selection | V3 uses consistent format (simpler) |
| **PCSR** | Packed arrays work | V3's 64 nodes/page matches |

### V3 vs Industry Standards

| Metric | V3 Design | Neo4j | RedisGraph | TigerGraph |
|--------|-----------|-------|-----------|-----------|
| **Max Nodes** | 4B+ | Unlimited | RAM-limited | Distributed |
| **Node Lookup** | O(log n) + cache | O(1) | O(1) matrix | O(1) CSR |
| **Edge Insertion** | Fast (page append) | Fast | Slow (rebuild) | Fast (local) |
| **Traversal** | Fast (clustered) | Fast | Fast (scan) | Fast (parallel) |
| **Storage** | Disk, compressed | Disk | Memory | Disk, compressed |
| **ACID** | Full WAL | Full WAL | Redis persistence | Configurable |

### Competitive Position

V3 will be competitive with:
- **Neo4j:** Similar scale, simpler architecture
- **RedisGraph:** Better durability, larger scale
- **TigerGraph:** Less parallel, but simpler deployment

V3 advantages:
- **Embeddable** (no server required)
- **Rust-based** (memory safety)
- **HNSW integrated** (vector search)

---

## Sources

### Neo4j
- [Neo4j Store Formats Documentation](https://neo4j.com/docs/operations-manual/current/database-internals/store-formats/)
- [Understanding Neo4j's Data on Disk](https://neo4j.com/developer/kb/understanding-data-on-disk/)
- [Neo4j Block Format (NODES 2024)](https://neo4j.com/videos/nodes-2024-block-format-the-next-generation-graph-native-storage-engine/)
- [Neo4j Infinigraph Announcement](https://neo4j.com/blog/graph-database/infinigraph-scalable-architecture/)
- [Neo4j Internals Blog](https://gauravsarma1992.medium.com/neo4j-storage-internals-be8d150028db)

### RedisGraph
- [RedisGraph Design Documentation](https://github.com/RedisGraph/RedisGraph/blob/master/docs/docs/design/_index.md)
- [RedisGraph GraphBLAS Paper](https://arxiv.org/pdf/1905.01294)
- [RedisGraph Performance Blog](https://redis.io/blog/new-redisgraph-1-0-achieves-600x-faster-performance-graph-databases/)

### TigerGraph
- [TigerGraph Internal Architecture](https://docs.tigergraph.com/tigergraph-server/4.2/intro/internal-architecture)
- [TigerGraph: A Native MPP Graph Database](https://arxiv.org/pdf/1901.08248)
- [Native Parallel Graphs Whitepaper](https://www.tigergraph.com/wp-content/uploads/2018/09/Native-Parallel-Graphs-The-Next-Generation-of-Graph-Database-for-Real-Time-Deep-Link-Analytics.pdf)

### Academic Research
- [LiveGraph: Transactional Graph Storage (Tsinghua, 2020)](https://pacman.cs.tsinghua.edu.cn/~cwg/publication/livegraph-2020/livegraph-2020.pdf)
- [LSMGraph: High-Performance Dynamic Graph Storage (ArXiv, Nov 2024)](https://arxiv.org/html/2411.06392v2)
- [BACH: Bridging Adjacency List and CSR (VLDB 2024)](https://www.vldb.org/pvldb/vol18/p1509-miao.pdf)
- [PCSR: Packed Compressed Sparse Row](https://itshelenxu.github.io/files/papers/pcsr.pdf)
- [VCSR: Mutable CSR Graph Format](https://webpages.charlotte.edu/ddai/data/dong-ccgrid-22.pdf)

### Graph Database Overviews
- [Survey: Graph Databases](https://www.researchgate.net/publication/392314679_Survey_Graph_Databases)
- [Demystifying Graph Databases: Analysis and Taxonomy](https://dl.acm.org/doi/full/10.1145/3604932)
- [An Overview of Graph Databases (Academic, 2021)](https://academic.oup.com/database/article/doi/10.1093/database/baab026/6277722)

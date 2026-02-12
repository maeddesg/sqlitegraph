# Codebase Structure

**Analysis Date:** 2025-02-12

## Directory Layout

```
sqlitegraph/
├── sqlitegraph/              # Main library crate
│   ├── src/                 # Library source code
│   │   ├── algo/            # Graph algorithms (35+)
│   │   ├── backend/         # Storage backend implementations
│   │   │   ├── native/      # Native backend (file-based)
│   │   │   │   └── v2/     # Native V2 (clustered, WAL)
│   │   │   └── sqlite/      # SQLite backend
│   │   ├── graph/           # Core graph types and operations
│   │   ├── hnsw/            # HNSW vector search
│   │   ├── pattern_engine/  # Triple pattern matching
│   │   ├── config/          # Configuration and factory
│   │   └── *.rs            # Core modules (mvcc, cache, etc.)
│   ├── benches/            # Criterion benchmarks
│   └── Cargo.toml          # Library manifest
├── sqlitegraph-cli/          # Command-line interface crate
│   ├── src/                 # CLI source code
│   ├── examples/             # Example usage files
│   └── Cargo.toml          # CLI manifest
├── src/                    # Top-level workspace source (minimal)
├── tests/                   # Integration tests
├── docs/                    # Documentation
├── scripts/                 # Utility scripts
├── .planning/              # Project planning artifacts
├── .codemcp/               # Code graph database (not committed)
└── Cargo.toml              # Workspace manifest
```

## Directory Purposes

**sqlitegraph/src/:**
- Purpose: Core library implementation
- Contains: All graph database functionality, algorithms, backends
- Key files: `lib.rs`, `backend.rs`, `graph/mod.rs`, `algo/mod.rs`

**sqlitegraph/src/algo/:**
- Purpose: Graph theory algorithms (backend-agnostic)
- Contains: 35+ algorithm implementations organized by category
- Key files:
  - `mod.rs` - Public API re-exports
  - `centrality.rs` - PageRank, Betweenness
  - `community.rs` - Louvain, Label Propagation
  - `reachability.rs` - Reachability analysis
  - `dominators.rs`, `post_dominators.rs` - CFG analysis
  - `natural_loops.rs` - Loop detection
  - `control_dependence.rs` - Control dependence graph
  - `cycle_basis.rs` - Cycle enumeration
  - `path_enumeration.rs` - Path enumeration
  - `taint_analysis.rs` - Security analysis
  - `cut_partition.rs` - Graph partitioning
  - `program_slicing.rs` - Program slicing
  - `call_graph_analysis.rs` - Call graph utilities
  - `graph_diff.rs` - Graph diffing
  - `subgraph_isomorphism.rs` - Pattern matching
  - `graph_similarity.rs` - Similarity metrics
  - `graph_rewriting.rs` - DPO rewriting

**sqlitegraph/src/backend/:**
- Purpose: Storage backend implementations and abstraction
- Contains: `GraphBackend` trait, backend implementations
- Key files:
  - `mod.rs` - Backend module root
  - `sqlite/` - SQLite backend implementation
  - `native/` - Native backend implementation

**sqlitegraph/src/backend/sqlite/:**
- Purpose: SQLite-backed graph storage
- Contains: Connection management, SQL execution
- Key files:
  - `mod.rs` - Module exports
  - `impl_.rs` - Main implementation
  - `types.rs` - `NodeSpec`, `EdgeSpec`, `NeighborQuery`
  - `helpers.rs` - SQLite utilities

**sqlitegraph/src/backend/native/:**
- Purpose: High-performance custom binary storage
- Contains: File I/O, adjacency, node/edge stores
- Key files:
  - `mod.rs` - Native module exports
  - `graph_backend.rs` - `NativeGraphBackend` implementation
  - `graph_file/` - Memory-mapped file operations
  - `adjacency/` - Neighbor iteration
  - `edge_store/` - Edge storage and clustering
  - `node_store.rs` - Node storage
  - `graph_ops/` - Traversal implementations
  - `constants.rs` - Storage constants
  - `types/` - Native type definitions

**sqlitegraph/src/backend/native/v2/:**
- Purpose: Next-generation native backend with clustering
- Contains: Edge clusters, WAL, KV store, Pub/Sub
- Key files:
  - `mod.rs` - V2 module exports
  - `edge_cluster/` - Compact edge records
  - `node_record_v2/` - V2 node records
  - `wal/` - Write-Ahead Log (manager, reader, writer, recovery, checkpoint)
  - `kv_store/` - Transactional key-value storage
  - `pubsub/` - Event notification
  - `free_space/` - Free space management
  - `string_table/` - String deduplication
  - `storage/` - Delta index for MVCC
  - `snapshot/`, `backup/`, `restore/` - Data management
  - `migration/` - Format migration utilities

**sqlitegraph/src/graph/:**
- Purpose: Core graph database types and operations
- Contains: `SqliteGraph`, `GraphEntity`, `GraphEdge`, adjacency
- Key files:
  - `mod.rs` - Graph module exports
  - `core.rs` - `SqliteGraph` main implementation
  - `types.rs` - `GraphEntity`, `GraphEdge` types
  - `adjacency.rs` - Adjacency list management
  - `entity_ops.rs` - Node CRUD operations
  - `edge_ops.rs` - Edge CRUD operations
  - `pattern_matching.rs` - Pattern query execution
  - `snapshot.rs` - Graph snapshot implementation
  - `pool/` - Connection pooling
  - `metrics/` - Performance metrics

**sqlitegraph/src/hnsw/:**
- Purpose: Vector similarity search (HNSW index)
- Contains: Multi-layer graph index, distance metrics, storage
- Key files:
  - `mod.rs` - HNSW module exports
  - `index.rs` - Main HNSW index
  - `builder.rs` - Configuration builder
  - `config.rs` - HNSW parameters
  - `distance_metric.rs` - Distance metric enum
  - `distance_functions.rs` - Distance implementations
  - `storage.rs` - Vector storage trait
  - `multilayer.rs` - Multi-layer graph
  - `layer.rs` - Single layer management
  - `neighborhood.rs` - Neighbor selection
  - `serialization.rs` - Persistence
  - `simd.rs` - SIMD optimizations

**sqlitegraph/src/pattern_engine/:**
- Purpose: Triple pattern matching (subject-predicate-object)
- Contains: Pattern types, matcher, query executor
- Key files:
  - `mod.rs` - Pattern module exports
  - `pattern.rs` - `PatternTriple` type
  - `matcher.rs` - Pattern matching engine
  - `query.rs` - Query execution
  - `property.rs` - Property filters

**sqlitegraph/src/pattern_engine_cache/:**
- Purpose: Fast-path optimization for pattern matching
- Contains: Edge validation, fast path detection/execution
- Key files:
  - `mod.rs` - Cache module exports
  - `edge_validation.rs` - Edge validation logic
  - `fast_path_detection.rs` - Fast path detection
  - `fast_path_execution.rs` - Fast path execution

**sqlitegraph/src/mvcc.rs:**
- Purpose: Multi-version concurrency control
- Contains: `SnapshotState`, `GraphSnapshot`, snapshot manager
- Uses ArcSwap for lock-free state updates

**sqlitegraph/src/snapshot.rs:**
- Purpose: Snapshot isolation type definitions
- Contains: `SnapshotId` type
- Enforces ACID read isolation

**sqlitegraph/src/config/:**
- Purpose: Configuration and backend selection
- Contains: `GraphConfig`, `BackendKind`, factory function
- Key files:
  - `mod.rs` - Config module exports
  - `kinds.rs` - `BackendKind` enum
  - `config.rs` - `GraphConfig` struct
  - `factory.rs` - `open_graph()` factory
  - `sqlite.rs` - SQLite config
  - `native.rs` - Native config

**sqlitegraph-cli/src/:**
- Purpose: Command-line interface
- Contains: CLI argument parsing, command handlers
- Key files:
  - `main.rs` - CLI entry point
  - `cli.rs` - Argument parsing
  - `client.rs` - Backend client
  - `dsl.rs` - DSL commands
  - `reasoning.rs` - Reasoning commands

**tests/:**
- Purpose: Integration tests (outside library crate)
- Contains: API ergonomics, DSL, pipeline, V2 regression tests
- Key files:
  - `api_ergonomics_tests.rs`
  - `dsl_tests.rs`
  - `pipeline_tests.rs`
  - `v2_*.rs` - V2 regression and invariant tests
  - `snapshot_*.rs` - Snapshot tests
  - `kv_rollback_test.rs`

**docs/:**
- Purpose: User and developer documentation
- Contains: Architecture guides, algorithm docs, troubleshooting
- Key files: `ARCHITECTURE.md`, `TESTING.md`, algorithm guides

**benches/ (sqlitegraph/benches/):**
- Purpose: Performance benchmarks using Criterion
- Contains: Algorithm benchmarks, backend comparisons
- Key files:
  - `algo_benchmarks.rs`
  - `bfs.rs`
  - `comprehensive_performance.rs`
  - `graph_theory_benchmarks.rs`

## Key File Locations

**Entry Points:**
- `sqlitegraph/src/lib.rs` - Library root with public API re-exports
- `sqlitegraph-cli/src/main.rs` - CLI entry point with command routing

**Configuration:**
- `sqlitegraph/src/config/mod.rs` - Configuration module root
- `sqlitegraph/src/config/kinds.rs` - `BackendKind` enum (SQLite, Native)
- `sqlitegraph/src/config/factory.rs` - `open_graph()` factory function
- `sqlitegraph/src/config/config.rs` - `GraphConfig` struct
- `sqlitegraph/src/config/sqlite.rs` - `SqliteConfig`
- `sqlitegraph/src/config/native.rs` - `NativeConfig`

**Core Logic:**
- `sqlitegraph/src/graph/core.rs` - `SqliteGraph` main implementation
- `sqlitegraph/src/graph/types.rs` - `GraphEntity`, `GraphEdge` types
- `sqlitegraph/src/backend.rs` - `GraphBackend` trait definition
- `sqlitegraph/src/algo/mod.rs` - Algorithm re-exports

**Backend Implementations:**
- `sqlitegraph/src/backend/sqlite/impl_.rs` - SQLite backend implementation
- `sqlitegraph/src/backend/native/graph_backend.rs` - Native backend implementation
- `sqlitegraph/src/backend/native/v2/mod.rs` - V2 module exports

**Graph Operations:**
- `sqlitegraph/src/bfs.rs` - BFS traversal
- `sqlitegraph/src/multi_hop.rs` - K-hop queries
- `sqlitegraph/src/query.rs` - High-level query interface
- `sqlitegraph/src/graph_opt.rs` - Graph optimization helpers

**Utilities:**
- `sqlitegraph/src/cache.rs` - LRU-K adjacency cache
- `sqlitegraph/src/query_cache.rs` - Query result caching
- `sqlitegraph/src/introspection.rs` - Debugging APIs
- `sqlitegraph/src/recovery.rs` - Backup and restore
- `sqlitegraph/src/schema.rs` - Database schema
- `sqlitegraph/src/errors.rs` - Error types
- `sqlitegraph/src/api_ergonomics.rs` - Public API types
- `sqlitegraph/src/progress.rs` - Progress tracking

**Testing:**
- `tests/` - Integration tests (workspace root)
- `sqlitegraph/src/**/tests.rs` - Module-level unit tests
- `benches/` - Criterion benchmarks

## Naming Conventions

**Files:**
- `mod.rs` - Module public interface and re-exports
- `types.rs` - Type definitions for module
- `errors.rs` - Error types (if module-specific)
- `core.rs` - Core implementation logic
- `helpers.rs` / `utils.rs` - Utility functions
- `tests.rs` - Module-level tests (not test/ directory)
- `bench_*.rs` - Benchmark utilities
- `*.rs` - Descriptive names: `bfs.rs`, `cache.rs`, `mvcc.rs`

**Directories:**
- `algo/` - Algorithm implementations
- `backend/` - Storage backends
- `v2/` - Version 2 of a component
- `metrics/` - Performance/statistics tracking
- `cache/` - Caching implementations
- `ops/` - Operation implementations

**Types:**
- `Graph*` - Graph-related types (GraphEntity, GraphEdge, GraphQuery)
- `*Backend` - Backend implementations (SqliteGraphBackend, NativeGraphBackend)
- `*Config` - Configuration structs (GraphConfig, HnswConfig, NativeConfig)
- `*Error` - Error types (SqliteGraphError, NativeBackendError, KvStoreError)
- `*Result` - Result types with success/failure
- `*Spec` - Input specification types (NodeSpec, EdgeSpec)
- `*Manager` - Resource managers (SnapshotManager, PoolManager, FreeSpaceManager)
- `*Store` - Storage implementations (NodeStore, EdgeStore, KvStore, StringTable)

## Where to Add New Code

**New Graph Algorithm:**
- Primary code: `sqlitegraph/src/algo/<algorithm_name>.rs`
- Tests: Inline `#[cfg(test)]` or `sqlitegraph/src/algo/<algorithm_name>/tests.rs`
- Re-export: Add `pub use` to `sqlitegraph/src/algo/mod.rs`
- Progress support: Add `*_with_progress` variant

**New Backend Feature (SQLite):**
- Implementation: `sqlitegraph/src/backend/sqlite/<feature>.rs`
- Tests: Inline `#[cfg(test)]` module
- Trait method: Add to `GraphBackend` trait in `sqlitegraph/src/backend.rs`

**New Backend Feature (Native):**
- Implementation: `sqlitegraph/src/backend/native/<feature>.rs`
- Tests: `sqlitegraph/src/backend/native/<feature>/tests.rs` or inline
- Update: `sqlitegraph/src/backend/native/mod.rs` exports

**New Backend Feature (Native V2):**
- Implementation: `sqlitegraph/src/backend/native/v2/<feature>.rs`
- Tests: Inline `#[cfg(test)]` or separate test module
- Update: `sqlitegraph/src/backend/native/v2/mod.rs` exports

**New HNSW Distance Metric:**
- Implementation: `sqlitegraph/src/hnsw/distance_functions.rs`
- Registration: Add to `DistanceMetric` enum in `sqlitegraph/src/hnsw/distance_metric.rs`

**New CLI Command:**
- Handler: `sqlitegraph-cli/src/main.rs` (add `run_*` function)
- Help: Update help text in `sqlitegraph-cli/src/cli.rs`

**New Pattern Matching Feature:**
- Implementation: `sqlitegraph/src/pattern_engine/<feature>.rs`
- Optimization: `sqlitegraph/src/pattern_engine_cache/<feature>.rs`

**Utilities:**
- Shared helpers: `sqlitegraph/src/bench_utils.rs` (for benchmarks)
- Test utilities: `sqlitegraph/src/fault_injection.rs` (for testing)

## Special Directories

**target/:**
- Purpose: Cargo build output
- Generated: Yes
- Committed: No

**.cargo/ (sqlitegraph/.cargo/):**
- Purpose: Cargo build cache
- Generated: Yes
- Committed: Yes (for registry caching)

**.codemcp/:**
- Purpose: Code graph database (Mag indexer)
- Generated: Yes
- Committed: No

**.planning/:**
- Purpose: Project planning and documentation
- Generated: Mixed (manual edits + generated)
- Committed: Yes

**docs/:**
- Purpose: User and developer documentation
- Generated: No
- Committed: Yes

**benches/:**
- Purpose: Criterion benchmark suites
- Generated: No
- Committed: Yes

**sqlitegraph_temp/:**
- Purpose: Temporary testing directory
- Generated: Yes
- Committed: No

---

*Structure analysis: 2025-02-12*

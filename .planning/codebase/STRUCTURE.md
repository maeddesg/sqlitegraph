# Codebase Structure

**Analysis Date:** 2025-02-11

## Directory Layout

```
sqlitegraph/
├── sqlitegraph/          # Main library crate
│   ├── src/             # Library source code
│   ├── benches/         # Criterion benchmarks
│   └── Cargo.toml       # Library manifest
├── sqlitegraph-cli/      # Command-line interface crate
│   ├── src/             # CLI source code
│   ├── examples/         # Example usage files
│   └── Cargo.toml       # CLI manifest
├── src/                # Top-level workspace source (minimal)
├── tests/              # Integration tests
├── docs/               # Documentation (internal and public)
├── scripts/            # Utility scripts
├── .planning/          # Project planning artifacts
└── Cargo.toml          # Workspace manifest
```

## Directory Purposes

**sqlitegraph/src/:**
- Purpose: Core library implementation
- Contains: All graph database functionality, algorithms, backends
- Key files: `lib.rs`, `backend.rs`, `graph/mod.rs`, `algo/mod.rs`

**sqlitegraph/src/backend/:**
- Purpose: Storage backend implementations
- Contains: `GraphBackend` trait, SQLite and Native backends
- Key files: `mod.rs`, `sqlite/mod.rs`, `native/mod.rs`

**sqlitegraph/src/backend/native/:**
- Purpose: Native storage layer (file-based, no SQLite)
- Contains: File I/O, adjacency, node/edge stores
- Key files: `mod.rs`, `graph_file/mod.rs`, `adjacency/mod.rs`, `edge_store/mod.rs`

**sqlitegraph/src/backend/native/v2/:**
- Purpose: Next-generation native backend with clustering
- Contains: Edge clusters, node records v2, WAL, KV store, pub/sub
- Key files: `mod.rs`, `edge_cluster/mod.rs`, `node_record_v2/mod.rs`, `wal/mod.rs`, `kv_store/mod.rs`, `pubsub/mod.rs`

**sqlitegraph/src/algo/:**
- Purpose: Graph theory algorithms
- Contains: Centrality, community detection, reachability, CFG analysis, taint analysis
- Key files: `mod.rs`, `centrality.rs`, `community.rs`, `reachability.rs`, `dominators.rs`, `taint_analysis.rs`

**sqlitegraph/src/graph/:**
- Purpose: Core graph database types and operations
- Contains: `SqliteGraph`, `GraphEntity`, `GraphEdge`, adjacency
- Key files: `mod.rs`, `core.rs`, `types.rs`, `adjacency.rs`, `entity_ops.rs`, `edge_ops.rs`

**sqlitegraph/src/hnsw/:**
- Purpose: Vector similarity search (HNSW index)
- Contains: Index implementation, distance metrics, storage
- Key files: `mod.rs`, `index.rs`, `builder.rs`, `distance_metric.rs`

**sqlitegraph/src/pattern_engine/:**
- Purpose: Triple pattern matching
- Contains: Pattern types, matcher, query executor
- Key files: `mod.rs`, `pattern.rs`, `matcher.rs`, `query.rs`

**sqlitegraph/src/pattern_engine_cache/:**
- Purpose: Fast-path optimization for pattern matching
- Contains: Edge validation, fast path detection/execution
- Key files: `mod.rs`, `edge_validation.rs`, `fast_path_detection.rs`, `fast_path_execution.rs`

**sqlitegraph/src/mvcc/:**
- Purpose: Multi-version concurrency control
- Contains: Snapshot state, snapshot manager
- Key files: `mvcc.rs` (top-level)

**sqlitegraph/src/config/:**
- Purpose: Configuration and backend selection
- Contains: `GraphConfig`, `BackendKind`, factory function
- Key files: `mod.rs`, `config.rs`, `kinds.rs`, `factory.rs`

**sqlitegraph-cli/src/:**
- Purpose: Command-line interface
- Contains: CLI argument parsing, command handlers
- Key files: `main.rs`, `cli.rs`, `client.rs`, `dsl.rs`, `reasoning.rs`

**tests/:**
- Purpose: Integration-level tests (outside library crate)
- Contains: API ergonomics, DSL tests, pipeline tests, V2 regression tests
- Key files: `api_ergonomics_tests.rs`, `dsl_tests.rs`, `v2_*.rs`

**docs/:**
- Purpose: User and developer documentation
- Contains: Architecture guides, algorithm documentation, troubleshooting
- Key files: `ARCHITECTURE.md`, `GRAPH_ALGORITHMS_GUIDE.md`, `TESTING.md`

**benches/ (sqlitegraph/benches/):**
- Purpose: Performance benchmarks using Criterion
- Contains: Algorithm benchmarks, backend comparisons
- Key files: `algo_benchmarks.rs`, `bfs.rs`, `comprehensive_performance.rs`, `graph_theory_benchmarks.rs`

## Key File Locations

**Entry Points:**
- `sqlitegraph/src/lib.rs`: Library root with public API re-exports
- `sqlitegraph-cli/src/main.rs`: CLI entry point with command routing

**Configuration:**
- `sqlitegraph/src/config/mod.rs`: Configuration module root
- `sqlitegraph/src/config/kinds.rs`: `BackendKind` enum (SQLite, Native)
- `sqlitegraph/src/config/factory.rs`: `open_graph()` factory function

**Core Logic:**
- `sqlitegraph/src/graph/core.rs`: `SqliteGraph` main implementation
- `sqlitegraph/src/graph/types.rs`: `GraphEntity`, `GraphEdge` types
- `sqlitegraph/src/backend.rs`: `GraphBackend` trait definition
- `sqlitegraph/src/algo/mod.rs`: Algorithm re-exports

**Backend Implementations:**
- `sqlitegraph/src/backend/sqlite/mod.rs`: SQLite backend
- `sqlitegraph/src/backend/native/mod.rs`: Native backend v1
- `sqlitegraph/src/backend/native/v2/mod.rs`: Native backend v2 (clustered)

**Graph Operations:**
- `sqlitegraph/src/bfs.rs`: BFS traversal
- `sqlitegraph/src/multi_hop.rs`: K-hop queries
- `sqlitegraph/src/query.rs`: High-level query interface

**Testing:**
- `tests/`: Integration tests (workspace root)
- `sqlitegraph/src/**/tests.rs`: Module-level unit tests
- `benches/`: Criterion benchmarks

## Naming Conventions

**Files:**
- `mod.rs`: Module public interface and re-exports
- `types.rs`: Type definitions for the module
- `errors.rs`: Error types (if module-specific)
- `core.rs`: Core implementation logic
- `helpers.rs` / `utils.rs`: Utility functions
- `tests.rs`: Module-level tests (not test/ directory)
- `bench_*.rs`: Benchmark utilities

**Directories:**
- `algo/`: Algorithm implementations
- `backend/`: Storage backends
- `v2/`: Version 2 of a component
- `metrics/`: Performance/statistics tracking
- `cache/`: Caching implementations

**Types:**
- `Graph*`: Graph-related types (GraphEntity, GraphEdge, GraphQuery)
- `*Backend`: Backend implementations (SqliteGraphBackend, NativeGraphBackend)
- `*Config`: Configuration structs (GraphConfig, HnswConfig)
- `*Error`: Error types (SqliteGraphError, NativeBackendError)
- `*Result`: Result types with success/failure
- `*Spec`: Input specification types (NodeSpec, EdgeSpec)

## Where to Add New Code

**New Graph Algorithm:**
- Primary code: `sqlitegraph/src/algo/<algorithm_name>.rs`
- Tests: `sqlitegraph/src/algo/<algorithm_name>/tests.rs` or inline `#[cfg(test)]`
- Re-export: Add `pub use` to `sqlitegraph/src/algo/mod.rs`

**New Backend Feature:**
- Implementation: `sqlitegraph/src/backend/<backend>/feature_name.rs`
- Tests: `sqlitegraph/src/backend/<backend>/feature_name/tests.rs`
- Trait method: Add to `GraphBackend` trait in `sqlitegraph/src/backend.rs`

**New HNSW Distance Metric:**
- Implementation: `sqlitegraph/src/hnsw/distance_functions.rs`
- Registration: Add to `DistanceMetric` enum in `sqlitegraph/src/hnsw/distance_metric.rs`

**New CLI Command:**
- Handler: `sqlitegraph-cli/src/main.rs` (add `run_*` function)
- Help: Update `CommandLineConfig::help()` in `sqlitegraph-cli/src/cli.rs`

**New Pattern Matching Feature:**
- Implementation: `sqlitegraph/src/pattern_engine/<feature>.rs`
- Optimization: `sqlitegraph/src/pattern_engine_cache/<feature>.rs`

**Utilities:**
- Shared helpers: `sqlitegraph/src/bench_utils.rs` (for benchmarks)
- Test utilities: `sqlitegraph/src/fault_injection.rs` (for testing)

## Special Directories

**target/:**
- Purpose: Cargo build output (not committed)
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

**docs/internal/:**
- Purpose: Internal implementation documentation
- Generated: No
- Committed: Yes

**benches/:**
- Purpose: Criterion benchmark suites
- Generated: No
- Committed: Yes

---

*Structure analysis: 2025-02-11*

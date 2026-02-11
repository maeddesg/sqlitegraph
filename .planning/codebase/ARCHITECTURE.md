# Architecture

**Analysis Date:** 2025-02-11

## Pattern Overview

**Overall:** Dual-backend graph database with unified trait abstraction

**Key Characteristics:**
- Backend-agnostic API via `GraphBackend` trait
- Dual storage backends (SQLite for ACID, Native for performance)
- MVCC-lite snapshot isolation for concurrent reads
- Pluggable graph algorithms library (35+ algorithms)
- Vector similarity search via HNSW index
- WAL-based durability for native backend

## Layers

**API Layer (`sqlitegraph/src/lib.rs`):**
- Purpose: Public API surface and re-exports
- Location: `sqlitegraph/src/lib.rs`
- Contains: Re-exports of core types, configuration, algorithms
- Depends on: All internal modules
- Used by: Client applications, CLI

**Backend Layer (`sqlitegraph/src/backend/`):**
- Purpose: Storage abstraction and implementations
- Location: `sqlitegraph/src/backend/mod.rs`
- Contains: `GraphBackend` trait, `SqliteGraphBackend`, `NativeGraphBackend`
- Depends on: Graph core types, errors, configuration
- Used by: `SqliteGraph`, higher-level APIs

**SQLite Backend (`sqlitegraph/src/backend/sqlite/`):**
- Purpose: SQLite-backed graph storage with full ACID transactions
- Location: `sqlitegraph/src/backend/sqlite/mod.rs`
- Contains: Connection management, query execution, schema
- Depends on: `rusqlite` library
- Used by: Applications requiring SQL compatibility

**Native Backend (`sqlitegraph/src/backend/native/`):**
- Purpose: High-performance custom binary storage
- Location: `sqlitegraph/src/backend/native/mod.rs`
- Contains: File I/O, adjacency storage, node/edge stores, WAL
- Depends on: `memmap2`, `binrw`, `bytemuck`
- Used by: Performance-critical applications

**Native V2 Backend (`sqlitegraph/src/backend/native/v2/`):**
- Purpose: Next-generation native backend with clustering
- Location: `sqlitegraph/src/backend/native/v2/mod.rs`
- Contains: Edge clusters, node records v2, WAL, checkpoint, free space management
- Depends on: Native backend primitives, SIMD optimizations
- Used by: Native V2 enabled applications

**Graph Core (`sqlitegraph/src/graph/`):**
- Purpose: Core graph database operations and types
- Location: `sqlitegraph/src/graph/mod.rs`
- Contains: `SqliteGraph`, `GraphEntity`, `GraphEdge`, adjacency management
- Depends on: Backend implementations, MVCC system
- Used by: All graph operations

**Algorithms Layer (`sqlitegraph/src/algo/`):**
- Purpose: Graph theory algorithms
- Location: `sqlitegraph/src/algo/mod.rs`
- Contains: Centrality, community detection, reachability, CFG analysis, taint analysis
- Depends on: `GraphBackend` trait
- Used by: Applications requiring graph analysis

**HNSW Vector Search (`sqlitegraph/src/hnsw/`):**
- Purpose: Approximate nearest neighbor search
- Location: `sqlitegraph/src/hnsw/mod.rs`
- Contains: Index, builder, storage, distance metrics
- Depends on: Backend storage
- Used by: Vector similarity applications

**Pattern Matching (`sqlitegraph/src/pattern_engine/`):**
- Purpose: Triple pattern matching (subject-predicate-object)
- Location: `sqlitegraph/src/pattern_engine/mod.rs`
- Contains: `PatternTriple`, `match_triples`, fast-path cache
- Depends on: Backend graph operations
- Used by: Query operations

**MVCC Snapshot System (`sqlitegraph/src/mvcc.rs`):**
- Purpose: Multi-version concurrency control for read isolation
- Location: `sqlitegraph/src/mvcc.rs`
- Contains: `SnapshotState`, `GraphSnapshot`, snapshot manager
- Depends on: `arc_swap` for lock-free updates
- Used by: Concurrent read operations

**Configuration (`sqlitegraph/src/config/`):**
- Purpose: Backend selection and configuration
- Location: `sqlitegraph/src/config/mod.rs`
- Contains: `GraphConfig`, `BackendKind`, `open_graph` factory
- Depends on: Backend-specific config types
- Used by: Application initialization

## Data Flow

**Graph Initialization Flow:**

1. User calls `open_graph(path, &GraphConfig)`
2. `GraphConfig.backend` selects `SqliteGraphBackend` or `NativeGraphBackend`
3. Backend creates/opens storage file
4. Schema validation/migration runs (SQLite only)
5. `SqliteGraph` wrapper initialized with backend
6. Cache and snapshot manager initialized

**Write Operation Flow:**

1. Application calls `graph.insert_node(node_spec)` or `graph.insert_edge(...)`
2. Operation routed to backend via `GraphBackend` trait
3. Backend validates input
4. Transaction started (BEGIN IMMEDIATE for SQLite, WAL record for Native)
5. Data written to storage
6. Indexes updated (adjacency, string tables)
7. Transaction committed
8. Cache invalidated for affected nodes
9. Snapshot ID updated
10. Pub/Sub events emitted (if subscribed)

**Read Operation Flow:**

1. Application calls `graph.fetch_outgoing(node_id)` or similar
2. Cache checked (LRU-K adjacency cache)
3. On cache miss: query via `GraphBackend::neighbors(snapshot_id, ...)`
4. Backend retrieves from storage (SQLite query or file read)
5. Results cached
6. Returns to caller

**Snapshot Isolation Flow:**

1. Writer creates transaction, gets `SnapshotId`
2. Reader creates `GraphSnapshot` with current `SnapshotId`
3. Reader sees only data committed at/before snapshot
4. Writer commits, new `SnapshotId` created
5. Old reader snapshots remain consistent
6. New readers see new `SnapshotId`

**State Management:**
- **Graph state**: Stored in backend (SQLite tables or binary file)
- **Cache state**: LRU-K in-memory adjacency cache
- **Snapshot state**: Immutable `HashMap` clones for isolation
- **WAL state**: Append-only log for native backend crash recovery

## Key Abstractions

**GraphBackend Trait:**
- Purpose: Unified interface for storage backends
- Examples: `sqlitegraph/src/backend.rs`
- Pattern: Strategy pattern - pluggable storage implementations

**SnapshotId:**
- Purpose: Transaction identifier for MVCC isolation
- Examples: `sqlitegraph/src/snapshot.rs`
- Pattern: Opaque token representing committed transaction state

**NodeId / EdgeId:**
- Purpose: Strongly-typed identifiers (newtype wrappers)
- Examples: `sqlitegraph/src/api_ergonomics.rs`
- Pattern: Newtype pattern for type safety

**ChainStep:**
- Purpose: Directional edge traversal specification
- Examples: `sqlitegraph/src/multi_hop.rs`
- Pattern: Builder pattern for query construction

## Entry Points

**Library Entry Point (`sqlitegraph/src/lib.rs`):**
- Location: `sqlitegraph/src/lib.rs`
- Triggers: `use sqlitegraph::` statements
- Responsibilities: Public API re-exports, feature flags

**CLI Entry Point (`sqlitegraph-cli/src/main.rs`):**
- Location: `sqlitegraph-cli/src/main.rs`
- Triggers: `sqlitegraph` command execution
- Responsibilities: Argument parsing, backend selection, command dispatch

**Factory Function (`sqlitegraph/src/config/factory.rs`):**
- Location: `sqlitegraph/src/config/factory.rs`
- Triggers: `open_graph(path, config)` calls
- Responsibilities: Backend instantiation based on `BackendKind`

## Error Handling

**Strategy:** Structured error type with thiserror derivation

**Patterns:**
- `SqliteGraphError` enum with variants for each error category
- Backend-specific errors wrapped (`NativeError`)
- Context preserved via `#[error]` attributes
- Conversion via `From` traits

**Error Categories:**
- `ConnectionError`: Database/file open failures
- `SchemaError`: Migration/validation errors
- `QueryError`: Read/write operation failures
- `NotFound`: Entity/edge not found
- `InvalidInput`: User-provided input validation
- `TransactionError`: Transaction commit/rollback failures
- `ValidationError`: Constraint violations
- `Unsupported`: Operation not available for backend

## Cross-Cutting Concerns

**Logging:** Feature-gated via `debug` feature
- Uses conditional compilation (`#[cfg(feature = "debug")]`)
- Zero overhead in release builds
- Macros: `debug_log!`, `info_log!`, `warn_log!`, `error_log!`

**Validation:** Schema versioning and migration
- SQLite: `graph_meta` table tracks version
- Native: Header magic bytes and version field
- Automatic migration on open (optional via config)

**Authentication:** Not applicable (embedded database)

**Caching:** LRU-K adjacency cache
- Automatic invalidation on writes
- Traversal-aware scoring
- 95%+ hit ratio for BFS workloads

**Pub/Sub:** In-process event delivery (Native V2 only)
- Events emitted on commit
- Best-effort delivery
- Event types: NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted

**SIMD Optimization:** CPU feature detection
- Runtime dispatch based on CPUID
- Profiles: Generic, AVX2, Zen4
- Used in: Distance calculations, batch operations

---

*Architecture analysis: 2025-02-11*

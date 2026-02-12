# Architecture

**Analysis Date:** 2025-02-12

## Pattern Overview

**Overall:** Dual-Backend Graph Database with Unified GraphBackend Trait

**Key Characteristics:**
- Unified `GraphBackend` trait abstraction over SQLite and Native storage engines
- MVCC-lite snapshot isolation for concurrent read operations
- Pluggable storage backends selected at runtime via `BackendKind`
- Layered architecture: Public API → GraphBackend → Backend Implementation → Storage
- 35+ graph algorithms provided via `algo` module (backend-agnostic)
- Native V2 backend with WAL, KV store, and Pub/Sub support

## Layers

**Public API Layer:**
- Purpose: User-facing graph operations and types
- Location: `sqlitegraph/src/lib.rs`
- Contains: `SqliteGraph`, `open_graph()`, `GraphConfig`, algorithm functions
- Depends on: GraphBackend trait, backend implementations
- Used by: Application code, CLI tools

**Backend Abstraction Layer:**
- Purpose: Unified interface for all storage operations
- Location: `sqlitegraph/src/backend.rs`
- Contains: `GraphBackend` trait (38+ methods), `BackendKind` enum
- Depends on: `SqliteGraphBackend`, `NativeGraphBackend`, storage types
- Used by: `SqliteGraph`, higher-level APIs

**Backend Implementations Layer:**
- Purpose: Concrete storage engines
- Location: `sqlitegraph/src/backend/`
- Contains: `sqlite/` (SQLite backend), `native/` (Native backend)
- Depends on: Storage engines (rusqlite for SQLite, custom file format for Native)
- Used by: GraphBackend trait consumers

**SQLite Backend:**
- Purpose: SQLite-backed graph storage with full ACID transactions
- Location: `sqlitegraph/src/backend/sqlite/mod.rs`
- Contains: Connection management, SQL query execution, schema management
- Depends on: `rusqlite` library, SQLite database
- Used by: Applications requiring SQL compatibility and tooling

**Native Backend Sub-Layers:**
- Purpose: Modular native storage architecture
- Location: `sqlitegraph/src/backend/native/`
- Contains:
  - `graph_file/` - Memory-mapped file I/O, header management
  - `node_store/`, `edge_store/` - Record storage and indexing
  - `adjacency/` - Neighbor iteration, cluster operations
  - `graph_ops/` - Traversal operations (BFS, k-hop, pathfinding)
  - `v2/` - Next-generation kernel with WAL, KV, Pub/Sub

**Native V2 Backend:**
- Purpose: Next-generation storage with clustered edges and ACID guarantees
- Location: `sqlitegraph/src/backend/native/v2/`
- Contains:
  - `edge_cluster/` - Compact edge records with spatial locality
  - `wal/` - Write-Ahead Log with commit sequencing
  - `kv_store/` - Transactional key-value storage with MVCC
  - `pubsub/` - In-process event notification
  - `free_space/` - Contiguous allocation management
  - `string_table/` - Deduplicated string storage
  - `snapshot/`, `backup/`, `restore/` - Data management

**Graph Core Layer:**
- Purpose: Core graph database operations and types
- Location: `sqlitegraph/src/graph/`
- Contains: `SqliteGraph`, `GraphEntity`, `GraphEdge`, adjacency management
- Depends on: Backend implementations, MVCC system
- Used by: All graph operations

**Algorithm Layer:**
- Purpose: Backend-agnostic graph analysis
- Location: `sqlitegraph/src/algo/`
- Contains: 35+ algorithms (centrality, community detection, CFG analysis, taint analysis)
- Depends on: GraphBackend trait only (not backend-specific)
- Used by: Applications requiring graph analysis

**HNSW Vector Search Layer:**
- Purpose: Approximate nearest neighbor search
- Location: `sqlitegraph/src/hnsw/`
- Contains: Multi-layer graph index, distance metrics, SIMD optimization
- Depends on: Storage backends (SQLite or Native for persistence)
- Used by: Vector similarity applications

**Pattern Matching Layer:**
- Purpose: Triple pattern matching (subject-predicate-object)
- Location: `sqlitegraph/src/pattern_engine/`
- Contains: `PatternTriple`, `match_triples`, fast-path cache
- Depends on: Backend graph operations
- Used by: Query operations

**MVCC/Snapshot Layer:**
- Purpose: Concurrent read isolation
- Location: `sqlitegraph/src/mvcc.rs`, `sqlitegraph/src/snapshot.rs`
- Contains: `SnapshotManager`, `GraphSnapshot`, `SnapshotId`
- Depends on: ArcSwap for lock-free state, SQLite read-only connections
- Used by: All read operations requiring isolation

**Configuration Layer:**
- Purpose: Backend selection and configuration
- Location: `sqlitegraph/src/config/`
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

**Write Operation Flow (e.g., insert_node):**

1. Application calls `graph.insert_node(node_spec)`
2. Operation routed to backend via `GraphBackend` trait
3. Backend validates input
4. Transaction started (BEGIN IMMEDIATE for SQLite, WAL record for Native)
5. Data written to storage
6. Indexes updated (adjacency, string tables)
7. Transaction committed
8. Cache invalidated for affected nodes
9. Snapshot ID updated
10. Pub/Sub events emitted (if subscribed, V2 only)

**Read Operation Flow (e.g., neighbors):**

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
- Purpose: Unified interface for all graph storage operations
- Examples: `sqlitegraph/src/backend.rs` (trait definition)
- Pattern: Strategy pattern - pluggable storage implementations
- Methods: 38+ operations including:
  - Write ops: `insert_node()`, `insert_edge()`, `update_node()`, `delete_entity()`
  - Read ops: `get_node()`, `neighbors()`, `entity_ids()`
  - Traversal ops: `bfs()`, `shortest_path()`, `k_hop()`, `k_hop_filtered()`
  - Pattern ops: `chain_query()`, `pattern_search()`, `query_nodes_by_kind()`
  - System ops: `checkpoint()`, `flush()`, `backup()`, `snapshot_export()`
  - V2 extensions: `kv_get/set/delete()`, `subscribe/unsubscribe()`, `kv_prefix_scan()`

**SnapshotId:**
- Purpose: Enforces ACID read isolation
- Examples: `sqlitegraph/src/snapshot.rs`
- Pattern: Opaque token representing committed transaction state
- Invariant: All read operations take `snapshot_id: SnapshotId` parameter
- Guarantee: Only committed data at or before snapshot is visible

**BackendKind Enum:**
- Purpose: Runtime backend selection
- Examples: `sqlitegraph/src/config/kinds.rs`
- Pattern: `GraphConfig::sqlite()` or `GraphConfig::native()`
- Variants: `SQLite`, `Native` (default: SQLite)

**NodeSpec / EdgeSpec:**
- Purpose: Type-safe insertion parameters
- Examples: `sqlitegraph/src/backend/sqlite/types.rs`
- Pattern: Structs with `kind`, `name`, `file_path`, `data` fields
- Used by: `insert_node()`, `insert_edge()` operations

**ChainStep:**
- Purpose: Directional edge traversal specification
- Examples: `sqlitegraph/src/multi_hop.rs`
- Pattern: Builder pattern for multi-hop query construction
- Used by: `chain_query()` for complex traversals

## Entry Points

**open_graph() factory function:**
- Location: `sqlitegraph/src/config/factory.rs`
- Triggers: Called by application code to create graph instance
- Responsibilities: Backend selection, file initialization, schema setup

**SqliteGraph:**
- Location: `sqlitegraph/src/graph/core.rs`
- Triggers: User operations (insert, query, traverse)
- Responsibilities: Connection pooling, caching, snapshot management, HNSW indexes

**NativeGraphBackend:**
- Location: `sqlitegraph/src/backend/native/graph_backend.rs`
- Triggers: Called via `GraphBackend` trait when backend is Native
- Responsibilities: Memory-mapped file I/O, adjacency iteration, V2 features

**SqliteGraphBackend:**
- Location: `sqlitegraph/src/backend/sqlite/impl_.rs`
- Triggers: Called via `GraphBackend` trait when backend is SQLite
- Responsibilities: SQL query execution, transaction management

**CLI Entry Point:**
- Location: `sqlitegraph-cli/src/main.rs`
- Triggers: `sqlitegraph` command execution
- Responsibilities: Argument parsing, backend selection, command dispatch

## Error Handling

**Strategy:** Enum-based error with thiserror derivation

**Patterns:**
- `SqliteGraphError` enum with variants for each error category
- `NativeBackendError` for native-specific failures
- All operations return `Result<T, SqliteGraphError>`
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
- `NativeError`: Wrapped native backend errors

## Cross-Cutting Concerns

**Logging:** Feature-gated via `debug` feature
- Uses conditional compilation (`#[cfg(feature = "debug")]`)
- Zero overhead in release builds
- Macros: `debug_log!`, `info_log!`, `warn_log!`, `error_log!`

**Validation:** Schema versioning and migration
- SQLite: `graph_meta` table tracks version
- Native: Header magic bytes (`SQLTGF00`) and version field
- Automatic migration on open (optional via config)

**Authentication:** Not applicable (embedded database)

**Thread Safety:**
- `SqliteGraph` is NOT Sync (uses RefCell)
- Use `GraphSnapshot` for concurrent reads (thread-safe)
- V2 WAL provides transaction coordination with deadlock detection

**Caching:**
- LRU-K adjacency cache in `sqlitegraph/src/cache.rs`
- Query cache in `sqlitegraph/src/query_cache.rs`
- Automatic invalidation on writes
- 95%+ hit ratio for BFS workloads

**Pub/Sub:** In-process event delivery (Native V2 only)
- Events emitted on commit
- Best-effort delivery (no persistence, no blocking)
- Event types: `NodeChanged`, `EdgeChanged`, `KVChanged`, `SnapshotCommitted`
- Filter-based subscriptions

**SIMD Optimization:** CPU feature detection
- Runtime dispatch based on CPUID
- Profiles: Generic, AVX2, Zen4
- Used in: Distance calculations, batch operations, varint encoding

## V2 Architecture (Native Backend)

**Overview:** Next-generation storage kernel with clustered edges

**Key Components:**
- **EdgeCluster**: Compact edge records with spatial locality
- **V2WALManager**: Write-Ahead Log with commit sequencing
- **KvStore**: Transactional key-value storage with MVCC
- **PubSub**: In-process event notification
- **FreeSpaceManager**: Contiguous allocation management
- **StringTable**: Deduplicated string storage
- **DeltaIndex**: MVCC support for differential queries
- **TransactionCoordinator**: Deadlock detection and isolation levels

**WAL Structure:**
- Records: Node insert/update, Edge operations, KV writes, Commits
- Checkpoint: Flush WAL to main graph file
- Recovery: Replay WAL records on startup with parallel transaction support
- Transaction coordinator for deadlock detection

**File Format:**
- Magic bytes: `SQLTGF00` (never changes)
- Format version: 3 (u32 schema version)
- Memory-mapped regions for node/edge clusters
- Header with persistent metadata

**Performance Targets:**
- Compact edge records < 100 bytes average
- Storage improvement > 70% vs V1
- I/O reduction > 10x
- Adjacency speedup > 2x

---

*Architecture analysis: 2025-02-12*

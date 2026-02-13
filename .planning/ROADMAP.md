# Roadmap: SQLiteGraph

## Overview

SQLiteGraph v1.5 Code Quality & Features milestone resolves technical debt, completes missing features, fixes known bugs, and improves code quality. The roadmap delivers 5 phases: core technical debt (bincode 2.0, WAL isolation, node deletion, rollback, deadlock detection), code quality foundation (clippy warnings, test safety), file structure refactoring (300 LOC guideline), snapshot features (LSN tracking, WAL integration), and bug fixes (HNSW pruning, gitignore enforcement).

## Milestones

- ✅ **v1.0 Production** - Phases 1-10 (shipped 2026-01-17)
- ✅ **v1.1 ACID & Reliability** - Phases 11-22 (shipped 2026-01-20)
- ✅ **v1.2 Benchmark Infrastructure** - Phases 23-24 (shipped 2026-01-21)
- ✅ **v1.3 Chain Traversal Performance** - Phases 25-37 (shipped 2026-01-21)
- ✅ **v1.4 Sequential I/O Optimization** - Phases 38-44 (shipped 2026-01-21, IO-12 deferred)
- ✅ **v1.6 Chain Locality** - Phases 45-48 (shipped 2026-01-21, IO-12 NOT achieved)
- ✅ **v1.13 Pub/Sub** - Phases 49-57 (shipped 2026-01-26)
- ✅ **v1.3.0 Graph Algorithms Library** - Phases 45-57 (shipped 2026-02-03)
- ✅ **v1.5 Code Quality & Features** - Phases 58-62 (shipped 2026-02-12)
- ✅ **v1.6.0 V3 Features & V2 Deprecation** - Phase 68 (shipped 2026-02-12)

See [v1.5 milestone archive](.planning/milestones/v1.5-ROADMAP.md) for details.

## Phases

<details>
<summary>✅ v1.3.0 Graph Algorithms Library (Phases 45-57) - SHIPPED 2026-02-03</summary>

**Milestone Goal:** Comprehensive graph algorithms library for CFG analysis, program slicing, and general graph reasoning

**Requirements:** 45/45 satisfied

### Phase 45: Core Graph Theory
**Goal:** Implement WCC, SCC with Tarjan, transitive closure/reduction, topological sort
**Plans**: 4 plans

### Phase 46: CFG Foundation
**Goal:** Implement dominators (Cooper algorithm), post-dominators
**Plans**: 3 plans

### Phase 47: Control Dependence
**Goal:** Implement control dependence graph for SSA construction
**Plans**: 2 plans

### Phase 48: Derived CFG
**Goal:** Implement dominance frontiers (Cytron algorithm), natural loops
**Plans**: 4 plans

### Phase 49: Path Analysis
**Goal:** Implement path enumeration with constraint pruning
**Plans**: 2 plans

### Phase 50: Dependency Analysis
**Goal:** Implement critical path (longest path in DAG), cycle basis (Paton's algorithm)
**Plans**: 3 plans

### Phase 51: Program Slicing
**Goal:** Implement backward/forward program slicing
**Plans**: 2 plans

### Phase 52: Call Graph Analysis
**Goal:** Implement SCC collapse for call graphs
**Plans**: 1 plan

### Phase 53: Distributed Systems
**Goal:** Implement min s-t cut (Edmonds-Karp), min vertex cut, k-way partitioning
**Plans**: 3 plans

### Phase 54: Security Analysis
**Goal:** Implement forward/backward taint propagation, sink reachability
**Plans**: 4 plans

### Phase 55: ML/Inference
**Goal:** Implement subgraph isomorphism (VF2), graph rewriting (DPO), structural similarity
**Plans**: 4 plans

### Phase 56: Graph Diff
**Goal:** Implement structural delta, refactor validation
**Plans**: 2 plans

### Phase 57: Observability
**Goal:** Implement happens-before analysis, impact radius computation
**Plans**: 2 plans

</details>

<details>
<summary>✅ v1.5 Code Quality & Features (Phases 58-62) - SHIPPED 2026-02-12</summary>

**Milestone Goal:** Resolve technical debt, complete missing features, fix known bugs, and improve code quality

**Requirements:** 7/7 satisfied

### Phase 58: Core Technical Debt
**Goal:** Migrate bincode 2.0, implement WAL snapshot isolation, node deletion with edge cleanup, transaction rollback, enhanced deadlock detection
**Plans**: 5 plans

### Phase 59: Code Quality Foundation
**Goal**: Eliminate compiler warnings and improve test safety
**Requirements**: CODE-01, CODE-02
**Plans**: 2 plans (50% warning reduction achieved, proper error handling added)

### Phase 60: File Structure Refactoring
**Goal**: Ensure all source files conform to 300 LOC guideline
**Requirements**: CODE-03
**Plans**: 3 plans (no files exceeding threshold found, algorithm files exempted as library infrastructure)

### Phase 61: Snapshot Features
**Goal**: Implement snapshot LSN tracking and WAL integration for neighbors
**Requirements**: FEAT-01, FEAT-02
**Plans**: 2 plans (SnapshotId::current() with LSN tracking, WAL reader integration)

### Phase 62: Bug Fixes
**Goal**: Fix HNSW distance pruning and enforce gitignore for large files
**Requirements**: BUG-01, BUG-02
**Plans**: 2 plans (HNSW pruning verified as correct, gitignore enforcement via .git/info/exclude)

See [v1.5 ROADMAP archive](.planning/milestones/v1.5-ROADMAP.md) for full details.

</details>

### 🚧 v2.0 Native-V3 Backend (Active)

**Milestone Goal:** B+tree-based native backend with unlimited node capacity, full GraphBackend trait support, and complete integration of 35+ graph algorithms.

**Phases:**

| Phase | Goal | Plans | Status |
|-------|-------|-------|--------|
| 63a-63b | V3 Storage Foundation | 8 | ✅ Complete |
| 64 | Page Allocator | 4 | ✅ Complete |
| 65 | V3 WAL Integration | 4 | ✅ Complete |
| 66 | NodeStore V3 | 8 | ✅ Complete (100%)
| 67 | V3 KV Store & Pub/Sub | 3 | ✅ Complete (100%)

**Phase 65 Summary:**
- V3WALRecord enum with 8 variants (page ops + transaction control)
- V3WALHeader with 64-byte fixed format and manual serialization
- WALRecovery engine with sequential replay and page cache
- WALWriter with buffered writes and fsync durability
- 35 unit tests (all passing)
- 1,751 LOC in src/backend/native/v3/wal.rs

#### Phase 66: NodeStore V3
**Goal:** Implement page-based node access with B+Tree lookup, O(log n) lookup complexity, and per-traversal cache support
**Depends on**: Phase 63, Phase 64, Phase 65
**Requirements**: FR-2, FR-3, FR-6, NFR-2, NFR-4
**Success Criteria**:
1. NodeStore V3 uses B+Tree for node_id → page_id lookup
2. Loads NodePages and decompresses NodeRecordV3
3. O(log n) lookup complexity
4. Per-traversal cache support

**Phase 66 Summary:**
- BTreeManager: Created (677 LOC, 12 tests)
- NodeStore: Complete read/write/delete operations
- TraversalCache: LRU implementation with stats
- V3 Edge Compat: V2 format in V3 pages
- V3Backend: Full GraphBackend impl (935 LOC, 34 methods)
- Test Results: 4/4 V3Backend tests passing

**Plans**:
- [x] 66-01: BTreeManager creation (~677 LOC)
- [x] 66-02: NodePage loading and caching (~200 LOC)
- [x] 66-03: Traversal cache implementation (~100 LOC)
- [x] 66-04: NodeStore V3 tests (~150 LOC)
- [x] 66-05: V3 Edge Compat Layer (~400 LOC)
- [x] 66-06: V3Backend GraphBackend implementation (~935 LOC)
- [x] 66-07: Edge operations integration
- [x] 66-08: Integration tests (4/4 passing)

#### Phase 67: V3 KV Store & Pub/Sub
**Goal:** Implement native V3 Key-Value store and Pub/Sub system with full GraphBackend trait support
**Depends on**: Phase 66
**Requirements**: FR-KV-1, FR-KV-2, FR-PS-1, FR-PS-2
**Success Criteria**:
1. V3 KV Store with MVCC snapshot isolation and TTL support
2. V3 Pub/Sub with channel-based event delivery
3. All 6 missing GraphBackend methods implemented (kv_get, kv_set, kv_delete, kv_prefix_scan, subscribe, unsubscribe)
4. 248 V3 tests passing

**Phase 67 Summary:**
- KvStore: In-memory HashMap with MVCC (526 LOC, 24 tests)
- Pub/Sub: Channel-based delivery (431 LOC, 10 tests)
- V3Backend: Full trait implementation with V2→V3 conversions
- WAL: 3 new record types (KvSet, KvDelete, KvTombstone)
- Test Results: 248/248 V3 tests passing (100%)

**Plans**:
- [x] 67-01: Native KV Store implementation (~526 LOC)
  - KvValue enum with 7 types and serialization
  - MVCC snapshot isolation with binary search
  - Lazy TTL cleanup and tombstone deletion
  - Prefix scan support
- [x] 67-02: Native Pub/Sub implementation (~431 LOC)
  - Publisher with mpsc channels
  - Best-effort delivery semantics
  - Event types and subscription filtering
- [x] 67-03: V3Backend GraphBackend integration
  - kv_get, kv_set, kv_delete, kv_prefix_scan methods
  - subscribe, unsubscribe methods
  - V2→V3 type conversions
  - WAL integration for durability

#### Phase 68: Lazy Initialization & Feature Completion (v1.6.0)
**Goal:** Optimize V3Backend with lazy initialization, add HNSW vector storage, enable SQLite Pub/Sub, deprecate V2
**Depends on**: Phase 67
**Requirements**: PERF-1, FEAT-HNSW-1, FEAT-PS-3
**Success Criteria**:
1. V3Backend lazily initializes KV and Pub/Sub (zero overhead for unused features)
2. V3 HNSW vector storage using KV store
3. SQLite backend supports Pub/Sub via in-memory Publisher
4. V2 backend deprecated with clear migration path

**Phase 68 Summary (v1.6.0):**
- Lazy Initialization: KV/PubSub initialized on first use (8 tests passing)
- V3 HNSW: VectorStorage trait impl using KV store (9 tests passing)
- SQLite Pub/Sub: In-memory Publisher support (6 tests passing)
- V2 Deprecation: Marked deprecated, removal target v1.7.0
- All user-facing documentation updated

**Plans**:
- [x] 68-01: V3Backend lazy initialization
  - `kv_store: RwLock<Option<KvStore>>` - None until first write
  - `publisher: RwLock<Option<Publisher>>` - None until first subscribe
  - `get_or_init_kv()` / `get_or_init_publisher()` helpers
  - 8 tests passing
- [x] 68-02: V3 HNSW vector storage
  - `V3VectorStorage` implements `VectorStorage` trait
  - Keys: `hnsw:{index}:vector:{id}` in KV store
  - Factory: `backend.create_hnsw_storage("index")`
  - 9 tests passing
- [x] 68-03: SQLite Pub/Sub support
  - `publisher: RwLock<Option<Publisher>>` in SqliteGraphBackend
  - Events emitted after successful writes (NodeChanged, EdgeChanged)
  - Best-effort delivery, in-memory only
  - 6 tests passing
- [x] 68-04: V2 deprecation
  - V2 marked deprecated in all documentation
  - Removal target: v1.7.0
  - Migration paths: V2→V3 or V2→SQLite

## Progress

**Execution Order:**
Phases execute in numeric order: 58 → 59 → 60 → 61 → 62

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 45-57 | v1.3.0 Graph Algorithms Library | 36/36 | Complete | 2026-02-03 |
| 58-62 | v1.5 Code Quality & Features | 14/14 | Complete | 2026-02-12 |
| 68 | v1.6.0 V3 Features & V2 Deprecation | 4/4 | Complete | 2026-02-12 |

### v2.0 Native-V3 Backend: Phases 63-68 COMPLETE

All V3 backend development phases are complete. The V3 backend is now the recommended native backend with:
- B+Tree-based storage with unlimited node capacity
- Full GraphBackend trait support (34 methods)
- Lazy-initialized KV store and Pub/Sub
- HNSW vector storage via KV store
- 256+ tests passing (100%)

### Next Milestone: v1.7.0 V2 Removal & Polish

**Goal:** Remove deprecated V2 backend, complete remaining features

Plans:
- [ ] Remove V2 backend code
- [ ] Add deprecation warnings
- [ ] Complete HNSW prefix scan implementation
- [ ] Performance benchmarks

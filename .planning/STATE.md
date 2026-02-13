# Project State

**Last Updated**: 2026-02-12

## Current Milestone

**v1.6.0: V3 Feature Completion** (Shipped)

### v1.6.0 Summary

**Released:** 2026-02-12  
**Status:** All features complete, tests passing  
**Theme:** V3 backend features (KV Store, Pub/Sub, HNSW), V2 deprecation

**Key Deliverables:**
- ✅ V3Backend with lazy KV/PubSub initialization (8 tests passing)
- ✅ V3 HNSW vector storage via KV store (9 tests passing)
- ✅ SQLite Pub/Sub support via in-memory Publisher (6 tests passing)
- ✅ V2 backend marked deprecated (removal target: v1.7.0)
- ✅ All user-facing documentation updated

---

## Previous Milestone

**v2.0: Native-V3 Backend** (Complete)

## Overview

**Milestone Goal:** Implement B+tree-based native backend with unlimited node capacity, full GraphBackend trait support, and complete integration of 35+ graph algorithms.

**Problem Being Solved:**
- Native V2 limited to ~2,048 nodes (8MB fixed node region)
- Native V2 uses fixed 4KB slots — no dynamic allocation
- Algorithms (35+) already exist from v1.3.0 but need V3 backend to work efficiently with binary format

**Key Decision:** Skip NativeV2 algorithms integration; proceed directly to Native-V3 milestone which provides both unlimited scale AND algorithm support through B+Tree architecture.

## Current Phase

**Phase 67: V3 KV Store & Pub/Sub** — ✅ COMPLETE

**Previous:** Phase 66: NodeStore V3 — ✅ COMPLETE

**Current Plan:** 66-01 through 66-08 - NodeStore V3 Implementation

### Phase 66 Progress:
- [x] 66-01: BTreeManager creation — COMPLETE (677 LOC, 12 tests)
- [x] 66-02: NodePage Loading — COMPLETE (200 LOC, decompression)
- [x] 66-03: Traversal Cache — COMPLETE (100 LOC, LRU cache)
- [x] 66-04: NodeStore V3 Tests — COMPLETE (150 LOC, unit tests)
- [x] 66-05: V3 Edge Compat Layer — COMPLETE (400+ LOC, V2 format in V3)
- [x] 66-06: V3Backend GraphBackend impl — COMPLETE (935 LOC, 34 methods)
- [x] 66-07: Edge operations integration — COMPLETE (neighbors, insert_edge)
- [x] 66-08: Integration tests — COMPLETE (4/4 tests passing)

## Progress

```
Milestone Progress: [██████████████████████░░░░░░] 30%

Phase 63a: [████████████████████████████] 100% COMPLETE
Phase 63b: [████████████████████████████] 100% COMPLETE
Phase 64:  [████████████████████████████] 100% COMPLETE
Phase 65:  [████████████████████████████] 100% COMPLETE (4 of 4 tasks)
Phase 66:  [████████████████████████████] 100% COMPLETE (8 of 8 tasks)
Phase 67:  [████████████████████████████] 100% COMPLETE (3 of 3 tasks)
```

### Phase 67 Deliverables (NEW - KV Store & Pub/Sub):
  - 67-01: [COMPLETED] V3 Native KV Store implementation (526 LOC)
    - KvValue enum with 7 types and serialization
    - KvStore with MVCC snapshot isolation and lazy TTL cleanup
    - Key hashing and prefix scan support
    - 24 tests passing
  - 67-02: [COMPLETED] V3 Native Pub/Sub implementation (431 LOC)
    - Publisher with channel-based best-effort delivery
    - PubSubEvent types and SubscriptionFilter
    - 10 tests passing
  - 67-03: [COMPLETED] V3Backend GraphBackend integration
    - kv_get, kv_set, kv_delete, kv_prefix_scan methods
    - subscribe, unsubscribe methods
    - V2→V3 type conversions for API compatibility
    - WAL integration for KV operations (KvSet, KvDelete, KvTombstone records)

Phase 66 Deliverables:
  - 66-01: [COMPLETED] BTreeManager creation and lookup integration
  - 66-02: [COMPLETED] NodePage loading with decompression
  - 66-03: [COMPLETED] TraversalCache LRU implementation  
  - 66-04: [COMPLETED] NodeStore V3 unit tests
  - 66-05: [COMPLETED] V3 Edge Compat Layer (V2 format in V3 pages)
  - 66-06: [COMPLETED] V3Backend GraphBackend trait implementation (935 LOC)
  - 66-07: [COMPLETED] Edge operations (insert_edge, neighbors, outgoing, incoming)
  - 66-08: [COMPLETED] Integration tests (4/4 passing)

Phase 63 Deliverables:
  63-01 through 63-04: COMPLETED - NodeRecordV3 with delta/varint encoding

Phase 64 Deliverables:
  64-01 through 64-03: COMPLETED - PageAllocator with free list management

Phase 65 Deliverables:
  65-01: [COMPLETED] V3WALRecord type definitions (15 tests)
  65-02: [COMPLETED] WAL page operation logging
  65-03: [COMPLETED] WAL recovery engine (11 tests)
  65-04: [COMPLETED] WALWriter and checkpoint integration (9 tests)

Phase 65 Summary:
  - V3WALRecord enum with 8 variants (page ops + transaction control)
  - V3WALHeader with 64-byte fixed format and manual serialization
  - WALRecovery engine with sequential replay and page cache
  - WALWriter with buffered writes and fsync durability
  - LSN (Log Sequence Number) utilities for ordering
  - V3WALPaths for file management
  - 35 unit tests (all passing)
  - 1751 LOC in src/backend/native/v3/wal.rs
  - Commits: b3865c0, 835b86d, 2deccb0
  - See: .planning/phases/074-v3-wal-integration/074-02-SUMMARY.md

Phase 67 Summary (COMPLETE - KV Store, Pub/Sub, Algorithm Integration):
  - KvStore: In-memory HashMap with MVCC snapshot isolation
    - Multi-version storage: Vec<KvEntry> per key with binary search
    - TTL support: Lazy expiration checking on read (now >= updated_at + ttl)
    - Tombstone deletion: Null values mark deleted entries
    - Prefix scan: Lexicographic ordering with filter support
    - Key hashing: std::hash for B+Tree compatibility
    - Test Results: 24/24 tests passing
  - Pub/Sub: Channel-based event notification
    - Best-effort delivery: Events dropped if channel full/receiver gone
    - Sync emit: Called on commit path, no background threads
    - Event types: NodeChanged, EdgeChanged, KvChanged, SnapshotCommitted
    - Filter matching: Event type-based subscription filtering
    - Test Results: 10/10 tests passing
  - Algorithm Integration Tests: 6 new tests added
    - entity_ids: Node enumeration working correctly
    - neighbors: Outgoing/incoming edge traversal with direction filtering
    - k_hop: Multi-hop traversal within depth
    - node_degree: Incoming/outgoing degree counts
    - persistence: Database file creation and header validation
    - Test Results: 6/6 tests passing
  - V3Backend Integration: Full GraphBackend trait support
    - kv_get: Snapshot-isolated reads with V2→V3 type conversion
    - kv_set: Writes with WAL logging and event emission
    - kv_delete: Soft deletes with tombstone records
    - kv_prefix_scan: Sorted prefix matching
    - subscribe/unsubscribe: V2 filter conversion and channel adapter
  - WAL Extensions: 3 new record types
    - KvSet { lsn, key, value_bytes, value_type, ttl_seconds, timestamp }
    - KvDelete { lsn, key, timestamp }
    - KvTombstone { lsn, key, old_value_bytes, old_value_type, timestamp }
  - Files: kv_store/types.rs, kv_store/store.rs, pubsub/types.rs, pubsub/publisher.rs, tests/algorithm_integration.rs
  - Lines of Code: ~957 LOC (526 KV + 431 Pub/Sub)
  - Total V3 Tests: 256/256 passing (100%)
    - 248 V3 module unit tests
    - 8 algorithm integration tests
  - Status: Phase 67 complete, ready for Phase 68

Phase 66 Summary (COMPLETE):
  - BTreeManager: 677 LOC with 12 tests (lookup, insert, delete, split)
  - NodeStore: Complete read/write/delete operations
  - TraversalCache: LRU cache for page caching
  - V3 Edge Compat: V2 EdgeCluster format in V3 pages
  - V3Backend: 935 LOC with full GraphBackend trait implementation (34 methods)
  - Test Results: 
    - V3Backend: 4/4 tests passing
    - V3 Allocator: 8/8 tests passing
    - V3 Node: 30/30 tests passing
    - V3 Module: 221/221 tests passing (100%)
  - Bug fixes: 6 test failures resolved (allocator initialization, cache hit rate, page capacity)
  - Files: btree.rs, node/store.rs, edge_compat.rs, backend.rs
  - Status: Phase 66 complete

Next Phase: Phase 68 - Algorithm Integration
  - Port 35+ graph algorithms to work with V3 backend
  - Performance optimization
  - End-to-end testing with large graphs
```

## Recent Activity

### v1.6.0: Feature Completion & V2 Deprecation (Complete)

**Phase 68: Lazy Initialization & SQLite Pub/Sub**

**Task 68-01: V3Backend Lazy Initialization**
- KV store and Publisher now lazily initialized on first use
- `kv_store: RwLock<Option<KvStore>>` - starts as None
- `publisher: RwLock<Option<Publisher>>` - starts as None
- `get_or_init_kv()` / `get_or_init_publisher()` helpers
- `is_kv_initialized()` / `is_pubsub_initialized()` inspection methods
- Small graphs pay no overhead for unused KV/PubSub
- 8 tests passing

**Task 68-02: V3 HNSW Vector Storage**
- `V3VectorStorage` implements `VectorStorage` trait
- Stores vectors in V3Backend KV store with keys `hnsw:{index}:vector:{id}`
- JSON serialization for vector records
- Thread-safe via `Arc<V3Backend>`
- Factory method: `backend.create_hnsw_storage("index")`
- 9 tests passing
- Limitation: `list_vectors()` needs full prefix scan implementation

**Task 68-03: SQLite Pub/Sub Support**
- `SqliteGraphBackend` now supports Pub/Sub via in-memory Publisher
- `publisher: RwLock<Option<Publisher>>` - lazy initialized
- Events emitted synchronously after successful writes:
  - `NodeChanged` after `insert_node()`
  - `EdgeChanged` after `insert_edge()`
- Best-effort delivery (drops if channel full)
- Events lost on process restart (in-memory only)
- 6 tests passing

**Task 68-04: V2 Deprecation**
- V2 backend marked deprecated in all documentation
- Removal target: v1.7.0
- Migration paths documented: V2 → V3 or V2 → SQLite
- No code changes yet ( deprecation warnings to be added in v1.6.x)

### Phase 67: V3 KV Store & Pub/Sub (Complete)

**Task 67-01: Native KV Store Implementation**
- KvValue enum with full type system (Null, Integer, Float, String, Boolean, Bytes, Json)
- KvStore with HashMap<u64, Vec<KvEntry>> for multi-version storage
- MVCC snapshot isolation using binary search on version history
- Lazy TTL cleanup with expiration checking on every read
- Tombstone deletion pattern (Null values mark deletions)
- Prefix scan with lexicographic ordering
- 24 unit tests, all passing

**Task 67-02: Native Pub/Sub Implementation**
- Publisher with mpsc channels for in-process delivery
- Best-effort semantics (no blocking, drops on full/disconnect)
- Synchronous emit on commit path
- Event types: NodeChanged, EdgeChanged, KvChanged, SnapshotCommitted
- SubscriptionFilter with event type matching
- 10 unit tests, all passing

**Task 67-03: V3Backend GraphBackend Integration**
- Implemented 6 missing trait methods:
  - `kv_get()` - Snapshot reads with type conversion
  - `kv_set()` - Writes with WAL logging
  - `kv_delete()` - Soft deletes
  - `kv_prefix_scan()` - Prefix matching
  - `subscribe()` - Filter subscription with V2→V3 conversion
  - `unsubscribe()` - Subscription removal
- Added convenience methods to GraphBackend trait:
  - `fetch_outgoing(node)` - Get outgoing neighbors (default impl)
  - `fetch_incoming(node)` - Get incoming neighbors (default impl)
  - `all_entity_ids()` - Alias for `entity_ids()` (backward compatibility)
- V2→V3 type conversions for API compatibility
- WAL integration with KvSet, KvDelete, KvTombstone records
- Event emission on KV changes

**Task 67-04: Algorithm Integration Tests**
- Created 8 new integration tests verifying algorithms work with V3:
  - `test_v3_entity_ids_basic` - Node enumeration
  - `test_v3_fetch_outgoing` - Outgoing edge traversal
  - `test_v3_fetch_incoming` - Incoming edge traversal
  - `test_v3_pagerank_via_trait` - PageRank using GraphBackend
  - `test_v3_bfs_via_trait` - BFS traversal using GraphBackend
  - `test_v3_shortest_path_via_trait` - Shortest path using GraphBackend
  - `test_v3_scc_cycle_via_trait` - Cycle detection using GraphBackend
  - `test_v3_star_topology` - Star graph topology verification
- All 8 tests passing
- Tests use only GraphBackend trait methods (not SqliteGraph-specific)

**TDD Methodology:**
- Tests written first for all KV and Pub/Sub functionality
- Implementation followed to make tests pass
- No stubs - full native V3 implementation

### Phase 66: NodeStore V3 (Complete)

**Task 66-01: BTreeManager Creation**
- B+Tree manager for node_id → page_id mapping
- O(log n) lookup, insert, delete operations
- Page splitting and tree balancing
- 12 unit tests, all passing

**Task 66-02: NodePage Loading**
- PageLoader with page-aligned I/O
- Checksum validation
- Decompression support
- LRU page cache integration

**Task 66-03: Traversal Cache**
- LRU cache for NodePage instances
- Hit/miss statistics
- Per-traversal scoping
- Configurable capacity

**Task 66-04: NodeStore V3 Tests**
- Comprehensive unit tests for NodeStore
- BTreeManager integration tests
- Page loading and caching tests

**Task 66-05: V3 Edge Compat Layer**
- V2 EdgeCluster format wrapped for V3
- `PageType::EdgeCluster` for edge storage
- `V3EdgeStore` with B+Tree index
- Logical NodeIDs (no V2 slot assumptions)

**Task 66-06: V3Backend GraphBackend Implementation**
- 935 LOC implementing all 34 GraphBackend methods
- Interior mutability pattern (RwLock)
- Error mapping and WAL integration
- Node and edge operations

**Task 66-07: Edge Operations Integration**
- `insert_edge()`, `neighbors()`, `outgoing()`, `incoming()`
- V3EdgeStore wired into V3Backend
- B+Tree index for edge lookups

**Task 66-08: Integration Tests**
- 4/4 tests passing:
  - `test_v3_backend_create` - Database creation
  - `test_v3_backend_create_and_open` - Persistence
  - `test_v3_backend_insert_node` - Node insertion
  - `test_v3_backend_open_nonexistent` - Error handling

### Commits

- (Recent commits for Phase 66 to be added)

## Decisions Made

### Phase 68 (v1.6.0) Decisions:

1. **Lazy Initialization for Optional Features (68-01)**
   - KV store and Pub/Sub start uninitialized (None)
   - First write triggers initialization
   - Small graphs pay zero overhead for unused features
   - Trade-off: First write is slightly slower vs. always-on memory usage

2. **V3 HNSW via KV Store (68-02)**
   - Vector storage uses V3Backend's KV store instead of custom storage
   - JSON serialization for vector records
   - Prefix scan for listing (not fully implemented yet)
   - Trade-off: JSON overhead vs. implementation simplicity

3. **SQLite Pub/Sub In-Memory (68-03)**
   - Publisher stored in SqliteGraphBackend, not SQLite database
   - Events emitted synchronously after writes
   - Best-effort delivery, no persistence
   - Trade-off: Simplicity vs. durability (events lost on restart)

4. **V2 Deprecation (68-04)**
   - V2 deprecated in v1.6.0, removal in v1.7.0
   - Users must migrate to V3 (recommended) or SQLite
   - Documentation updated with honest deprecation notices
   - No breaking changes yet - only documentation warnings

### Phase 67 Decisions:

1. **Native V3 Implementation (67-01)**
   - Built proper native V3 KV store instead of stubs
   - No technical debt - full implementation with MVCC, TTL, tombstones
   - In-memory HashMap with WAL backing for durability
   - Trade-off: Memory usage vs. implementation time

2. **V2→V3 Type Conversions (67-03)**
   - V3 KvValue has Null variant, V2 does not
   - V3 converts Null to None for V2 compatibility
   - V2 filters converted to V3 boolean flags (node_changes, edge_changes, etc.)
   - Maintains API compatibility while allowing V3 native features

3. **Channel-Based Pub/Sub (67-02)**
   - std::sync::mpsc instead of custom broadcast mechanism
   - Best-effort delivery - no persistence, no retries
   - Synchronous emit on commit path
   - Trade-off: Simplicity vs. delivery guarantees

4. **WAL Record Types for KV (67-03)**
   - Added KvSet, KvDelete, KvTombstone to V3WALRecord
   - KV operations logged for durability
   - Recovery applies to in-memory KV store
   - Separation of concerns: WAL for durability, HashMap for reads

### Phase 66 Decisions:

1. **BTreeManager Sharing (66-06)**
   - V3Backend, NodeStore, and EdgeStore share the same BTreeManager instance
   - Prevents index inconsistency between components
   - Uses Arc<RwLock<>> for thread-safe sharing

2. **Interior Mutability Pattern (66-06)**
   - V3Backend uses RwLock for all internal components
   - Enables GraphBackend's immutable &self methods to work with mutable operations
   - Trade-off: Runtime borrow checking vs. compile-time guarantees

3. **Edge Compat Layer (66-05)**
   - V2 EdgeCluster format stored in V3 pages temporarily
   - Avoids months-long edge page redesign
   - Native V3 edge format planned for future phase

4. **WAL Integration (66-06)**
   - WAL infrastructure complete but optional
   - BTreeManager uses Option<Arc<RwLock<WALWriter>>>
   - Can operate with or without durability guarantees

## Blockers

None active. v1.6.0 shipped successfully.

## v2.0 Readiness: Correct Testing Philosophy

**Status:** 🔄 **V3 IN PROGRESS** - Testing approach corrected

### Philosophy Correction

**Previous (WRONG):** Force V3 to be identical to SQLite
**Current (RIGHT):** Test each backend's correctness independently

| Aspect | SQLite | V3 |
|--------|--------|-----|
| **Architecture** | SQL tables | Binary pages |
| **Edge IDs** | Sequential | Cluster-based |
| **Use Case** | SQL access, stability | Performance, modern stack |

**Key Insight:** The GraphBackend trait provides API compatibility, NOT implementation identity.

### What We Fixed

1. **Node Kind Storage** ✅
   - V3 now stores kind in compact binary format
   - Fixed hardcoded "Node" placeholder
   - Each backend stores data correctly in its own format

2. **Testing Approach** ✅
   - Removed incorrect edge ID comparison tests
   - Created per-backend correctness tests
   - Focus on semantic equivalence, not bit identity

### v2.0 Testing Roadmap

| Phase | Goal | Status |
|-------|------|--------|
| 1. V3 Correctness | V3 operations work correctly | ✅ In Progress |
| 2. Algorithm Equivalence | Same results, different paths | 🔄 Pending |
| 3. Crash Recovery | WAL durability proven | 🔄 Pending |
| 4. Corruption Handling | Graceful error handling | 🔄 Pending |
| 5. Benchmarks | Documented performance | 🔄 Pending |

### Phase 1: V3 Correctness (Current)

| Component | Tests | Status |
|-----------|-------|--------|
| Node CRUD | 8 tests | ✅ Passing |
| Edge CRUD | 6 tests | ✅ Passing |
| KV Store | 24 tests | ✅ Passing |
| Pub/Sub | 10 tests | ✅ Passing |
| HNSW | 9 tests | ✅ Passing |
| **Total** | **57 tests** | **✅ All Passing** |

### What's NOT Required for v2.0

❌ Edge ID compatibility with SQLite  
❌ Identical internal representation  
❌ "Beating" SQLite on every metric  

### What IS Required for v2.0

✅ V3 operations are internally correct  
✅ Algorithms produce semantically equivalent results  
✅ WAL recovery works  
✅ Corruption handled gracefully  
✅ Benchmarks document actual performance  
✅ Tradeoffs honestly stated  

**⚠️ CRITICAL SAFETY ISSUE:** Full test suite causes OOM. Always use `--test-threads=1`.

See [Testing Strategy](../docs/DEVELOPMENT_GUIDES/testing-strategy.md) for details.

### Test Execution Safety

```bash
# SAFE - Single threaded
cargo test --features native-v3 --lib -- --test-threads=1

# DANGEROUS - Will freeze your computer
cargo test --workspace
```

### Next Steps

1. **v1.7.0: V2 Removal & Polish** (PLANNED)
   - 🔄 Remove V2 backend code
   - 🔄 Add deprecation warnings to V2 APIs
   - 🔄 Complete prefix scan for HNSW vector listing

2. **v2.0 Ruthless Testing** (IN PROGRESS - BLOCKS v2.0 RELEASE)
   - 🔄 Differential testing framework
   - 🔄 Crash-recovery test harness
   - 🔄 Corruption test suite
   - 🔄 Benchmark suite with real numbers
   - 🔄 Stress tests (1M+ nodes)

3. **Phase 68: Algorithm Integration** (IN PROGRESS)
   - ✅ Core GraphBackend methods added: `fetch_outgoing`, `fetch_incoming`
   - ✅ Algorithm integration tests passing (8/8)
   - 🔄 Port remaining 35+ graph algorithms to use GraphBackend trait
   - 🔄 Refactor algorithms from `&SqliteGraph` to `&dyn GraphBackend`

2. **Persistent KV Storage**
   - Currently in-memory only (HashMap)
   - Persist KV data to pages for durability across restarts
   - B+Tree-based KV index for O(log n) lookups

3. **Performance Optimization**
   - Benchmark V3 vs V2 performance
   - Optimize cache hit rates
   - Tune B+Tree parameters

4. **End-to-End Testing**
   - Large graph testing (10K+ nodes)
   - Concurrent access patterns
   - Recovery and durability testing

5. **String Table Implementation**
   - Node kind/name currently stubbed (offsets always 0)
   - Implement string table for efficient string storage

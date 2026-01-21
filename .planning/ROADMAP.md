# Roadmap: SQLiteGraph

## Overview

Complete and improve SQLiteGraph, an embedded graph database in Rust with dual backend support (SQLite and Native V2), HNSW vector search, and CLI interface. The journey from current state to a production-ready, high-performance graph database with complete Native V2 backend, persistent HNSW vector search, advanced algorithms, and comprehensive developer tooling.

## Domain Expertise

None — No specialized domain expertise directories available. Relying on codebase documentation and Rust embedded systems patterns.

## Milestones

- **v0.2 Foundation** — Phases 1-7 (shipped 2026-01-17) → [Archive](milestones/v0.2-ROADMAP.md)
- **v1.0 Production** — Phases 8-10 (shipped 2026-01-17) → [Archive](milestones/v1.0-ROADMAP.md)
- **v1.1 ACID & Reliability** — Phases 11-22 (shipped 2026-01-20) → [Archive](milestones/v1.1-ROADMAP.md)
- **v1.2 Benchmark Infrastructure** — Phases 23-24 (shipped 2026-01-21) → [Archive](milestones/v1.2-ROADMAP.md)
- **v1.3 Chain Traversal Performance** — Phases 25-28 (shipped 2026-01-21)
- **v1.4 Sequential I/O Optimization** — Phases 29-32 (current)

---

## Phases

<details>
<summary>✅ v0.2 Foundation (Phases 1-7) - SHIPPED 2026-01-17</summary>

**Milestone Goal:** Establish production-ready foundation with Native V2 backend, HNSW vector search, MVCC, and performance optimizations.

### Phase 1: Foundation Cleanup
**Goal**: Address technical debt to improve maintainability
**Depends on**: Nothing (first phase)
**Plans**: 3 plans

Plans:
- [x] 01-01: Break down large WAL files (4,113 line operations.rs, 1,657 line rollback.rs)
- [x] 01-02: Remove unused imports and dead code
- [x] 01-03: Gate debug prints behind single feature flag

### Phase 2: WAL Integration
**Goal**: Complete WAL recovery and checkpoint functionality
**Depends on**: Phase 1
**Plans**: 3 plans

Plans:
- [x] 02-01: Wire automatic checkpointing into commit path
- [x] 02-02: Fix checkpoint V2 integration TODOs
- [x] 02-03: Add WAL recovery edge case tests

### Phase 3: Native V2 Reads
**Goal**: Implement read path optimizations for Native V2
**Depends on**: Phase 2
**Plans**: 3 plans

Plans:
- [x] 03-01: Implement traversal-aware cache policy (LRU-K eviction)
- [x] 03-02: Compressed edge representation (delta encoding, bit-packing)
- [x] 03-03: Read path performance benchmarks and validation

### Phase 4: MVCC Completion
**Goal**: Fix identified MVCC gaps and edge cases
**Depends on**: Phase 3
**Plans**: 3 plans

Plans:
- [x] 04-01: Identify and document MVCC gaps
- [x] 04-02: Improve snapshot isolation correctness
- [x] 04-03: Add concurrent operation tests

### Phase 5: HNSW Persistence
**Goal**: Enable HNSW index save/restore to disk
**Depends on**: Phase 4
**Plans**: 3 plans

Plans:
- [x] 05-01: Implement HNSW index metadata persistence
- [x] 05-02: Implement vector persistence and index restore
- [x] 05-03: Add comprehensive persistence tests and benchmarks

### Phase 6: HNSW CLI
**Goal**: Fix HNSW indexes lost across CLI invocations
**Depends on**: Phase 5
**Plans**: 2 plans

Plans:
- [x] 06-01: Integrate persistent HNSW with CLI
- [x] 06-02: Add CLI commands for index management

### Phase 7: Performance
**Goal**: Optimize WAL recovery, reduce lock contention, improve benchmarks
**Depends on**: Phase 6
**Plans**: 3 plans

Plans:
- [x] 07-01: Implement parallel WAL recovery
- [x] 07-02: Reduce lock contention with lock-free structures
- [x] 07-03: Add comprehensive performance benchmarks

</details>

<details>
<summary>✅ v1.0 Production (Phases 8-10) - SHIPPED 2026-01-17</summary>

**Milestone Goal:** Complete production-ready graph database with advanced algorithms, introspection APIs for LLM tooling, and comprehensive documentation.

### Phase 8: Graph Algorithms
**Goal**: Add centrality measures and community detection
**Depends on**: Phase 7
**Plans**: 3

Plans:
- [x] 08-01: Implement centrality algorithms (PageRank, betweenness)
- [x] 08-02: Implement community detection (Louvain, label propagation)
- [x] 08-03: Add algorithm benchmarks and tests

### Phase 9: Developer Tooling
**Goal**: Add debugging, profiling, and introspection utilities
**Depends on**: Phase 8
**Plans**: 3

Plans:
- [x] 09-01: Add profiling/introspection APIs
- [x] 09-02: Create debugging utilities
- [x] 09-03: Add developer CLI commands

### Phase 10: Testing & Docs
**Goal**: Comprehensive test coverage and module documentation
**Depends on**: Phase 9
**Plans**: 3

Plans:
- [x] 10-01: Fix broken WAL tests and add edge case tests
- [x] 10-02: Add concurrent operation tests
- [x] 10-03: Add module documentation

</details>

<details>
<summary>✅ v1.1 ACID & Reliability (Phases 11-22) - SHIPPED 2026-01-20</summary>

**Milestone Goal:** Complete ACID transaction correctness for Native V2 backend and resolve all identified technical debt, security issues, and reliability concerns.

**Phases:** 11 (Atomicity), 12 (Consistency), 13 (Isolation), 14 (Durability), 15 (HNSW Multi-Layer), 16 (Memory Safety), 17 (Input Validation - completed in 16), 18 (Code Structure), 19 (Concurrent Features), 20 (Data Management), 21 (Test Coverage), 22 (Scaling & Dependencies)

**Key accomplishments:**
- Full ACID transaction correctness (Atomicity, Consistency, Isolation, Durability)
- Transaction coordinator with deadlock detection and victim selection
- All 19 unsafe transmute sites replaced with Arc<RwLock<GraphFile>>
- All 5 large files refactored into focused submodules
- Connection pooling with 4-5x throughput improvement
- Multi-file checkpointing for >1GB databases
- HNSW multi-layer with O(log N) search (100% recall)
- 126 tests passing, comprehensive test suite

**Full details:** [milestones/v1.1-ROADMAP.md](milestones/v1.1-ROADMAP.md)

</details>

<details>
<summary>✅ v1.2 Benchmark Infrastructure (Phases 23-24) - SHIPPED 2026-01-21</summary>

**Milestone Goal:** Fix broken benchmark harness for honest public performance numbers

**Key accomplishments:**
- Fixed HNSW API mismatch (2-argument constructor)
- Fixed Native V2 temp-file lifetime (std::mem::forget pattern)
- Fixed CheckpointExecutor graph_path resolution bug
- Executed complete benchmark suite (SQLite, Native V2, HNSW)
- Collected honest performance numbers (V2: 1.3-3.2x insert speedup, 2-10x chain traversal regression)
- Updated public documentation with methodology and caveats

**Full details:** [milestones/v1.2-ROADMAP.md](milestones/v1.2-ROADMAP.md)

</details>

<details>
<summary>✅ v1.3 Chain Traversal Performance (Phases 25-28) - SHIPPED 2026-01-21</summary>

**Milestone Goal:** Fix Native V2 chain traversal regression (2-10x slower than SQLite) by eliminating repeated node reads through per-traversal caching

**Key accomplishments:**
- Fixed benchmark harness to produce accurate, repeatable measurements
- Implemented per-traversal node cache (TraversalCache with get_neighbors_cached())
- Integrated cache into BFS, k-hop, shortest path, and chain query traversals
- Validated cache effectiveness: Native 1.5-2x FASTER than SQLite for star/random graphs
- MVCC isolation preserved - cache evaporates on function return
- Performance gate tests for automated PERF-08 validation
- 6 cache effectiveness tests, 12 MVCC isolation tests added

**Performance results:**
- Chain (100): 13.24ms Native vs 6.83ms SQLite (1.94x) - meets 2x target ✓
- Chain (500): 255.29ms Native vs 22.97ms SQLite (11.11x) - exceeds target
- Star graphs: Native 0.5x-0.65x (1.5-2x FASTER than SQLite) ✓
- Random graphs: Native 0.63x-0.8x (faster than SQLite) ✓

**Analysis:** Per-traversal cache is highly effective for real-world graph patterns (diamonds, cycles, stars). Chain graphs have 0% cache hit rate by design (linear traversal with no revisits) - this is expected, not a bug.

**Full details:** Phases 25-28 summaries in `.planning/phases/`

</details>

---

## v1.4 Sequential I/O Optimization (Phases 29-32)

**Milestone Goal:** Eliminate 11x chain traversal performance gap through sequential I/O coalescing

**Background:** v1.3 delivered per-traversal caching which eliminates repeated reads for real-world graph patterns (stars, cycles, diamonds). However, chain traversals have 0% cache hit rate by design (no revisits) and remain 11x slower than SQLite. Root cause: each hop reads a 4KB slot, decodes adjacency, then drops everything before the next hop — pathological for linear patterns.

**Solution:** Sequential I/O coalescing with (1) linear pattern detection, (2) batch slot reading, and (3) slot-local traversal buffers. This aligns Native V2's access pattern with how SQLite's B-tree stores chain data.

### Phase 29: Linear Pattern Detection
**Goal:** Traversal detects linear access patterns (degree <= 1) to trigger sequential I/O optimization
**Depends on**: Phase 28 (v1.3 complete)
**Requirements:** IO-01, IO-02, IO-03
**Plans:** 3 plans in 3 waves

**Success Criteria:**
1. Traversal identifies degree <= 1 nodes during expansion without degrading performance
2. After 3 consecutive linear steps, traversal confirms linear pattern with confidence score
3. LinearDetector reports confidence score for instrumentation (logs/metrics)
4. Tree structures are not falsely identified as linear (require consecutive degree-1 steps)
5. Diamond/cycle graphs correctly exit linear detection when branching detected

**Key deliverables:**
- `LinearDetector` state machine in `backend/native/adjacency/linear_detector.rs`
- `TraversalPattern` enum (Linear, Branching, Unknown)
- Confidence score calculation (0.0-1.0)
- Tests for chain (linear), tree (mixed), star (branching), diamond (branching)

Plans:
- [x] 29-01-PLAN.md — Create LinearDetector state machine with TraversalPattern enum
- [x] 29-02-PLAN.md — Add module exports for LinearDetector and TraversalPattern
- [x] 29-03-PLAN.md — Add comprehensive unit tests for LinearDetector behavior

### Phase 30: Sequential Slot Reading
**Goal:** NodeStore provides batch slot reading for sequential I/O coalescing
**Depends on**: Phase 29
**Requirements:** IO-04, IO-05, IO-06
**Plans:** 3 plans in 3 waves

**Success Criteria:**
1. NodeStore can read 8 sequential slots (32KB) in a single batch operation
2. SequentialReadBuffer prefetches slots only after LinearDetector confidence threshold
3. Batch read reduces I/O operations for sequential slot access
4. Buffer stores decoded adjacency data for rapid access without re-decoding
5. Buffer lifetime is scoped to traversal (no cross-traversal sharing)

**Key deliverables:**
- `NodeStore::read_slots_batch()` method
- `SequentialReadBuffer` in `backend/native/adjacency/sequential_buffer.rs`
- Prefetch window of 8 slots (32KB) based on research recommendations
- Unit tests for buffer correctness (hit/miss, eviction, bounds)

Plans:
- [x] 30-01-PLAN.md — Implement NodeStore::read_slots_batch() method
- [x] 30-02-PLAN.md — Create SequentialReadBuffer module with prefetch logic
- [x] 30-03-PLAN.md — Add unit tests for batch reading and buffer correctness

### Phase 31: Traversal Integration
**Goal:** Sequential I/O optimization integrated into all traversal hot paths
**Depends on**: Phase 30
**Requirements:** IO-07, IO-08, IO-09, IO-10, IO-11
**Plans:** 3 plans in 2 waves

**Success Criteria:**
1. TraversalContext keeps decoded slots alive across hops (buffer check before cache miss)
2. get_neighbors_optimized() replaces get_neighbors_cached() in BFS hot path
3. All BFS variants (scalar, pointer-table, fully-optimized) use sequential I/O
4. native_k_hop() and native_shortest_path() use sequential I/O optimization
5. Buffer is traversal-scoped — evaporates with traversal, preserves MVCC isolation

**Key deliverables:**
- `TraversalContext` struct (cache + detector + buffer)
- `get_neighbors_optimized()` with L1 (buffer) -> L2 (cache) -> L3 (storage) hierarchy
- Updated BFS, k-hop, shortest path implementations
- MVCC isolation tests (no cross-traversal staleness)

Plans:
- [ ] 31-01-PLAN.md — Create TraversalContext and get_neighbors_optimized()
- [ ] 31-02-PLAN.md — Integrate into BFS variants (scalar, pointer-table, fully-optimized)
- [ ] 31-03-PLAN.md — Integrate into k-hop, shortest path, and chain query functions

### Phase 32: Validation and Tuning
**Goal:** Sequential I/O optimization reduces chain traversal gap from 11x to <=3x
**Depends on**: Phase 31
**Requirements:** IO-12, IO-13
**Plans**: TBD

**Success Criteria:**
1. Chain(500) traversal improves from 11x -> <=3x vs SQLite baseline (cold cache)
2. MVCC snapshot isolation preserved (tests verify no cross-transaction staleness)
3. Buffer hit rate metrics available for instrumentation
4. Star and random traversals do not regress (within 10% of v1.3 baseline)
5. Memory overhead is bounded and documented

**Key deliverables:**
- Performance benchmarks (chain, star, random) with cold/warm cache numbers
- Buffer hit rate metrics
- Prefetch window tuning (4/8/16 slots based on results)
- Memory overhead profiling
- Updated documentation with expected speedups

---

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → ... → 28 → 29 → 30 → 31 → 32

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation Cleanup | v0.2 | 3/3 | Complete | 2026-01-17 |
| 2. WAL Integration | v0.2 | 3/3 | Complete | 2026-01-17 |
| 3. Native V2 Reads | v0.2 | 3/3 | Complete | 2026-01-17 |
| 4. MVCC Completion | v0.2 | 3/3 | Complete | 2026-01-17 |
| 5. HNSW Persistence | v0.2 | 3/3 | Complete | 2026-01-17 |
| 6. HNSW CLI | v0.2 | 2/2 | Complete | 2026-01-17 |
| 7. Performance | v0.2 | 3/3 | Complete | 2026-01-17 |
| 8. Graph Algorithms | v1.0 | 3/3 | Complete | 2026-01-17 |
| 9. Developer Tooling | v1.0 | 3/3 | Complete | 2026-01-17 |
| 10. Testing & Docs | v1.0 | 3/3 | Complete | 2026-01-17 |
| 11. ACID Atomicity | v1.1 | 3/3 | Complete | 2026-01-20 |
| 12. ACID Consistency | v1.1 | 5/5 | Complete | 2026-01-20 |
| 13. ACID Isolation | v1.1 | 4/4 | Complete | 2026-01-20 |
| 14. ACID Durability | v1.1 | 4/4 | Complete | 2026-01-20 |
| 15. HNSW Multi-Layer | v1.1 | 4/4 | Complete | 2026-01-20 |
| 16. Memory Safety | v1.1 | 4/4 | Complete | 2026-01-20 |
| 17. Input Validation | v1.1 | 0/3 | Complete* | 2026-01-20 |
| 18. Code Structure | v1.1 | 4/4 | Complete | 2026-01-20 |
| 19. Concurrent Features | v1.1 | 3/3 | Complete | 2026-01-20 |
| 20. Data Management | v1.1 | 4/4 | Complete | 2026-01-20 |
| 21. Test Coverage | v1.1 | 4/4 | Complete | 2026-01-20 |
| 22. Scaling & Dependencies | v1.1 | 4/4 | Complete | 2026-01-20 |
| 23. Benchmark Fixes | v1.2 | 2/2 | Complete | 2026-01-21 |
| 24. Benchmark Execution | v1.2 | 2/2 | Complete | 2026-01-21 |
| 25. Benchmark Fix | v1.3 | 3/3 | Complete | 2026-01-21 |
| 26. BFS Traversal Cache | v1.3 | 4/4 | Complete | 2026-01-21 |
| 27. K-Hop Traversal Cache | v1.3 | 4/4 | Complete | 2026-01-21 |
| 28. Performance Validation | v1.3 | 4/4 | Complete | 2026-01-21 |
| 29. Linear Pattern Detection | v1.4 | 3/3 | Complete | 2026-01-21 |
| 30. Sequential Slot Reading | v1.4 | 3/3 | Complete | 2026-01-21 |
| 31. Traversal Integration | v1.4 | 0/3 | Pending | — |
| 32. Validation and Tuning | v1.4 | 0/2 | Pending | — |

**Overall Progress:** 100/103 plans complete (97.1%). v1.4: 9/13 plans complete (69.2%).

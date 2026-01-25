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
- **v1.4 Sequential I/O Optimization** — Phases 29-32 (shipped 2026-01-21)
- **v1.6 Chain Locality** — Phases 33-36 (shipped 2026-01-21)
- **v1.7 Gap Closure** — Phase 37 (implementation complete)
- **v1.8 ACID API Fix** — Phase 38 (current)

---

## Phases

<details>
<summary>v0.2-v1.6 Archive</summary>

See milestone archives for complete history.
- v0.2 Foundation: Phases 1-7
- v1.0 Production: Phases 8-10
- v1.1 ACID & Reliability: Phases 11-22
- v1.2 Benchmark Infrastructure: Phases 23-24
- v1.3 Chain Traversal Performance: Phases 25-28
- v1.4 Sequential I/O Optimization: Phases 29-32
- v1.6 Chain Locality: Phases 33-36

</details>

---

## v1.6 Chain Locality (Phases 33-36) - COMPLETE

**Milestone Goal:** Achieve IO-12 target (Chain(500) <=75ms, 3x SQLite) through traversal-time sequential cluster reads.

**Status:** COMPLETE (2026-01-21)
- IO-12 Target: NOT ACHIEVED (Chain(500) = 231.12ms vs 75ms target, 3.08x gap)
- MVCC Isolation: CONFIRMED (15/15 tests passed)
- Requirements: 5/5 satisfied
- Next: Gap closure via Phase 37

**Background:** v1.4 achieved linear pattern detection and sequential slot reading. However, edge clusters for sequential chains are stored non-contiguously in the global cluster pool. The IO-12 target (9.96x gap) remains unmet because prefetching non-contiguous clusters is still random I/O.

**Surgical Solution:** Traversal-time sequential cluster reads. Detect chains during traversal (not at write time), read all clusters in single I/O when chain confirmed, fall back immediately when pattern breaks. No write-time allocation, no migration, no metadata storage.

**Why surgical:** Write-time detection risks false positives and schema debt. Traversal-time approach is reversible, honest, and closes IO-12 without collateral damage.

### Phase 33: Traversal-Time Chain Detection
**Goal:** Traversal detects linear chains and switches to sequential cluster reads
**Depends on**: Phase 32 (v1.4 complete)
**Requirements:** CL-01 ✓ SATISFIED, CL-03 ✓ SATISFIED
**Plans:** 5/5 complete (extend LinearDetector with cluster offset tracking, contiguity validation, sequential read trigger, instrumentation, integration tests)

**Plans:**
- [x] 33-01-PLAN.md — Cluster offset tracking in LinearDetector
- [x] 33-02-PLAN.md — Cluster contiguity validation
- [x] 33-03-PLAN.md — Sequential read trigger condition
- [x] 33-04-PLAN.md — Chain detection instrumentation
- [x] 33-05-PLAN.md — Integration tests for graph patterns

### Phase 34: Sequential Cluster Reader
**Goal:** Sequential cluster reader reads all clusters for a chain in single I/O operation
**Depends on**: Phase 33
**Requirements:** CL-02 (with Phase 35 split)
**Plans:** 3/3 complete

**Plans:**
- [x] 34-01-PLAN.md — Create SequentialClusterReader module with read_chain_clusters() method
- [x] 34-02-PLAN.md — Add cluster buffer fields to TraversalContext
- [x] 34-03-PLAN.md — Integrate sequential cluster read into get_neighbors_optimized()

### Phase 35: Neighbor Extraction and Fallback
**Goal:** Extract neighbors from cluster buffer and fall back immediately when pattern breaks
**Depends on**: Phase 34
**Requirements:** CL-02 (completion), CL-04
**Plans:** 4/4 complete

**Plans:**
- [x] 35-01-PLAN.md — Add node_cluster_index field to TraversalContext
- [x] 35-02-PLAN.md — Extract neighbors from cluster_buffer using mapping
- [x] 35-03-PLAN.md — Add traverse_with_detection helper and unit tests
- [x] 35-04-PLAN.md — Integration tests for extraction and fallback

### Phase 36: IO-12 Validation
**Goal:** MVCC isolation preserved and IO-12 target achieved
**Depends on**: Phase 35
**Requirements:** CL-05
**Plans:** 4/4 complete
**Status:** Complete (2026-01-21) - IO-12 target NOT achieved

**Actual Results:**
- Chain(500): 231.12ms (target: <=75ms)
- MVCC isolation: 15/15 tests passed
- Star/Random: No regression detected

**Plans:**
- [x] 36-01-PLAN.md — Create Criterion benchmark suite for IO-12 validation
- [x] 36-02-PLAN.md — Validate MVCC isolation for sequential cluster reads
- [x] 36-03-PLAN.md — Run benchmarks and document IO-12 status
- [x] 36-04-PLAN.md — Update documentation with Phase 36 completion

---

## v1.7 Gap Closure (Phase 37) - IMPLEMENTATION COMPLETE

**Milestone Goal:** Close the 156.12ms gap to achieve IO-12 target (Chain(500) <=75ms)

**Status:** IMPLEMENTATION COMPLETE (2026-01-25)
- Gap: 156.12ms remaining (231.12ms actual vs 75ms target) - from v1.6 baseline
- Root cause: IDENTIFIED - BFS used observe() instead of observe_with_cluster()
- Fix: IMPLEMENTED - All 4 BFS implementations now call observe_with_cluster()
- Verification: INTEGRATION TESTS PASS - cluster_offsets_count: 500, fragmentation_score: 0.0
- **BENCHMARK REQUIRED** - Fix implemented but Chain(500) benchmark needs to run to confirm target achieved

**Approach:** Diagnostic investigation first, then surgical optimization:
1. Add internal instrumentation to LinearDetector, SequentialClusterReader, TraversalContext
2. Run external profiling (perf flamegraphs, strace I/O tracing)
3. Create microbenchmark suite to isolate component costs
4. Analyze telemetry to identify root cause (I/O vs CPU vs fragmentation)
5. Implement surgical optimization based on diagnosis
6. Verify no regressions (write cost, memory, concurrency)

**Success Criteria:**
- Chain(500) <= 75ms (IO-12 target achieved)
- Write-path cost increase <= +5%
- Memory overhead <= +5%
- No new lock contention
- Star/Random traversals within 10% of v1.6 baseline

**Plans:**
- [x] 37-01-PLAN.md — Internal instrumentation (LinearDetector, SequentialClusterReader, TraversalContext telemetry)
- [x] 37-02-PLAN.md — External profiling scripts (perf flamegraphs, strace I/O tracing)
- [x] 37-03-PLAN.md — Microbenchmark suite (cluster population, LinearDetector overhead, fragmentation)
- [x] 37-04-PLAN.md — Root cause analysis (run Chain(500) with instrumentation, interpret data, generate diagnosis)
- [x] 37-05-PLAN.md — Surgical optimization (BFS observe_with_cluster() fix based on diagnosis)
- [x] 37-06-PLAN.md — Regression testing (write cost, memory, concurrency, non-chain patterns)

---

## v1.8 ACID API Fix (Phase 38) - PLANNED

**Milestone Goal:** Fix public read APIs to enforce snapshot isolation - no API may observe state not bound to a committed snapshot_id

**Status:** PLANNED (2026-01-25)
- **Root Cause Identified:** Public read APIs bypass transaction system, can observe uncommitted WAL state
- **Transaction System Exists:** `TransactionId`, `IsolationLevel::Snapshot`, `commit_transaction()` all present but unused by reads
- **Hard Rule:** No API may observe state not bound to a committed snapshot_id
- **Next:** Execute 38-01 through 38-06 plans

**Problem:**
- `GraphBackend::neighbors()`, `get_node()`, etc. call `with_graph_file()` directly
- No `snapshot_id` parameter or filtering
- `acquire_snapshot()` opens `:memory:` connection, not real database
- Reads can see partially committed WAL state

**Approach:**
1. Audit all public read APIs for snapshot_id violations
2. Design `SnapshotId(u64)` type and propagation strategy
3. Update `GraphBackend` trait with `snapshot_id` parameter on all reads
4. Implement WAL filtering (`tx_id <= snapshot_id`)
5. Add regression tests for the bug
6. Verify no performance regression (run Phase 37 benchmark)

**Success Criteria:**
- Every public read API accepts/infers `snapshot_id: SnapshotId`
- Reads filter WAL records to only show data ≤ snapshot_id
- Regression test reproduces and verifies fix for original bug
- Phase 37 benchmarks pass (no performance regression)
- Overhead < 5%

**Plans:**
- [ ] 38-01-PLAN.md — Audit all public read APIs for snapshot_id violations
- [ ] 38-02-PLAN.md — Design snapshot_id architecture
- [ ] 38-03-PLAN.md — Implement snapshot_id parameter in GraphBackend trait
- [ ] 38-04-PLAN.md — Implement WAL filtering by snapshot_id
- [ ] 38-05-PLAN.md — Add regression tests for ACID snapshot isolation
- [ ] 38-06-PLAN.md — Verify no performance regression

---

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → ... → 32 → 33 → 34 → 35 → 36 → 37 → 38

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-32 | v0.2-v1.4 | 109/109 | Complete | 2026-01-21 |
| 33. Traversal-Time Chain Detection | v1.6 | 5/5 | Complete | 2026-01-21 |
| 34. Sequential Cluster Reader | v1.6 | 3/3 | Complete | 2026-01-21 |
| 35. Neighbor Extraction and Fallback | v1.6 | 4/4 | Complete | 2026-01-21 |
| 36. IO-12 Validation | v1.6 | 4/4 | Complete | 2026-01-21 |
| 37. Gap Analysis and Closure | v1.7 | 6/6 | Implementation Complete | 2026-01-25 |
| 38. ACID API Fix | v1.8 | 0/6 | Planned | - |

**Overall Progress:** 146/146 plans planned (140 complete, 6 planned). v1.4 complete, v1.6 complete, v1.7 implementation complete (benchmark pending), v1.8 planned.

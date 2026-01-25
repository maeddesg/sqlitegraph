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
- **v1.8 ACID API Fix** — Phase 38 (complete)
- **v1.9 WAL Filtering & Allocation Optimization** — Phase 40 (complete)
- **v1.10 ACID API Completion** — Phase 41 (planned)

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

## v1.8 ACID API Fix (Phase 38) - INFRASTRUCTURE COMPLETE

**Milestone Goal:** Fix public read APIs to enforce snapshot isolation - no API may observe state not bound to a committed snapshot_id

**Status:** INFRASTRUCTURE COMPLETE (2026-01-25)
- **SnapshotId Type:** Implemented (38-02) - `SnapshotId(pub u64)` with current/from_tx/invalid/as_lsn methods
- **GraphBackend Trait:** Updated (38-03) - All read methods accept snapshot_id parameter
- **LSN-Based Architecture:** Designed (38-01/38-04) - SnapshotId = CommitLSN, TxRangeIndex for tracking
- **WAL Infrastructure:** Built (38-04) - TxRangeIndex, build_tx_index(), snapshot-aware helpers
- **Regression Tests:** Created (38-05) - acid_regression_test.rs, acid_snapshot_test.rs
- **Performance Baseline:** Verified (38-06) - Chain(500) = 234.79ms, no regression from API changes

**Deferred Work (Full WAL Filtering):**
- Actual WAL record filtering in read paths is deferred
- Snapshot-aware methods have TODO placeholders
- Integration tests commented out pending full implementation
- Required for complete snapshot isolation (committed-but-not-checkpointed data visibility)

**Architecture Decision - LSN-Based Snapshot Isolation:**
- SnapshotId = CommitLSN (commit sequence number, monotonic u64)
- Visibility Rule: tx.commit_lsn <= snapshot_id
- WAL Contiguity Invariant: Records for a transaction are contiguous in WAL (single-writer model)
- TxRangeIndex tracks transaction begin_lsn and commit_lsn for visibility filtering

**Plans:**
- [x] 38-01-PLAN.md — Audit all public read APIs for snapshot_id violations
- [x] 38-02-PLAN.md — Design snapshot_id architecture
- [x] 38-03-PLAN.md — Implement snapshot_id parameter in GraphBackend trait
- [x] 38-04-PLAN.md — Implement WAL filtering infrastructure (full filtering deferred)
- [x] 38-05-PLAN.md — Add regression tests for ACID snapshot isolation
- [x] 38-06-PLAN.md — Verify no performance regression

---

## v1.9 WAL Filtering & Allocation Optimization (Phase 40) - COMPLETE

**Milestone Goal:** Complete WAL filtering (Phase 38 deferred work) and achieve IO-12 target (Chain(500) <=75ms) through allocation-aware sequential cluster optimization.

**Status:** COMPLETE (2026-01-25)
- **Wave 1:** COMPLETE - Delta-index filtering implemented and validated
- **Wave 2:** COMPLETE - Allocation-aware optimization implemented
- **IO-12 Target:** NOT ACHIEVED (Chain(500) = 213.24ms vs target of <=75ms, ~3x gap)

**Wave 1 - WAL Filtering Completion (40-01 to 40-06):** ✅ COMPLETE
Implemented commit-delta index for snapshot isolation correctness. The original "WAL overlay per read" design was replaced with "commit-built delta overlay" to preserve the mmap fast path.

- **40-01:** ✅ Source of truth functions (is_tx_visible, iter_visible_wal_records)
- **40-02:** ✅ WAL contiguity invariant enforcement
- **40-03:** ✅ DeltaIndex module (commit-time delta building, not WAL scanning)
- **40-04:** ✅ Commit-time delta integration with V2WALManager
- **40-05:** ✅ Delta-aware read paths
- **40-06:** ✅ Regression validation (Chain(500) = 238.00ms, -0.8% change, <1% overhead target met)

**Wave 2 - Allocation-Aware Optimization (40-07 to 40-12):** ✅ COMPLETE
Implement contiguous cluster allocation for linear chains to achieve IO-12 target.

- **40-07:** ✅ FreeSpaceManager contiguous reservation API (15 tests pass)
- **40-08:** ✅ Region accounting (commit/rollback/recovery, 32 tests pass)
- **40-09:** ✅ AdjacencyWriter write_cluster_with_hint() (22 tests pass)
- **40-10:** ✅ Threshold-gated activation (39 tests pass)
- **40-11:** ✅ WAL records for contiguous allocation (73 tests pass)
- **40-12:** ✅ Benchmark gates (IO-12 validation - TARGET NOT ACHIEVED)

**Benchmark Results:**
- Chain(500): 213.24ms (target: <=75ms, FAIL)
- Star(100): 23.094us (7.1% improvement, PASS)
- Random(100): 22.839us (8.2% improvement, PASS)
- Random(500): 14.856us (4.7% improvement, PASS)
- Snapshot isolation: 1/1 tests pass (PASS)

**Key Finding:** The contiguous allocation optimization implementation is complete but did NOT produce the expected 3x performance improvement. The IO-12 target remains unmet.

**Plans:**
- [x] 40-01-PLAN.md — Source of truth functions for WAL visibility
- [x] 40-02-PLAN.md — Enforce WAL contiguity invariants
- [x] 40-03-PLAN.md — Implement DeltaIndex module (commit-time delta building)
- [x] 40-04-PLAN.md — Commit-time delta integration with V2WALManager
- [x] 40-05-PLAN.md — Delta-aware read paths
- [x] 40-06-PLAN.md — Regression gates and validation
- [x] 40-07-PLAN.md — FreeSpaceManager contiguous reservation API
- [x] 40-08-PLAN.md — Region accounting (commit/rollback/recovery)
- [x] 40-09-PLAN.md — AdjacencyWriter write_cluster_with_hint()
- [x] 40-10-PLAN.md — Threshold-gated activation
- [x] 40-11-PLAN.md — WAL records for contiguous allocation
- [x] 40-12-PLAN.md — Benchmark gates and IO-12 validation

---

## v1.10 ACID API Completion (Phase 41) - PLANNED

**Milestone Goal:** Complete the ACID API fix by removing `*_current()` convenience methods that allow implicit snapshot usage

**Status:** PLANNED

**Problem:** Phase 38 added convenience methods (`get_node_current`, `neighbors_current`, etc.) that implicitly use `SnapshotId::current()`, violating the hard rule that no API may observe state not bound to a committed snapshot_id.

**Hard Rule (NON-NEGOTIABLE):**
> No API may observe state not bound to a committed snapshot_id.
> If a value cannot be tied to a committed snapshot → it does not exist.

**Solution:** Remove the `impl dyn GraphBackend` block containing all 9 `*_current()` convenience methods (backend.rs:266-352). These methods are currently unused in the codebase, making this a clean removal.

**Plans:**
- [ ] 41-01-PLAN.md — Remove `*_current()` convenience methods from GraphBackend

**Success Criteria:**
- All public read APIs require explicit snapshot_id parameter
- No convenience shortcut exists for implicit snapshot usage
- Compiler enforces snapshot passing at all call sites
- Architectural decision logged documenting hard rule enforcement

---

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → ... → 32 → 33 → 34 → 35 → 36 → 37 → 38 → 40 → 41

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-32 | v0.2-v1.4 | 109/109 | Complete | 2026-01-21 |
| 33. Traversal-Time Chain Detection | v1.6 | 5/5 | Complete | 2026-01-21 |
| 34. Sequential Cluster Reader | v1.6 | 3/3 | Complete | 2026-01-21 |
| 35. Neighbor Extraction and Fallback | v1.6 | 4/4 | Complete | 2026-01-21 |
| 36. IO-12 Validation | v1.6 | 4/4 | Complete | 2026-01-21 |
| 37. Gap Analysis and Closure | v1.7 | 6/6 | Implementation Complete | 2026-01-25 |
| 38. ACID API Fix | v1.8 | 6/6 | Infrastructure Complete | 2026-01-25 |
| 40. WAL Filtering & Allocation Optimization | v1.9 | 12/12 | Complete (IO-12 target NOT achieved) | 2026-01-25 |
| 41. ACID API Completion | v1.10 | 0/1 | Planned | TBD |

**Overall Progress:** 159/160 plans planned (158 complete, 1 planned). v0.2-v1.9 complete, v1.10 planned.

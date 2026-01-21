# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-21)

**Core value:** Feature parity, performance, and reliability equally. Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.
**Current focus:** v1.6 milestone - Chain Locality

## Current Position

Phase: 37 - Gap Analysis Closure
Plan: 06 (complete)
Status: Phase 37 Plan 06 complete - regression test suite created for BFS optimization validation
Last activity: 2026-01-22 — Completed Phase 37 Plan 06: Regression test suite (write cost, memory overhead, concurrency, non-chain patterns)

Progress: [██████████░] 99% (37/37 phases planned, 134/134 plans complete, v1.4 complete, v1.6 gap analysis closure in progress)

## v1.6 Milestone Goals

**Problem:** Chain traversals have 10x performance gap vs SQLite (Chain(500) = 248.68ms, target: 75ms).

**Root Cause:** Edge clusters for sequential chains are stored non-contiguously. Prefetching doesn't reduce I/O count.

**Surgical Solution:** Traversal-time sequential cluster reads.
- Reuse existing LinearDetector (proven by v1.4)
- Detect chains during traversal (not at write time)
- Add sequential cluster reader for confirmed chains
- Fall back immediately when pattern breaks
- No write-time allocation, no migration, no metadata storage

**Target:** IO-12 — Chain(500) <=75ms (3x SQLite)

**Actual Result:** Chain(500) = 231.12ms (NOT achieved, 3.08x over target)
**Gap:** 156.12ms remaining
**Speedup:** 1.07x vs baseline (248.68ms -> 231.12ms, expected 3.3x)

**Root Cause Analysis (from 37-04):**
- **ROOT CAUSE IDENTIFIED (HIGH confidence):** BFS uses observe() not observe_with_cluster()
- Line 164 in graph_ops/mod.rs: `ctx.detector.observe(current_node, degree)` instead of `observe_with_cluster()`
- Result: LinearDetector cannot track cluster offsets (cluster_offsets_count: 0), never confirms chains (chains_detected: 0)
- Sequential cluster read optimization never engages, explaining 1.07x speedup vs expected 3.3x
- I/O is NOT bottleneck (strace confirms mmap working)
- CPU is NOT bottleneck outside missing optimization (flamegraph confirms no SequentialClusterReader activity)
- Recommended fix: Update native_bfs() to use observe_with_cluster(), expected 75-100ms (2.3-3.1x speedup)

**FIX IMPLEMENTED (37-05):**
- ✅ BFS traversal now calls observe_with_cluster() with cluster offset and size metadata (4 locations)
- ✅ Cluster metadata extracted via graph_file.read_node_at() (outgoing_cluster_offset, outgoing_cluster_size)
- ✅ TraversalContext::get_cluster_info() helper method added for clean abstraction
- ✅ Integration tests confirm: cluster_offsets_count: 500, fragmentation_score: 0.0, gap_bytes: 0
- ✅ Cluster offset tracking is now ENABLED - sequential cluster read optimization can engage

**NEXT STEP:** Run Chain(500) benchmark to measure performance improvement (expected: 75-100ms reduction, 2.3-3.1x speedup)

## v1.6 Requirements Coverage

| Requirement | Phase | Status |
|-------------|-------|--------|
| CL-01: Traversal detects linear chains and switches to sequential cluster reads | Phase 33 | Complete (5/5 plans) |
| CL-02: Sequential cluster reader reads all clusters for a chain in single I/O | Phase 34-35 | Complete (34-01/02/03, 35-01/02/03/04) |
| CL-03: LinearDetector validates cluster contiguity before sequential read path | Phase 33 | Complete (33-02) |
| CL-04: Chain read path falls back immediately when pattern breaks | Phase 35 | Complete (35-01/02/03/04) |
| CL-05: MVCC isolation preserved (no cross-traversal pollution) | Phase 36 | Complete (36-01/02/03/04) |

**Coverage: 5/5 requirements complete (100%)**

## v1.6 Roadmap Summary

**Phases:** 4 phases (33-36)
**Depth:** Comprehensive
**Scope:** Surgical traversal-time optimization

| Phase | Goal | Requirements | Status |
|-------|------|--------------|--------|
| 33 - Traversal-Time Chain Detection | Extend LinearDetector to track cluster offsets, validate contiguity, and instrument chain detection | CL-01, CL-03 | Complete (5/5 plans) |
| 34 - Sequential Cluster Reader | Read all clusters for a chain in single I/O operation | CL-02 (partial, with Phase 35 split) | Complete (3/3 plans) |
| 35 - Neighbor Extraction and Fallback | Extract neighbors from cluster_buffer and fall back immediately when pattern breaks | CL-02 (completion), CL-04 | Complete (4/4 plans) |
| 36 - IO-12 Validation | Verify MVCC isolation preserved and Chain(500) <=75ms target achieved | CL-05 | Complete (36-01/02/03/04) |

## Performance Metrics

**Velocity:**
- Total plans completed: 133
- Total plans planned: 133
- Average duration: 7 min
- Total execution time: ~15.5 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| v1.0 (1-10) | 33 | ~3 days | ~5 min |
| v1.1 (11-22) | 42 | ~4 days | ~7 min |
| v1.2 (23-24) | 7 | 1 day | ~7 min |
| v1.3 (25-28) | 16 | ~30 min | ~7 min |
| v1.4 (29-32) | 12 | ~13 min | ~3 min |
| v1.6 (33-36) | 11 | ~15 min | ~2 min |
| 37 - Gap Analysis | 5 | ~1h 30min | ~18 min |

**Recent Trend:**
- Last 5 plans: ~7 min each
- Trend: Stable

---

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- v1.3: Per-traversal cache selected over global cache (preserves MVCC isolation)
- v1.3: HashMap<NodeId, Vec<NodeId>> chosen to avoid Arc<NodeRecord> cycles
- v1.3: Chain graphs have 0% cache hit rate by design - per-traversal cache provides no benefit for pure linear traversals
- v1.4: Sequential I/O coalescing is the correct approach for chain optimization (based on research)
- v1.4: Traversal-scoped buffers (not global) to preserve MVCC isolation
- v1.4: 3-step linear detection threshold to avoid false positives on trees
- v1.4: 8-slot prefetch window (32KB) based on RocksDB/LMDB research
- **v1.6: Traversal-time detection chosen over write-time - correctness first, reuse existing LinearDetector**
- **v1.6: No migration required - existing databases work unchanged**
- **v1.6: Surgical scope - traversal-time sequential reads only, no metadata storage**
- **v1.6.1: Vec<(u64, u32)> for cluster_offsets - simple tuple storage sufficient for contiguity validation**
- **v1.6.1: observe_with_cluster() separate method - maintains backward compatibility with existing observe() calls**
- **v1.6.2: are_clusters_contiguous() pure function for independent testing and validation**
- **v1.6.4: Simple u64 counters for chain instrumentation (chains_detected, total_chain_length) - no atomic operations needed for single-threaded traversal**
- **v1.6.5: should_use_sequential_read() combines is_linear_confirmed() && validate_contiguity() for Phase 34 integration**
- **v1.6.5: Integration tests validate chain detection on Chain(100), prevent false positives on trees/diamonds, and reject non-contiguous storage**
- **v1.6.6: 512KB MAX_CLUSTER_BUFFER_SIZE bounds memory usage, sufficient for ~128 clusters of 4KB each**
- **v1.6.6: Stateless SequentialClusterReader design with parameter-passed offsets keeps module simple and testable**
- **v1.6.6: Deferred deserialization (raw bytes → neighbors on-demand) avoids CPU cost for clusters never accessed**
- **v1.6.7: TraversalContext cluster_buffer field uses Option<Vec<u8>> to make sequential read state explicit**
- **v1.6.7: TraversalContext cluster_buffer_offsets field matches detector.cluster_offsets() return type**
- **v1.6.7: clear_cluster_buffer() method enables explicit buffer clearing on fallback and reset**
- **v1.6.8: Lazy trigger pattern ensures sequential read happens once on first miss after linear confirmation**
- **v1.6.8: Error handling leaves cluster_buffer as None on sequential read failure (graceful fallback to L2/L3)**
- **v1.6.8: Full neighbor extraction from cluster_buffer deferred to Phase 35 (requires node_id -> cluster_index mapping)**
- **v1.6.9: AHashMap<NativeNodeId, usize> for node_id -> cluster_index mapping (Phase 35)**
- **v1.6.9: Caller responsibility for mapping population (preserves separation of concerns)**
- **v1.6.9: Immediate fallback on Branching pattern (clear cluster_buffer and node_cluster_index)**
- **v1.6.9: Minimal fallback state reset (only cluster_buffer fields, not L1/L2/L3 caches)**
- **v1.6.10: Graceful fallback on extraction failure - fall through to L2/L3 instead of failing traversal (Phase 35-02)**
- **v1.6.10: L2 cache insertion on successful extraction - subsequent lookups avoid extraction overhead (Phase 35-02)**
- **v1.6.11: traverse_with_detection() helper demonstrates mapping population pattern (Phase 35-03)**
- **v1.6.11: cluster_index calculated as offsets.len() - 1 after observe_with_cluster() (Phase 35-03)**
- **v1.6.11: Immediate fallback on Branching triggers clear_cluster_buffer() (Phase 35-03)**
- **v1.6.12: MVCC isolation testing pattern - scoped blocks to force context drop, assert fresh state (Phase 36-02)**
- **v1.6.12: Per-field isolation testing - cluster_buffer, cluster_buffer_offsets, node_cluster_index independently validated (Phase 36-02)**
- **v1.6.13: IO-12 target NOT achieved - Chain(500) = 231.12ms, only 1.07x speedup vs 248.68ms baseline (expected 3.3x for 75ms target)**
- **v1.6.13: Sequential cluster reads not providing expected improvement - gap closure requires profiling to identify bottleneck (I/O vs CPU)**
- **v1.6: Chain Locality milestone complete** - All 4 phases executed, MVCC isolation confirmed (CL-05 satisfied)
- **v1.6: IO-12 target NOT achieved** - Chain(500) = 231.12ms vs 75ms target (3.08x gap)
- **v1.6: Sequential cluster reader implemented** - Infrastructure in place but not achieving expected 3.3x speedup
- **v1.6: Next action required** - Profile Chain(500) to identify bottleneck before gap closure
- **v1.6.14: Timing instrumentation added to LinearDetector - time_linear_detection_ns and time_contiguity_validation_ns track pattern detection performance (Phase 37-01)**
- **v1.6.14: SequentialClusterReader metrics added - read_time_ns, total_bytes_read, clusters_read, extract_time_ns, extract_count for I/O profiling (Phase 37-01)**
- **v1.6.14: TraversalContext telemetry export - export_telemetry() returns JSON with time_total_ms, nodes_visited, cluster_hits/misses, fragmentation_score, dedupe_ms, sort_ms, linear_detection_ms, contiguity_validation_ms (Phase 37-01)**
- **v1.6.14: Fragmentation calculation uses gap_bytes / total_span - measures gaps relative to spanned storage for I/O efficiency assessment (Phase 37-01)**
- **v1.6.14: SequentialClusterReader changed from stateless to stateful - metrics field added for diagnostic tracking (Phase 37-01)**
- **v1.6.15: Use cargo flamegraph wrapper instead of raw perf record - simpler for Rust projects with automatic symbol resolution (Phase 37-02)**
- **v1.6.15: 99Hz sampling frequency for flamegraph - good resolution without excessive overhead (Phase 37-02)**
- **v1.6.15: DWARF call graphs for accurate traces - optimized Rust builds with debug symbols (Phase 37-02)**
- **v1.6.15: Trace specific syscalls (mmap, read, lseek) instead of all syscalls - focused output for I/O pattern analysis (Phase 37-02)**
- **v1.6.16: Root cause identified with HIGH confidence - BFS uses observe() not observe_with_cluster() (Phase 37-04)**
- **v1.6.16: Telemetry JSON provides definitive evidence - chains_detected=0, cluster_offsets_count=0, cluster_hits=498 (old prefetch buffer) (Phase 37-04)**
- **v1.6.16: Code inspection confirms line 164 in graph_ops/mod.rs uses observe() instead of observe_with_cluster() (Phase 37-04)**
- **v1.6.16: Recommended fix is surgical - Update native_bfs() to use observe_with_cluster() with cluster metadata (Phase 37-04)**
- **v1.6.16: Expected improvement 75-100ms (2.3-3.1x speedup), closes 84-100% of gap to 75ms target (Phase 37-04)**
- **v1.6.17: Extract cluster metadata inline via graph_file.read_node_at() - O(1) per node, no memory overhead (Phase 37-05)**
- **v1.6.17: Use get_cluster_info() helper pattern for consistency across traversal implementations (Phase 37-05)**
- **v1.6.17: Test success criteria: cluster_offsets_count > 0 (cluster metadata tracked) not chains_detected > 0 (only incremented by explicit record_chain()) (Phase 37-05)**
- **v1.6.17: Graceful fallback to (0, 0) cluster metadata on node read failure - allows traversal to continue (Phase 37-05)**
- **v1.6.17: Cluster offset tracking now ENABLED - sequential cluster read optimization can engage (Phase 37-05)**

### Pending Todos

v1.6 Chain Locality:
- [x] Phase 33 Plan 01: Cluster offset tracking (completed)
- [x] Phase 33 Plan 02: Contiguity validation (completed)
- [x] Phase 33 Plan 03: Sequential read trigger (completed)
- [x] Phase 33 Plan 04: Chain detection instrumentation (completed)
- [x] Phase 33 Plan 05: Integration tests for graph patterns (completed)
- [x] Phase 34 Plan 01: Sequential cluster reader module (completed)
- [x] Phase 34 Plan 02: TraversalContext cluster buffer integration (completed)
- [x] Phase 34 Plan 03: Lazy sequential cluster read trigger (completed)
- [x] Phase 35 Plan 01: Node_id -> cluster_index mapping (completed)
- [x] Phase 35 Plan 02: Neighbor extraction from cluster_buffer (completed)
- [x] Phase 35 Plan 03: Traversal helper and unit tests (completed)
- [x] Phase 35 Plan 04: Integration tests for extraction and fallback (completed)
- [x] Phase 36 Plan 01: IO-12 validation benchmark suite (completed)
- [x] Phase 36 Plan 02: MVCC isolation tests (completed)
- [x] Phase 36 Plan 03: Performance validation (completed - IO-12 NOT achieved, gap closure required)
- [x] Phase 36 Plan 04: Documentation update (completed)

Next actions:
- [x] Profile Chain(500) to identify bottleneck (I/O count vs CPU time) - COMPLETE (37-04)
- [x] Verify LinearDetector confirms chain pattern - COMPLETE (37-04: chains_detected=0, NOT confirming)
- [x] Verify SequentialClusterReader is engaged during traversal - COMPLETE (37-04: NOT engaging)
- [x] Verify cluster_buffer is populated during traversal - COMPLETE (37-04: NOT populated)
- [x] **Phase 37-05: Fix BFS to use observe_with_cluster()** - COMPLETE (37-05)
- [x] **Phase 37-06: Create regression test suite** - COMPLETE (37-06)
- [ ] **Run Chain(500) benchmark to measure performance improvement** (expected: 75-100ms reduction, 2.3-3.1x speedup)
- [ ] Run regression benchmarks to validate Tier 2 criteria:
  - [ ] Write cost: ≤+5% increase (regression_write_cost.rs)
  - [ ] Memory overhead: ≤+5% (regression_memory.rs)
  - [ ] Concurrency: No new lock contention (regression_concurrent_traversal.rs)
  - [ ] Non-chain patterns: Within 10% baseline (regression_non_chain_patterns.rs)
- [ ] Verify cluster_offsets_count > 0, fragmentation_score = 0.0 in telemetry
- [ ] Compare to 75ms target, close gap or consider write-time allocation if insufficient

### Blockers/Concerns

- v1.6: Surgical scope requires discipline — avoid adding write-time allocation or metadata storage unless surgical fix insufficient
- v1.6: Cluster contiguity validation must be robust to avoid performance regression on non-contiguous data
- **RESOLVED (37-04):** Root cause identified, clear surgical fix path defined
- **LOW risk:** V2_SLOT_DEBUG logging spam in release builds may skew measurements (cleanup recommended)

## Session Continuity

Last session: 2026-01-22
Stopped at: Completed Phase 37 Plan 06: Regression test suite created - write cost, memory overhead, concurrency, non-chain pattern benchmarks ready for validation
Resume file: None

### Roadmap Evolution

- **v0.2 Foundation** (2026-01-17): Phases 1-7 complete
- **v1.0 Production** (2026-01-17): Phases 8-10 complete
- **v1.1 ACID & Reliability** (2026-01-20): Phases 11-22 complete
- **v1.2 Benchmark Infrastructure** (2026-01-21): Phases 23-24 complete
- **v1.3 Chain Traversal Performance** (2026-01-21): Phases 25-28 complete
- **v1.4 Sequential I/O Optimization** (2026-01-21): Phases 29-32 complete (IO-13 satisfied, IO-12 deferred)
- **v1.6 Chain Locality** (2026-01-21): Phases 33-36 complete (surgical traversal-time approach)
  - Phase 33 Plan 01 (2026-01-21): Cluster offset tracking complete
  - Phase 33 Plan 02 (2026-01-21): Contiguity validation complete
  - Phase 33 Plan 03 (2026-01-21): Sequential read trigger complete
  - Phase 33 Plan 04 (2026-01-21): Chain detection instrumentation complete
  - Phase 33 Plan 05 (2026-01-21): Integration tests for graph patterns complete (Phase 33 complete)
  - Phase 34 Plan 01 (2026-01-21): SequentialClusterReader module complete
  - Phase 34 Plan 02 (2026-01-21): TraversalContext cluster buffer integration complete
  - Phase 34 Plan 03 (2026-01-21): Lazy sequential cluster read trigger complete (Phase 34 complete)
  - Phase 35 Plan 01 (2026-01-21): Node_id -> cluster_index mapping complete
  - Phase 35 Plan 02 (2026-01-21): Neighbor extraction from cluster_buffer complete
  - Phase 35 Plan 03 (2026-01-21): Traversal helper and unit tests complete
  - Phase 35 Plan 04 (2026-01-21): Integration tests for extraction and fallback complete (Phase 35 complete)
  - Phase 36 Plan 01 (2026-01-21): IO-12 validation benchmark suite complete
  - Phase 36 Plan 02 (2026-01-21): MVCC isolation tests complete
  - Phase 36 Plan 03 (2026-01-21): Performance validation complete - IO-12 NOT achieved (Chain(500) = 231.12ms, target: 75ms, gap: 156ms)
  - Phase 36 Plan 04 (2026-01-21): Documentation update complete - Phase 36 complete, v1.6 milestone complete, gap identified
- **v1.7 Gap Analysis** (2026-01-21): Phase 37-01/02/03/04/05/06 complete
  - Phase 37 Plan 01 (2026-01-21): Gap analysis instrumentation complete - LinearDetector, SequentialClusterReader, TraversalContext with timing and telemetry export
  - Phase 37 Plan 02 (2026-01-21): External profiling scripts complete - CPU flamegraph (cargo flamegraph, 99Hz) and strace I/O tracing (mmap, read, lseek)
  - Phase 37 Plan 03 (2026-01-21): Microbenchmark suite complete - cluster population, LinearDetector overhead, fragmentation impact (Criterion-based)
  - Phase 37 Plan 04 (2026-01-22): Diagnostic pipeline complete - telemetry benchmark executed, flamegraph generated, strace I/O traced, root cause diagnosis created (HIGH confidence: BFS uses observe() not observe_with_cluster())
  - Phase 37 Plan 05 (2026-01-22): Surgical BFS optimization complete - cluster metadata extraction via graph_file.read_node_at(), observe_with_cluster() in all 4 BFS implementations, TraversalContext::get_cluster_info() helper, integration tests confirm cluster_offsets_count: 500, fragmentation_score: 0.0, gap_bytes: 0
  - Phase 37 Plan 06 (2026-01-22): Regression test suite complete - write cost, memory overhead, concurrency, non-chain pattern benchmarks created, regression_report.md documentation complete

*Updated after each plan completion*

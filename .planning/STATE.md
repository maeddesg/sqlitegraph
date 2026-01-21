# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-21)

**Core value:** Feature parity, performance, and reliability equally. Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.
**Current focus:** v1.6 milestone - Chain Locality

## Current Position

Phase: 33 - Traversal-Time Chain Detection
Plan: 3 of 4
Status: In progress - chain detection instrumentation complete
Last activity: 2026-01-21 — Completed 33-04 chain detection instrumentation

Progress: [█████████░] 97.4% (32/32 phases complete, 112/114 plans complete, v1.4 complete, v1.6 75% done)

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

## v1.6 Requirements Coverage

| Requirement | Phase | Status |
|-------------|-------|--------|
| CL-01: Traversal detects linear chains and switches to sequential cluster reads | Phase 33 | In Progress (contiguity validation complete) |
| CL-02: Sequential cluster reader reads all clusters for a chain in single I/O | Phase 34 | Pending |
| CL-03: LinearDetector validates cluster contiguity before sequential read path | Phase 33 | Complete (33-02) |
| CL-04: Chain read path falls back immediately when pattern breaks | Phase 35 | Pending |
| CL-05: MVCC isolation preserved (no cross-traversal pollution) | Phase 36 | Pending |

**Coverage: 5/5 requirements mapped (100%)**

## v1.6 Roadmap Summary

**Phases:** 4 phases (33-36)
**Depth:** Comprehensive
**Scope:** Surgical traversal-time optimization

| Phase | Goal | Requirements | Status |
|-------|------|--------------|--------|
| 33 - Traversal-Time Chain Detection | Extend LinearDetector to track cluster offsets, validate contiguity, and instrument chain detection | CL-01, CL-03 | In Progress (3/4 plans) |
| 34 - Sequential Cluster Reader | Read all clusters for a chain in single I/O operation | CL-02 | Pending |
| 35 - Contiguity Validation and Fallback | Validate cluster contiguity and fall back immediately when pattern breaks | CL-04 | Pending |
| 36 - IO-12 Validation | Verify MVCC isolation preserved and Chain(500) <=75ms target achieved | CL-05 | Pending |

## Performance Metrics

**Velocity:**
- Total plans completed: 112
- Average duration: 7 min
- Total execution time: ~12.7 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| v1.0 (1-10) | 33 | ~3 days | ~5 min |
| v1.1 (11-22) | 42 | ~4 days | ~7 min |
| v1.2 (23-24) | 7 | 1 day | ~7 min |
| v1.3 (25-28) | 16 | ~30 min | ~7 min |
| v1.4 (29-32) | 12 | ~13 min | ~3 min |
| v1.6 (33-36) | 3 | ~9 min | ~3 min (so far) |

**Recent Trend:**
- Last 5 plans: ~5 min each
- Trend: Stable

*Updated after each plan completion*

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

### Pending Todos

v1.6 Chain Locality:
- [x] Phase 33 Plan 01: Cluster offset tracking (completed)
- [x] Phase 33 Plan 02: Contiguity validation (completed)
- [x] Phase 33 Plan 04: Chain detection instrumentation (completed)
- [ ] Phase 33 Plan 03: Integration into traversal hot paths
- [ ] Phase 34: Sequential cluster reader implementation
- [ ] Phase 35: Contiguity validation and fallback handling
- [ ] Phase 36: IO-12 validation (verify Chain(500) <=75ms target)

### Blockers/Concerns

- v1.6: Surgical scope requires discipline — avoid adding write-time allocation or metadata storage
- v1.6: Cluster contiguity validation must be robust to avoid performance regression on non-contiguous data

## Session Continuity

Last session: 2026-01-21
Stopped at: Completed 33-04 chain detection instrumentation
Resume file: None

### Roadmap Evolution

- **v0.2 Foundation** (2026-01-17): Phases 1-7 complete
- **v1.0 Production** (2026-01-17): Phases 8-10 complete
- **v1.1 ACID & Reliability** (2026-01-20): Phases 11-22 complete
- **v1.2 Benchmark Infrastructure** (2026-01-21): Phases 23-24 complete
- **v1.3 Chain Traversal Performance** (2026-01-21): Phases 25-28 complete
- **v1.4 Sequential I/O Optimization** (2026-01-21): Phases 29-32 complete (IO-13 satisfied, IO-12 deferred)
- **v1.6 Chain Locality** (2026-01-21): Phases 33-36 planned (surgical traversal-time approach)
  - Phase 33 Plan 01 (2026-01-21): Cluster offset tracking complete
  - Phase 33 Plan 02 (2026-01-21): Contiguity validation complete
  - Phase 33 Plan 04 (2026-01-21): Chain detection instrumentation complete

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-21)

**Core value:** Feature parity, performance, and reliability equally. Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.
**Current focus:** v1.4 milestone - Sequential I/O Optimization

## Current Position

Phase: 32 of 32 planned (3 plans + 1 extension) — In progress
Milestone: v1.4 Sequential I/O Optimization
Status: Phase 32 Plan 01 complete (checkpoint - needs Plan 32-04), Plan 02 complete, Plan 03 pending
Last activity: 2026-01-21 — Completed 32-01: Performance benchmark execution (Chain(500) = 10.90x SQLite, needs L1 buffer neighbor extraction)

Progress: [█████████░] 97.2% (31/32 phases complete, 104/107 plans)

## v1.4 Milestone Goals

**Problem:** Chain traversals have 11x performance gap vs SQLite. v1.3's per-traversal cache is ineffective (0% hit rate) because linear chains never revisit nodes.

**Root Cause:** Each hop reads a 4KB slot, decodes adjacency, extracts neighbor ID, then drops everything before the next hop. This "read-drop-repeat" is pathological for linear access patterns.

**Solution:** Sequential I/O coalescing:
1. Detect linear patterns (degree <= 1 for 3+ consecutive steps)
2. Batch-read sequential slots (8 slots, 32KB in one I/O)
3. Keep decoded slots alive across hops (traversal-scoped buffer)

**Expected Impact:** ~10x improvement for chain traversals (11x gap -> <=3x acceptable gap)

## Performance Metrics

**Velocity:**
- Total plans completed: 103
- Average duration: 7 min
- Total execution time: ~11.6 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| v1.0 (1-10) | 33 | ~3 days | ~5 min |
| v1.1 (11-22) | 42 | ~4 days | ~7 min |
| v1.2 (23-24) | 7 | 1 day | ~7 min |
| v1.3 (25-28) | 16 | ~30 min | ~7 min |
| v1.4 (29-32) | 12 | ~13 min | ~3 min |

**Recent Trend:**
- Last 5 plans: ~6 min each
- Trend: Stable

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- v1.3: Per-traversal cache selected over global cache (preserves MVCC isolation)
- v1.3: HashMap<NodeId, Vec<NodeId>> chosen to avoid Arc<NodeRecord> cycles
- v1.3: Benchmark fix must precede cache implementation (need accurate baseline)
- v1.3: Chain graphs have 0% cache hit rate by design - per-traversal cache provides no benefit for pure linear traversals
- v1.4: Sequential I/O coalescing is the correct approach for chain optimization (based on research)
- v1.4: Traversal-scoped buffers (not global) to preserve MVCC isolation
- v1.4: 3-step linear detection threshold to avoid false positives on trees
- v1.4: 8-slot prefetch window (32KB) based on RocksDB/LMDB research
- v1.4 Phase 31-01: L1 buffer lookup is instrumentation-only - records hit/miss but falls through to L2/L3 to avoid scope creep; full neighbor extraction from buffered NodeRecordV2 deferred to Phase 32
- v1.4 Phase 31-02: BFS functions preserve pointer table fast paths - these optimizations bypass cache entirely and are orthogonal to sequential I/O optimization
- v1.4 Phase 31-03: Edge-type filtered chain queries bypass optimization - filtered traversals don't benefit from sequential I/O patterns and continue using AdjacencyHelpers directly
- v1.4 Phase 32-02: TraversalContext and SequentialReadBuffer preserve MVCC isolation - ownership model guarantees buffer evaporation on function return; 13 tests confirm no cross-traversal pollution (IO-13 SATISFIED)
- v1.4 Phase 32-01: Benchmark results show Chain(500) = 10.90x SQLite (target: 3x). Root cause: L1 buffer lookup is instrumentation-only; full neighbor extraction from buffered NodeRecordV2 was deferred from Phase 31. Decision: Extend Phase 32 with Plan 32-04 to implement actual L1 buffer neighbor extraction.

### Pending Todos

v1.4 Sequential I/O Optimization:
- [x] Phase 29 Plan 01: Implement LinearDetector state machine (4-state FSM, 13 tests passing)
- [x] Phase 29 Plan 02: Module exports for LinearDetector (accessible from adjacency and graph_ops)
- [x] Phase 30 Plan 01: Implement NodeStore::read_slots_batch() method (batch I/O, 32KB in single syscall)
- [x] Phase 30 Plan 02: Create SequentialReadBuffer module with prefetch logic (AHashMap storage, 8 tests passing)
- [x] Phase 30 Plan 03: Add unit tests for batch reading and buffer correctness (10 unit tests, 8 integration tests)
- [x] Phase 31 Plan 01: TraversalContext struct and get_neighbors_optimized() function (3-tier lookup, 11 tests passing)
- [x] Phase 31 Plan 02: BFS TraversalContext integration (all three BFS variants use pattern detection and 3-tier lookup)
- [x] Phase 31 Plan 03: Traversal hot path integration (k-hop, shortest path, chain queries with direction-aware pattern detection)
- [x] Phase 32 Plan 01: Execute performance benchmarks and validate 3x SQLite target (IO-12) - RESULTS: 10.90x SQLite, needs Plan 32-04
- [x] Phase 32 Plan 02: Create MVCC isolation tests for TraversalContext (IO-13) - 13 tests passing
- [ ] Phase 32 Plan 03: Prefetch window tuning and memory overhead documentation
- [ ] Phase 32 Plan 04: Implement L1 buffer neighbor extraction (NEW - to achieve IO-12 3x target)

### Blockers/Concerns

- None identified for v1.4

## Session Continuity

Last session: 2026-01-21
Stopped at: Completed 32-01 (benchmark execution, Chain(500) = 10.90x SQLite) and 32-02 (MVCC isolation tests)
Resume file: None

### Roadmap Evolution

- **v0.2 Foundation** (2026-01-17): Phases 1-7 complete
- **v1.0 Production** (2026-01-17): Phases 8-10 complete
- **v1.1 ACID & Reliability** (2026-01-20): Phases 11-22 complete
- **v1.2 Benchmark Infrastructure** (2026-01-21): Phases 23-24 complete
- **v1.3 Chain Traversal Performance** (2026-01-21): Phases 25-28 complete ✅
- **v1.4 Sequential I/O Optimization** (2026-01-21): Phases 29-32 planned
  - Linear pattern detection (29-01, 29-02, 29-03 complete)
  - Sequential slot reading (30-01, 30-02, 30-03 complete)
  - Traversal integration (31-01, 31-02, 31-03 complete)
  - Validation and tuning (32-01 complete, 32-02 complete, 32-03 pending, 32-04 added)

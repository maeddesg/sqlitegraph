# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-21)

**Core value:** Feature parity, performance, and reliability equally. Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.
**Current focus:** v1.4 milestone - Sequential I/O Optimization

## Current Position

Phase: 32 of 32 planned (6 plans) — Complete (checkpoint decision reached)
Milestone: v1.4 Sequential I/O Optimization
Status: Phase 32 complete (all 6 plans done), checkpoint at 32-06 with Option A decision
Last activity: 2026-01-21 — Completed 32-06: Performance validation checkpoint (IO-12 remains OPEN)

Progress: [█████████░] 97.2% (31/32 phases complete, 108/109 plans complete, v1.4 milestone complete, IO-12 deferred to v1.5/v1.6)

## v1.4 Milestone Goals

**Problem:** Chain traversals have 11x performance gap vs SQLite. v1.3's per-traversal cache is ineffective (0% hit rate) because linear chains never revisit nodes.

**Root Cause:** Each hop reads a 4KB slot, decodes adjacency, extracts neighbor ID, then drops everything before the next hop. This "read-drop-repeat" is pathological for linear access patterns.

**Solution Implemented:** Sequential I/O coalescing:
1. Detect linear patterns (degree <= 1 for 3+ consecutive steps) - DONE (Phase 29)
2. Batch-read sequential slots (8 slots, 32KB in one I/O) - DONE (Phase 30)
3. Keep decoded slots alive across hops (traversal-scoped buffer) - DONE (Phase 31)
4. Extract neighbors from buffered slots - DONE (Plan 32-04)
5. Prefetch edge clusters - DONE (Plan 32-05)

**Result:** Chain(500) = 248.68ms (9.96x SQLite). 8.7% improvement from v1.3 baseline (255.29ms), but IO-12 target (3x) NOT achieved.

**v1.4 Outcomes:**
- IO-13 SATISFIED: MVCC isolation preserved (13 tests passing)
- IO-12 REMAINS OPEN: 9.96x gap requires sequential cluster storage (v1.5/v1.6)

**Root Cause of Remaining Gap:** Layout mismatch. Edge clusters for sequential chains are not stored contiguously on disk. Prefetching non-contiguous clusters is random I/O, not sequential I/O.

## v1.5/v1.6: Sequential Cluster Storage (Planned)

**Decision from 32-06 checkpoint:** Option A - Sequential cluster storage for chains only

**Framing:** NOT full v2.0 Memory Substrate, but scoped Native V2 layout optimization:
- Store edge clusters contiguously for chains only
- Keep general graph storage unchanged
- Gate behind format/version flag (backward-compatible)
- Apply only when linear traversal is detected

**Goal:** Achieve IO-12 target (Chain(500) <=75ms, 3x SQLite) by aligning storage layout with access pattern.

## Performance Metrics

**Velocity:**
- Total plans completed: 108
- Average duration: 7 min
- Total execution time: ~12.4 hours

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
- v1.4 Phase 32-03: Prefetch window tuning results show window 8 (32KB) provides optimal throughput/cost ratio. Memory overhead documented at ~2-3KB per traversal (evaporates on return). Window 4 is 35% faster but caches 50% fewer nodes; Window 16+ shows diminishing returns.
- v1.4 Phase 32-04: L1 buffer neighbor extraction implemented in get_neighbors_optimized() using EdgeCluster::deserialize to read adjacency from buffered NodeRecordV2. 8 unit tests verify outgoing/incoming direction handling, buffer miss fallback, and empty cluster handling.
- v1.4 Phase 32-05: Edge cluster prefetching and caching implemented in SequentialReadBuffer. prefetch_clusters_from() prefetches node slots AND edge clusters. get_neighbors_optimized() checks cluster cache before file I/O. 5 unit tests verify cluster cache hit/miss behavior.
- v1.4 Phase 32-06 CHECKPOINT: Benchmark results show Chain(500) = 248.68ms (9.96x SQLite). Cluster prefetching provides 8.7% improvement vs v1.3 but IO-12 target (3x) NOT achieved. Root cause: Layout mismatch - edge clusters for sequential chains are not stored contiguously on disk. Prefetching non-contiguous clusters is random I/O, not sequential I/O.
- v1.4 Phase 32-06 DECISION: Option A selected - Sequential cluster storage for chains only (scoped Native V2 layout optimization). Framing: v1.5/v1.6 (NOT full v2.0 Memory Substrate). Store edge clusters contiguously for chains, keep general graphs unchanged, gate behind format/version flag. IO-12 REMAINS OPEN until v1.5/v1.6 implementation.

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
- [x] Phase 32 Plan 03: Prefetch window tuning and memory overhead documentation - Window 8 confirmed as optimal, ~2-3KB overhead
- [x] Phase 32 Plan 04: L1 buffer neighbor extraction with unit tests - 8 tests passing
- [x] Phase 32 Plan 05: Edge cluster prefetching and caching - 5 tests passing
- [x] Phase 32 Plan 06: Performance validation checkpoint - Option A decision, IO-12 remains OPEN

v1.5/v1.6 Sequential Cluster Storage (Planned):
- [ ] Phase 33: Chain region detection (identify linear chain regions during graph construction)
- [ ] Phase 34: Contiguous cluster allocation (allocate sequential cluster storage for chains)
- [ ] Phase 35: Format version negotiation (backward-compatible flag for chain-local layout)
- [ ] Phase 36: IO-12 validation (verify Chain(500) <=75ms with sequential cluster storage)

### Blockers/Concerns

- v1.4: IO-12 target (3x SQLite) not achieved - 9.96x gap due to layout mismatch. Deferred to v1.5/v1.6 for sequential cluster storage implementation.

## Session Continuity

Last session: 2026-01-21
Stopped at: Completed 32-06 checkpoint with Option A decision (sequential cluster storage for v1.5/v1.6)
Resume file: None

### Roadmap Evolution

- **v0.2 Foundation** (2026-01-17): Phases 1-7 complete
- **v1.0 Production** (2026-01-17): Phases 8-10 complete
- **v1.1 ACID & Reliability** (2026-01-20): Phases 11-22 complete
- **v1.2 Benchmark Infrastructure** (2026-01-21): Phases 23-24 complete
- **v1.3 Chain Traversal Performance** (2026-01-21): Phases 25-28 complete
- **v1.4 Sequential I/O Optimization** (2026-01-21): Phases 29-32 complete (IO-13 satisfied, IO-12 deferred to v1.5/v1.6)
  - Linear pattern detection (29-01, 29-02 complete)
  - Sequential slot reading (30-01, 30-02, 30-03 complete)
  - Traversal integration (31-01, 31-02, 31-03 complete)
  - Validation and tuning (32-01 through 32-06 complete - Option A decision for v1.5/v1.6)
- **v1.5/v1.6 Sequential Cluster Storage** (planned): Layout optimization for chain locality to achieve IO-12 target

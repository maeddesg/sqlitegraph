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

**Phase 65 Summary:**
- V3WALRecord enum with 8 variants (page ops + transaction control)
- V3WALHeader with 64-byte fixed format and manual serialization
- WALRecovery engine with sequential replay and page cache
- WALWriter with buffered writes and fsync durability
- 35 unit tests (all passing)
- 1,751 LOC in src/backend/native/v3/wal.rs

## Progress

**Execution Order:**
Phases execute in numeric order: 58 → 59 → 60 → 61 → 62

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 45-57 | v1.3.0 Graph Algorithms Library | 36/36 | Complete | 2026-02-03 |
| 58-62 | v1.5 Code Quality & Features | 14/14 | Complete | 2026-02-12 |

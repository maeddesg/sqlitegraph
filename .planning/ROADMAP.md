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
- **v1.6 Chain Locality** — Phases 33-36 (current)

---

## Phases

<details>
<summary>v0.2-v1.4 Archive</summary>

See milestone archives for complete history.
- v0.2 Foundation: Phases 1-7
- v1.0 Production: Phases 8-10
- v1.1 ACID & Reliability: Phases 11-22
- v1.2 Benchmark Infrastructure: Phases 23-24
- v1.3 Chain Traversal Performance: Phases 25-28
- v1.4 Sequential I/O Optimization: Phases 29-32

</details>

---

## v1.6 Chain Locality (Phases 33-36)

**Milestone Goal:** Achieve IO-12 target (Chain(500) <=75ms, 3x SQLite) through traversal-time sequential cluster reads.

**Background:** v1.4 achieved linear pattern detection and sequential slot reading. However, edge clusters for sequential chains are stored non-contiguously in the global cluster pool. The IO-12 target (9.96x gap) remains unmet because prefetching non-contiguous clusters is still random I/O.

**Surgical Solution:** Traversal-time sequential cluster reads. Detect chains during traversal (not at write time), read all clusters in single I/O when chain confirmed, fall back immediately when pattern breaks. No write-time allocation, no migration, no metadata storage.

**Why surgical:** Write-time detection risks false positives and schema debt. Traversal-time approach is reversible, honest, and closes IO-12 without collateral damage.

### Phase 33: Traversal-Time Chain Detection
**Goal:** Traversal detects linear chains and switches to sequential cluster reads
**Depends on**: Phase 32 (v1.4 complete)
**Requirements:** CL-01
**Plans:** TBD

**Success Criteria:**
1. LinearDetector tracks cluster offsets during traversal to identify potential chains
2. After confirming degree <= 1 pattern for N consecutive nodes, traversal triggers sequential read path
3. Detection threshold (N) is configurable and validated against false positives on trees
4. Chain detection instrumentation reports chains found and average chain length

**Key deliverables:**
- Extended LinearDetector with cluster offset tracking
- Chain confirmation logic (degree <= 1 validation)
- Configurable detection threshold
- Unit tests for chain detection on various graph patterns

**Avoids:**
- Write-time detection (detects at traversal time only)
- False positives on tree structures
- Committing to chain layout before validation

### Phase 34: Sequential Cluster Reader
**Goal:** Sequential cluster reader reads all clusters for a chain in single I/O operation
**Depends on**: Phase 33
**Requirements:** CL-02
**Plans:** TBD

**Success Criteria:**
1. SequentialClusterReader reads all edge clusters for a confirmed chain in single I/O
2. Buffered clusters are stored in traversal-scoped memory (evaporates on return)
3. Neighbor extraction from buffered clusters matches existing get_neighbors() semantics
4. Memory overhead is bounded and documented

**Key deliverables:**
- SequentialClusterReader struct with read_chain_clusters() method
- Cluster buffer allocation and management
- Neighbor extraction from buffered clusters
- Unit tests for cluster reading and neighbor extraction

**Avoids:**
- Persistent cluster caching (traversal-scoped only)
- Cross-traversal pollution
- Unbounded memory growth

### Phase 35: Contiguity Validation and Fallback
**Goal:** LinearDetector validates cluster contiguity and falls back immediately when pattern breaks
**Depends on**: Phase 34
**Requirements:** CL-03, CL-04
**Plans:** TBD

**Success Criteria:**
1. LinearDetector validates cluster contiguity before committing to sequential read path
2. When clusters are not contiguous, traversal falls back immediately to standard path
3. Fallback happens within the same traversal (no restart required)
4. Pattern breaks (degree > 1, branching) trigger immediate fallback

**Key deliverables:**
- Cluster contiguity validation logic
- Fallback path integration
- Unit tests for non-contiguous cluster handling
- Integration tests for tree/diamond graph patterns

**Avoids:**
- Using sequential path on non-contiguous clusters
- Performance degradation from false positive chain detection
- Complex fallback state machines

### Phase 36: IO-12 Validation
**Goal:** MVCC isolation preserved and IO-12 target achieved
**Depends on**: Phase 35
**Requirements:** CL-05
**Plans:** TBD

**Success Criteria:**
1. Chain(500) traversal achieves <=75ms (3x SQLite baseline of ~22ms)
2. MVCC snapshot isolation preserved (no cross-traversal staleness)
3. Star and random graph traversals do not regress (within 10% of v1.4 baseline)
4. Chain optimization metrics available for instrumentation

**Key deliverables:**
- Performance benchmarks (chain, star, random) with cold/warm numbers
- MVCC isolation tests for sequential cluster reads
- Updated documentation with expected speedups
- Instrumentation metrics for chain optimization

**Avoids:**
- Regressing non-chain graph performance
- Breaking MVCC isolation guarantees
- Hiding performance numbers

---

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → ... → 32 → 33 → 34 → 35 → 36

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-32 | v0.2-v1.4 | 109/109 | Complete | 2026-01-21 |
| 33. Traversal-Time Chain Detection | v1.6 | 0/TBD | Not Started | — |
| 34. Sequential Cluster Reader | v1.6 | 0/TBD | Not Started | — |
| 35. Contiguity Validation and Fallback | v1.6 | 0/TBD | Not Started | — |
| 36. IO-12 Validation | v1.6 | 0/TBD | Not Started | — |

**Overall Progress:** 109/109 plans complete through v1.4. v1.6 planned for IO-12 completion.

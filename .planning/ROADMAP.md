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
**Requirements:** CL-01 ✓ SATISFIED, CL-03 ✓ SATISFIED
**Plans:** 5/5 complete (extend LinearDetector with cluster offset tracking, contiguity validation, sequential read trigger, instrumentation, integration tests)

**Success Criteria:**
1. LinearDetector tracks cluster offsets during traversal to identify potential chains
2. After confirming degree <= 1 pattern for N consecutive nodes, traversal triggers sequential read path
3. Detection threshold (N) is configurable and validated against false positives on trees
4. Chain detection instrumentation reports chains found and average chain length
5. **LinearDetector validates cluster contiguity before committing to sequential read path** (CL-03)

**Key deliverables:**
- Extended LinearDetector with cluster offset tracking
- Chain confirmation logic (degree <= 1 validation)
- Cluster contiguity validation (are_clusters_contiguous(), validate_contiguity())
- Configurable detection threshold
- Unit tests for chain detection on various graph patterns

**Avoids:**
- Write-time detection (detects at traversal time only)
- False positives on tree structures
- Committing to chain layout before validation

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

**Success Criteria:**
1. SequentialClusterReader reads all edge clusters for a confirmed chain in single I/O
2. Buffered clusters are stored in traversal-scoped memory (evaporates on return)
3. ~~Neighbor extraction from buffered clusters matches existing get_neighbors() semantics~~ (deferred to Phase 35)
4. Memory overhead is bounded and documented

**Scope Note:** Phase 34-35 together deliver the full sequential cluster reader capability. Phase 34 implements the read trigger and buffer storage. Phase 35 adds neighbor extraction from buffer with proper node_id -> cluster_index mapping.

**Key deliverables:**
- SequentialClusterReader struct with read_chain_clusters() method
- Cluster buffer allocation and management in TraversalContext
- Sequential read trigger in get_neighbors_optimized()
- ~~Neighbor extraction from buffered clusters~~ (deferred to Phase 35)
- Unit tests for cluster reading and buffer storage

**Avoids:**
- Persistent cluster caching (traversal-scoped only)
- Cross-traversal pollution
- Unbounded memory growth

**Plans:**
- [x] 34-01-PLAN.md — Create SequentialClusterReader module with read_chain_clusters() method
- [x] 34-02-PLAN.md — Add cluster buffer fields to TraversalContext
- [x] 34-03-PLAN.md — Integrate sequential cluster read into get_neighbors_optimized()

### Phase 35: Neighbor Extraction and Fallback
**Goal:** Extract neighbors from cluster buffer and fall back immediately when pattern breaks
**Depends on**: Phase 34
**Requirements:** CL-02 (completion), CL-04
**Plans:** 4 plans

**Success Criteria:**
1. Neighbors are extracted from cluster_buffer using node_id -> cluster_index mapping
2. ~~LinearDetector validates cluster contiguity before committing to sequential read path~~ (completed in Phase 33)
3. When clusters are not contiguous, traversal falls back immediately to standard path
4. Fallback happens within the same traversal (no restart required)
5. Pattern breaks (degree > 1, branching) trigger immediate fallback

**Key deliverables:**
- Node_id -> cluster_index mapping in TraversalContext
- Neighbor extraction from cluster_buffer in get_neighbors_optimized()
- ~~Cluster contiguity validation logic~~ (completed in Phase 33)
- Fallback path integration
- Unit tests for neighbor extraction and non-contiguous cluster handling
- Integration tests for tree/diamond graph patterns

**Avoids:**
- Using sequential path on non-contiguous clusters
- Performance degradation from false positive chain detection
- Complex fallback state machines

**Plans:**
- [x] 35-01-PLAN.md — Add node_cluster_index field to TraversalContext
- [x] 35-02-PLAN.md — Extract neighbors from cluster_buffer using mapping
- [x] 35-03-PLAN.md — Add traverse_with_detection helper and unit tests
- [x] 35-04-PLAN.md — Integration tests for extraction and fallback

### Phase 36: IO-12 Validation
**Goal:** MVCC isolation preserved and IO-12 target achieved
**Depends on**: Phase 35
**Requirements:** CL-05
**Plans:** 4 plans

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

**Plans:**
- [ ] 36-01-PLAN.md — Create Criterion benchmark suite for IO-12 validation
- [ ] 36-02-PLAN.md — Validate MVCC isolation for sequential cluster reads
- [ ] 36-03-PLAN.md — Run benchmarks and document IO-12 status
- [ ] 36-04-PLAN.md — Update documentation with Phase 36 completion

---

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → ... → 32 → 33 → 34 → 35 → 36

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-32 | v0.2-v1.4 | 109/109 | Complete | 2026-01-21 |
| 33. Traversal-Time Chain Detection | v1.6 | 5/5 | Complete | 2026-01-21 |
| 34. Sequential Cluster Reader | v1.6 | 3/3 | Complete | 2026-01-21 |
| 35. Neighbor Extraction and Fallback | v1.6 | 4/4 | Complete | 2026-01-21 |
| 36. IO-12 Validation | v1.6 | 0/4 | Not Started | — |

**Overall Progress:** 125/129 plans planned and executed. v1.6 in progress (3/4 phases complete, 1/4 not started).

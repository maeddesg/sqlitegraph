# Roadmap: SQLiteGraph

## Overview

SQLiteGraph roadmap from v1.0 through current milestone. Continuous phase numbering across all milestones — phases 1-44 completed in prior milestones, phases 45-57 planned for v1.14 Graph Algorithms Library.

## Milestones

- ✅ **v1.0 MVP** - Phases 1-10 (shipped 2026-01-17)
- ✅ **v1.1 ACID & Reliability** - Phases 11-22 (shipped 2026-01-20)
- ✅ **v1.2 Benchmark Infrastructure** - Phases 23-24 (shipped 2026-01-21)
- ✅ **v1.3 Chain Traversal Performance** - Phases 25-29 (shipped 2026-01-21)
- ✅ **v1.4 Sequential I/O Optimization** - Phases 30-32 (shipped 2026-01-21)
- ✅ **v1.6 Chain Locality** - Phases 33-36 (shipped 2026-01-21)
- ✅ **v1.13 Pub/Sub** - Phases 37-44 (shipped 2026-01-26)
- 🚧 **v1.14 Graph Algorithms Library** - Phases 45-57 (in progress)

## Phases

<details>
<summary>v0.2-v1.13 Archive</summary>

See milestone archives for complete history.
- v0.2 Foundation: Phases 1-7
- v1.0 Production: Phases 8-10
- v1.1 ACID & Reliability: Phases 11-22
- v1.2 Benchmark Infrastructure: Phases 23-24
- v1.3 Chain Traversal Performance: Phases 25-29
- v1.4 Sequential I/O Optimization: Phases 30-32
- v1.6 Chain Locality: Phases 33-36
- v1.13 Pub/Sub: Phases 37-44

</details>

---

### 🚧 v1.14 Graph Algorithms Library (In Progress)

**Milestone Goal:** Implement comprehensive graph algorithms library for CFG analysis, program slicing, and general graph reasoning.

#### Phase 45: Core Graph Theory
**Goal**: Fundamental graph decomposition and ordering algorithms
**Depends on**: Phase 44 (v1.13 Pub/Sub complete)
**Requirements**: GRT-01, GRT-02, GRT-03, GRT-04, GRT-05
**Success Criteria** (what must be TRUE):
  1. User can compute Strongly Connected Components with Tarjan's algorithm, receiving components, node-to-component mapping, and condensed DAG
  2. User can compute topological sort on DAGs and receive CycleDetected error with cycle path when cycles exist
  3. User can compute Weakly Connected Components for undirected connectivity analysis
  4. User can compute transitive reduction to remove redundant edges from DAGs
  5. User can compute transitive closure for all-pairs reachability with bounded computation and caching
**Plans**: 5 plans in 2 waves
  - [x] 45-01-PLAN.md — Weakly Connected Components (Wave 1)
  - [x] 45-02-PLAN.md — Strongly Connected Components / Tarjan (Wave 1)
  - [x] 45-03-PLAN.md — Transitive Closure (Wave 1)
  - [x] 45-04-PLAN.md — Transitive Reduction (Wave 2, depends on 45-03)
  - [x] 45-05-PLAN.md — Topological Sort with Cycle Detection + Benchmarks (Wave 2, depends on 45-02)

#### Phase 46: Reachability & Slicing
**Goal**: Forward and backward reachability queries
**Depends on**: Phase 45
**Requirements**: RCH-01, RCH-02, RCH-03, RCH-04
**Success Criteria** (what must be TRUE):
  1. User can compute forward reachability from a start node to answer "what does this affect?" queries
  2. User can compute backward reachability to a target node to answer "what affects this?" queries
  3. User can perform point-to-point reachability checks with efficient can_reach(from, to) query
  4. User can find unreachable nodes from entry point for dead code detection
**Plans**: 1 plan in 1 wave
  - [x] 46-01-PLAN.md — Forward/Backward Reachability, Point-to-Point, Unreachable Nodes (Wave 1)

#### Phase 47: Core CFG Algorithms
**Goal**: Dominator and post-dominator computation for control flow analysis
**Depends on**: Phase 46
**Requirements**: CFG-01, CFG-02, CFG-03
**Success Criteria** (what must be TRUE):
  1. User can compute dominators using Cooper et al. simple_fast algorithm, receiving dominator sets and immediate dominator tree
  2. User can compute post-dominators on reversed graph, receiving post-dominator sets and immediate post-dominator tree
  3. User can compute Control Dependence Graph derived from post-dominators for "this block executes because of that condition" explanations
**Plans**: 3 plans in 3 waves
  - [x] 47-01-PLAN.md — Dominators (Cooper et al. simple_fast algorithm) (Wave 1)
  - [x] 47-02-PLAN.md — Post-Dominators (reversed graph dominators) (Wave 2, depends on 47-01)
  - [x] 47-03-PLAN.md — Control Dependence Graph (from post-dominators) (Wave 3, depends on 47-02)

#### Phase 48: Derived CFG Algorithms
**Goal**: Dominance frontiers and natural loop detection
**Depends on**: Phase 47
**Requirements**: DCFG-01, DCFG-02
**Success Criteria** (what must be TRUE):
  1. User can compute dominance frontiers for all nodes using Cytron et al. efficient algorithm, supporting iterated dominance frontier for SSA phi-placement
  2. User can detect natural loops by finding back-edges where head dominates tail, receiving loop headers, back-edges, and loop bodies with nested loop detection
**Plans**: 2 plans in 2 waves
  - [x] 48-01-PLAN.md — Dominance Frontiers (Cytron et al. algorithm) (Wave 1)
  - [x] 48-02-PLAN.md — Natural Loops (back-edge detection) (Wave 2, depends on 48-01)

#### Phase 49: Path Analysis
**Goal**: Execution path enumeration with feasibility pruning
**Depends on**: Phase 48
**Requirements**: PATH-01, PATH-02
**Success Criteria** (what must be TRUE):
  1. User can enumerate execution paths with DFS, cycle detection, and bounds (max depth, max paths, revisit cap), receiving path classifications (Normal, Error, Degenerate, Infinite)
  2. User can apply dominance constraints to path enumeration to prune impossible branch combinations and reduce path explosion
**Plans**: TBD

#### Phase 50: Dependency & Build Systems
**Goal**: Critical path and cycle analysis for dependency graphs
**Depends on**: Phase 45
**Requirements**: DEP-01, DEP-02
**Success Criteria** (what must be TRUE):
  1. User can compute critical path in DAG using longest path computation (not shortest) to identify bottlenecks in dependency chains, supporting weighted edges
  2. User can compute minimal cycle basis that explains "why" not just "that" for cycles, with bounded enumeration
**Plans**: TBD

#### Phase 51: Program Analysis & Tooling
**Goal**: Program slicing and call graph analysis
**Depends on**: Phase 46
**Requirements**: PROG-01, PROG-02, PROG-03
**Success Criteria** (what must be TRUE):
  1. User can perform backward program slicing to answer "what can affect this node?" for bug isolation and refactoring safety
  2. User can perform forward program slicing to answer "what does this node affect?" for impact analysis
  3. User can collapse SCCs in call graphs to merge mutual recursion into supernodes, making call graphs readable and analyses tractable
**Plans**: TBD

#### Phase 52: Databases & Distributed Systems
**Goal**: Cut computation and graph partitioning
**Depends on**: Phase 46
**Requirements**: DIST-01, DIST-02, DIST-03
**Success Criteria** (what must be TRUE):
  1. User can compute minimum cut (smallest edge cut between source and target) for fault tolerance and security boundary analysis
  2. User can compute minimum vertex cut (smallest node cut between source and target)
  3. User can partition graphs using greedy, BFS-based, and size-bounded partitioning for sharding and locality optimization
**Plans**: TBD

#### Phase 53: Observability & Runtime
**Goal**: Runtime event ordering and impact analysis
**Depends on**: Phase 46
**Requirements**: OBS-01, OBS-02
**Success Criteria** (what must be TRUE):
  1. User can perform happens-before analysis for event ordering in traces with lightweight race detection hints
  2. User can compute impact radius using bounded reachability with weights for blast zone estimation
**Plans**: TBD

#### Phase 54: ML / Inference / Compute Graphs
**Goal**: Pattern matching and graph isomorphism
**Depends on**: Phase 45
**Requirements**: ML-01, ML-02, ML-03
**Success Criteria** (what must be TRUE):
  1. User can find subgraph patterns using bounded subgraph isomorphism for common subexpression detection
  2. User can rewrite patterns with graph rewriting support for compiler and ML framework optimization
  3. User can compute structural similarity using practical isomorphism check for regression detection and refactor verification
**Plans**: TBD

#### Phase 55: Graph Diff
**Goal**: Structural and semantic graph comparison
**Depends on**: Phase 45
**Requirements**: DIFF-01, DIFF-02
**Success Criteria** (what must be TRUE):
  1. User can compute structural graph delta comparing two snapshots, receiving nodes/edges added/removed and structural similarity score
  2. User can validate refactors with graph diff to answer "did I break anything structural?" and verify optimizer equivalence
**Plans**: TBD

#### Phase 56: Security & Compliance
**Goal**: Taint propagation for security analysis
**Depends on**: Phase 46
**Requirements**: SEC-01
**Success Criteria** (what must be TRUE):
  1. User can propagate taint on graph from sources, performing sink reachability analysis for security and compliance tooling
**Plans**: TBD

#### Phase 57: CLI Commands
**Goal**: Command-line interface for all graph algorithms
**Depends on**: Phases 45-56 (all algorithm phases)
**Requirements**: CLI-01, CLI-02
**Success Criteria** (what must be TRUE):
  1. User can invoke all 26 graph algorithms via CLI with consistent interface and backend selection support
  2. CLI commands show progress tracking for long-running algorithms consistent with existing progress infrastructure
**Plans**: TBD

---

## Progress

**Execution Order:**
Phases execute in numeric order: 45 → 46 → 47 → ... → 57

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-44 | v0.2-v1.13 | 182/182 | Complete | 2026-02-02 |
| 45. Core Graph Theory | v1.14 | 5/5 | Complete | 2026-02-02 |
| 46. Reachability & Slicing | v1.14 | 1/1 | Complete | 2026-02-02 |
| 47. Core CFG Algorithms | v1.14 | 3/3 | Complete | 2026-02-02 |
| 48. Derived CFG Algorithms | v1.14 | 0/2 | Not started | - |
| 49. Path Analysis | v1.14 | 0/TBD | Not started | - |
| 50. Dependency & Build Systems | v1.14 | 0/TBD | Not started | - |
| 51. Program Analysis & Tooling | v1.14 | 0/TBD | Not started | - |
| 52. Databases & Distributed Systems | v1.14 | 0/TBD | Not started | - |
| 53. Observability & Runtime | v1.14 | 0/TBD | Not started | - |
| 54. ML / Inference / Compute Graphs | v1.14 | 0/TBD | Not started | - |
| 55. Graph Diff | v1.14 | 0/TBD | Not started | - |
| 56. Security & Compliance | v1.14 | 0/TBD | Not started | - |
| 57. CLI Commands | v1.14 | 0/TBD | Not started | - |

**Overall Progress:** 182/182 plans complete for v0.2-v1.13. Phase 45 complete (5/5 plans). Phase 46 complete (1/1 plan). Phase 47 complete (3/3 plans). v1.14: 9/191 plans complete (4.7%).

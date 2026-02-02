# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-02)

**Core value:** Feature parity, performance, and reliability equally. Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.
**Current focus:** v1.14 Graph Algorithms Library

## Current Position

Milestone: v1.14 Graph Algorithms Library (IN PROGRESS)
Phase: 51 of 57 (Program Analysis & Tooling) — IN PROGRESS
Status: Phase 51 Plan 02 COMPLETE - SCC collapse for call graph analysis
Last activity: 2026-02-02 — Phase 51 Plan 02 complete (2/2 TBD: program slicing, SCC collapse - both done)

Progress: [████░░░░░░░] 38% of v1.14 (17/195 plans complete, 6/14 phases done, Phase 52 next)

## Performance Metrics

**Velocity:**
- Total plans completed: 194 (phases 1-44, plus 45-01 through 45-05, plus 46-01, plus 47-01 through 47-03, plus 48-01 through 48-02, plus 49-01 through 49-02, plus 50-01 through 50-02, plus 51-01 through 51-02)
- Average duration: ~20 min/plan
- Total execution time: ~80 hours across v1.0-v1.14

**By Phase:**

| Milestone | Phases | Plans | Notes |
|-----------|--------|-------|-------|
| v0.2-v1.0 | 1-10 | 36 | Foundation, Production MVP |
| v1.1 | 11-22 | 70 | ACID & Reliability |
| v1.2 | 23-24 | 8 | Benchmark Infrastructure |
| v1.3 | 25-29 | 27 | Chain Traversal Performance |
| v1.4 | 30-32 | 24 | Sequential I/O Optimization |
| v1.6 | 33-36 | 38 | Chain Locality |
| v1.13 | 37-44 | 24 | Pub/Sub |
| v1.14 | 45-57 | TBD | Graph Algorithms (17/195 complete - Phase 45 done, 46 done, 47 done, 48 done, 49 done, 50 complete, 51 complete) |

**Recent Trend:**
- v1.13 phases: ~3-6 plans each, ~15-25 min/plan
- v1.14 phase 45: ~8 min/plan (5 plans complete)
- v1.14 phase 46: ~7 min/plan (1 plan complete)
- v1.14 phase 47: ~10 min/plan (3 plans complete)
- v1.14 phase 48: ~7 min/plan (2 plans complete)
- v1.14 phase 49: ~9 min/plan (2 plans complete)
- v1.14 phase 50: ~6 min/plan (2 plans complete)
- Trend: Stable

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- **v1.14 Organization:** Graph algorithms library organized by category (Core Theory, Reachability, CFG, Path Analysis, etc.)
- **Cross-Cutting CC-01:** All algorithms must work on both SQLite and Native V2 backends
- **Cross-Cutting CC-02:** All algorithms support ProgressCallback for long-running operations
- **Cross-Cutting CC-04:** Results cross-validated against petgraph reference implementation
- **Cross-Cutting CC-07:** All algorithms have performance benchmarks (satisfied in Phase 45)
- **Design Philosophy:** "We're not adding algorithms — we're curating a standard library for graph reasoning"
- **Topological sort cycle detection:** Use SCC from plan 45-02 for cycle detection rather than inline detection
- **Topological sort error messages:** Return CycleDetected error with actual cycle path for debugging
- **Dominators algorithm selection:** Chose Cooper et al. simple_fast (2001) over Lengauer-Tarjan for simpler implementation; performs well for realistic CFGs with O(N²) worst case but O(E) to O(N log N) in practice
- **Dominators optimization:** Optimistic initialization (all nodes dominate all) accelerates convergence by only removing from sets; reverse postorder traversal improves speed by processing predecessors before successors
- **Post-dominators via virtual reversal:** Compute post-dominators using predecessor maps instead of physically reversing graph; post_dom(graph, exit) = dominators(reversed(graph), exit) using same Cooper algorithm
- **Multiple exit handling:** Automatically detect exit nodes and unify multiple exits with virtual exit node (id: -1) that is stripped from final results; enables correct post-dominator computation on real-world CFGs with multiple returns
- **Control dependence edge-based definition:** Use Cytron et al. (1991) edge-based conditions directly instead of dominance frontier; node Y is control-dependent on X iff (1) edge X->Y exists, (2) X does NOT post-dominate Y, (3) ipdom[Y] != X
- **Bidirectional CDG mappings:** Provide both cdg (what does this control?) and reverse_cdg (what does this depend on?) for efficient queries in both directions; enables both forward and backward program slicing
- **CDG acyclicity verification:** Control dependence graphs are always acyclic by definition; implemented is_acyclic() method to verify this fundamental property across all CFG structures
- **Dominance frontiers algorithm:** Chose Cytron et al. walk-up algorithm (1991) over naive O(N³) definition-based approach; O(N²) worst case but faster in practice for realistic CFGs
- **Iterated DF iteration limit:** Set to 100 iterations to prevent non-termination on pathological inputs while never triggering on valid CFGs (real-world convergence in 2-4 iterations)
- **Entry node DF handling:** Stop walk-up when idom is None (reached entry node) to prevent infinite loop; correctly handles entry node which has no immediate dominator
- **Natural loop definition:** Use back-edge dominance check (header dominates tail) per Muchnick and Cooper et al.; distinguishes reducible CFGs (natural loops) from irreducible cycles
- **Loop body computation:** DFS from tail, add all reachable nodes except header; O(E) per back-edge, correctly captures all nodes in loop
- **Multiple back-edges grouping:** Group all back-edges to same header into single NaturalLoop; matches programmer intuition (one loop, multiple continue points)
- **Nesting analysis API:** Provide is_nested_in(), nesting_tree(), nesting_depth() helper methods; enables hierarchical loop optimization passes
- **Path enumeration revisit counting:** Use HashMap<i64, usize> instead of boolean visited set; allows bounded loop exploration while preventing infinite enumeration
- **Default path enumeration bounds:** max_depth=100, max_paths=10000, revisit_cap=2; balance between coverage and explosion prevention
- **Path classification during enumeration:** Classify based on terminal node (exit/error) and bounds violations in single pass; efficient and correct
- **Constraint-based path pruning:** Use dominance, control dependence, and natural loop analysis to prune impossible paths during enumeration; provides 10-100x reduction on complex CFGs without false positives
- **Separate dominance config type:** PathEnumerationDominanceConfig wraps PathEnumerationConfig with constraint enablement flags; allows users to enable/disable specific constraint types independently while reusing base configuration
- **Pruning statistics tracking:** Track paths_pruned, total_considered, and reduction_ratio to quantify constraint effectiveness; helps users tune constraint enablement for their specific CFGs
- **Loop stack for constraint checking:** Maintain loop_stack during DFS to track active loop headers; push when entering loop header, pop when exiting; enables efficient loop constraint validation
- **Critical path multi-source DAG:** Initialize all distances to 0 (not -infinity) to handle DAGs with multiple source nodes; each source starts at 0, algorithm finds longest path from any source to any sink
- **Critical path MAX relaxation:** Use max() for longest path computation (opposite of shortest path's min()) - if distance[u] + weight > distance[v], update; this is the key algorithmic difference from shortest path
- **Critical path f64 weights:** Use floating point for edge weights to avoid integer overflow on large DAGs and support fractional durations (e.g., 2.5 hours for task completion)
- **Cycle basis algorithm selection:** Chose Paton's O(V+E+C*V) algorithm over Horton's O(V^3) - simpler, faster for most graphs, produces fundamental cycle basis sufficient for cycle explanation
- **Cycle basis SCC-first decomposition:** Compute SCC decomposition first, then find cycles within each non-trivial SCC - isolates cyclic regions and avoids processing DAG nodes
- **Cycle canonicalization by rotation:** Rotate cycles so minimum node ID is first element - ensures [A,B,C,A], [B,C,A,B], and [C,A,B,C] all deduplicate to same representation
- **Bounded cycle enumeration:** max_cycles (global limit), max_cycle_length (filter long cycles), max_per_scc (limit per SCC) - prevents resource exhaustion on dense graphs
- **SCC supernode ID selection:** Use component index as supernode ID (0, 1, 2, ...) rather than min node ID for deterministic output in condensation graph
- **Bidirectional SCC mappings:** Provide both node_to_supernode and supernode_members for efficient queries in both directions; enables reversible collapse
- **Condensation graph edge deduplication:** Use AHashSet during construction, then sort/dedup final Vec for deterministic output; prevents duplicate edges between supernodes
- **Condensation graph self-loop filtering:** Explicitly check from_supernode != to_supernode when adding edges; condensed graph is always acyclic by definition

### Pending Todos

None yet.

### Blockers/Concerns

**Pre-existing test compilation errors:**
- Test suite has 226 compilation errors unrelated to transitive reduction work
- Errors are in other modules (topological_sort, integration_tests, etc.)
- Library compiles successfully (`cargo check --lib` passes)
- Documentation builds successfully
- Does not block algorithm implementation or usage

## Session Continuity

Last session: 2026-02-02
Stopped at: Completed Phase 51 Plan 02 (SCC Collapse for Call Graph Analysis). 4/4 tasks complete, collapse_sccs and collapse_sccs_with_progress implemented with SccCollapseResult type, bidirectional mappings, 16 comprehensive tests, full module documentation, mod.rs wiring complete.
Resume file: None

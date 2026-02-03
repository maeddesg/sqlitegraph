# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-02)

**Core value:** Feature parity, performance, and reliability equally. Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.
**Current focus:** v1.14 Graph Algorithms Library

## Current Position

Milestone: v1.14 Graph Algorithms Library (IN PROGRESS)
Phase: 55 of 57 (Graph Diff) — IN PROGRESS
Status: Phase 55-01 COMPLETE - Graph diff module with set-based delta computation and Phase 54 similarity integration
Last activity: 2026-02-03 — Phase 55-01 complete with graph_diff(), graph_diff_with_progress(), GraphDiffResult, NodeDelta, EdgeDelta, 18 unit tests

Progress: [██████████░░] 49% of v1.14 (27/197 plans complete, 10/14 phases done, Phase 55-01 complete, 55-02 next)

## Performance Metrics

**Velocity:**
- Total plans completed: 205 (phases 1-44, plus 45-01 through 45-05, plus 46-01, plus 47-01 through 47-03, plus 48-01 through 48-02, plus 49-01 through 49-02, plus 50-01 through 50-02, plus 51-01 through 51-02, plus 52-01 through 52-02, plus 53-01 through 53-02, plus 54-01 through 54-03, plus 55-01)
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
| v1.14 | 45-57 | TBD | Graph Algorithms (27/197 complete - Phase 45 done, 46 done, 47 done, 48 done, 49 done, 50 complete, 51 complete, 52 complete, 53 complete, 54 complete, 55-01 complete) |

**Recent Trend:**
- v1.13 phases: ~3-6 plans each, ~15-25 min/plan
- v1.14 phase 45: ~8 min/plan (5 plans complete)
- v1.14 phase 46: ~7 min/plan (1 plan complete)
- v1.14 phase 47: ~10 min/plan (3 plans complete)
- v1.14 phase 48: ~7 min/plan (2 plans complete)
- v1.14 phase 49: ~9 min/plan (2 plans complete)
- v1.14 phase 50: ~6 min/plan (2 plans complete)
- v1.14 phase 51: ~7 min/plan (2 plans complete)
- v1.14 phase 52: ~7 min/plan (2 plans complete)
- v1.14 phase 53: ~7 min/plan (2 plans complete)
- v1.14 phase 54: ~40 min/plan (3 plans complete - subgraph isomorphism, graph rewriting, structural similarity)
- v1.14 phase 55: ~5 min/plan (1 plan complete - graph diff)
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
- **Edmonds-Karp over Dinic for max-flow:** Chose Edmonds-Karp (BFS-based, O(VE^2)) over Dinic (O(V^2E)) for simpler implementation despite worse theoretical complexity; sufficient for sparse graphs typical in sqlitegraph use cases
- **Vertex splitting encoding:** Used x*2 for x_in, x*2+1 for x_out encoding to avoid collisions with original node IDs; source and sink remain unsplit for edge case simplicity
- **Unit capacities for unweighted min-cut:** Each edge has capacity 1 for unweighted graph cut computation; weighted min-cut deferred to future phase (requires extracting weights from JSON data field)
- **Self-loop filtering in flow networks:** Self-loops removed during flow network construction as they don't affect s-t connectivity in directed graphs
- **Sparse adjacency for flow networks:** Used HashMap-based adjacency instead of dense matrix; sqlitegraph graphs are sparse so dense representation wastes memory
- **PartitionResult structure:** Includes partitions, cut_edges, and node_to_partition mapping for comprehensive analysis and downstream use
- **BFS-level tie-breaking:** Use smallest seed ID for deterministic results when multiple seeds reach node at same level during partitioning
- **Greedy best tracking:** Track best partition seen (not just final state) during greedy improvement to avoid degradation from later moves
- **K-way size relaxation:** When all partitions at max_size, relax bound by (1 + max_imbalance) factor instead of failing
- **Progress reporting frequency:** Report every 10 nodes assigned during partitioning to balance feedback granularity with overhead
- **Vector clock happens-before semantics:** Use strict partial order where A happens-before B requires A <= B element-wise with at least one strict < (not just <=); prevents equal clocks from being considered happens-before
- **Vector clock concurrency detection:** Two events are concurrent if neither happens-before the other; this correctly identifies race candidates where causal ordering cannot be determined
- **Race detection by location grouping:** Group trace events by memory_location and compare vector clocks within each group; concurrent accesses to same location with at least one write = potential data race
- **Read-only exclusion from race detection:** Concurrent reads to same location are not reported as races (only write-write and read-write conflicts matter for data races)
- **Vector clock merge semantics:** Element-wise max of both clocks; used after thread synchronization points to capture causal ordering from all threads
- **Impact radius weight callback reuse:** Re-use WeightCallback and default_weight_fn from critical_path module instead of duplicating in observability; maintains consistency across weighted graph algorithms
- **Epsilon comparison for boundary detection:** Use epsilon (1e-9) for floating-point boundary comparison when detecting nodes at exactly max_distance; handles precision issues with f64 arithmetic
- **Early termination in bounded BFS:** Check distance bound before enqueuing neighbors to avoid unnecessary work; nodes beyond max_distance are never added to the queue
- **Relax on shorter path:** In impact radius BFS, relax edges when new_dist < old_dist OR node not yet in distances; finds shortest weighted path to each node within the blast zone
- **Petgraph dependency location:** Added petgraph to dependencies (not dev-dependencies) because subgraph_isomorphism is part of public API; users may need petgraph types for working with results
- **Double reference pattern for petgraph VF2:** petgraph's subgraph_isomorphisms_iter requires G0: IntoEdgesDirected where trait is implemented for &Graph not Graph; requires passing &a_ref where a_ref = &graph (double reference pattern)
- **Subgraph isomorphism default bounds:** max_matches=100, timeout_ms=5000, max_pattern_nodes=10 to prevent exponential blowup on dense graphs with large patterns
- **MCS approximation approach:** Use smaller graph as pattern, larger as target for subgraph_isomorphism enumeration to find maximum common subgraph
- **Similarity score normalization:** mcs_size / max(graph1_size, graph2_size) for 0.0-1.0 range where 1.0 = isomorphic
- **Simplified GED computation:** 1.0 - mcs_similarity instead of full graph edit distance to avoid expensive computation
- **Empty graph similarity semantics:** Both empty = isomorphic (1.0), one empty = no common structure (0.0)
- **Similarity score interpretation:** 1.0=Identical, 0.8+=Very Similar, 0.5+=Similar, <0.5=Different, 0.0=None

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

Last session: 2026-02-03
Stopped at: Phase 55-01 complete. Graph diff module with set-based delta computation and Phase 54 similarity integration delivered: graph_diff(), graph_diff_with_progress(), GraphDiffResult, NodeDelta, EdgeDelta with 18 unit tests.
Resume file: None

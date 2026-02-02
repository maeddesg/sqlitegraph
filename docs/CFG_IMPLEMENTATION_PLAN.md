# Graph Algorithms Implementation Plan

**Status:** Ready to implement
**Last Updated:** 2026-02-02
**Priority:** High

---

## Task

Implement graph algorithms in SQLiteGraph as proposed in `CFG_ALGORITHMS.md`.

**Philosophy:** We're not "adding algorithms" — we're curating a **standard library for graph reasoning**.

---

## Design Principle: The "sqlitegraph Sweet Spot"

```
Can use for CFGs, call graphs, AND inference graphs?
    → YES: Belongs in sqlitegraph
    → NO:  Belongs in Mirage (if CFG-specific) or downstream tool
```

---

## Order of Implementation

### Phase 0: Core Graph Theory (Universally Useful, Low Regret)

**These are "if you have a graph engine, people expect these eventually".**

- [ ] **`src/algo/scc.rs`**
  - `strongly_connected_components()` - Tarjan's algorithm
  - `condense_dag()` - build condensed DAG from SCCs
  - Tests: linear chain, single loop, mutual recursion, complex SCCs
  - **Why:** Loops in CFGs, recursion in call graphs, feedback in inference graphs

- [ ] **`src/algo/topological_sort.rs`**
  - `topological_sort()` - Kahn's algorithm with cycle detection
  - `explain_cycle()` - human-readable cycle explanation
  - Tests: valid DAG, single cycle, multiple cycles
  - **Why:** Kernel verification, build ordering, partial order validation

- [ ] **`src/algo/wcc.rs`**
  - `weakly_connected_components()` - undirected connectivity
  - Tests: disconnected graph, bridged graph
  - **Why:** Dependency clustering, partitioning, infra sanity checks

- [ ] **`src/algo/transitive_reduction.rs`**
  - `transitive_reduction()` - remove redundant edges (DAG only)
  - Tests: linear chain, diamond, fully connected
  - **Why:** Makes graphs readable, explanations clearer, huge UX win

- [ ] **`src/algo/transitive_closure.rs`**
  - `transitive_closure()` - compute all reachability (bounded / cached)
  - Tests: simple paths, cycles, complex graphs
  - **Why:** Fast "does X ever influence Y?" queries, reachability caches

---

### Phase 1: Reachability & Slicing (Tier 1)

**Program slicing in its simplest form.**

- [ ] **`src/algo/reachability.rs`**
  - `reachable_from(start)` - forward slice
  - `reverse_reachable_from(target)` - backward slice
  - `can_reach(from, to)` - point-to-point check
  - `find_unreachable(entry)` - complement (dead code detection)
  - Tests: linear, diamond, complex CFGs, isolated nodes
  - **Why:** "What affects this?" "What does this affect?" "What's dead?"

---

### Phase 2: Core CFG Algorithms (Tier 2)

**The dominance family — structural must-execute analysis.**

- [ ] **`src/algo/dominators.rs`**
  - `dominators(graph, entry)` - compute all dominators
  - `immediate_dominators(graph, entry)` - IDom tree
  - `dominator_tree()` - tree representation
  - Tests: simple chain, diamond, complex function, irreducible
  - **Why:** SSA construction, must-execute analysis

- [ ] **`src/algo/post_dominators.rs`**
  - `post_dominators(graph, exit)` - compute post-dominators
  - `immediate_post_dominators(graph, exit)` - IPostDom tree
  - Tests: matching Phase 2 cases
  - **Why:** Control dependence, impact analysis from exit

- [ ] **`src/algo/control_dependence.rs`**
  - `control_dependence_graph()` - CDG from post-dominators
  - `explain_control_dependence()` - human-readable explanations
  - Tests: diamond, nested branches, complex control flow
  - **Why:** "This block executes because of that condition" — semantic twin of dominators

---

### Phase 3: Derived CFG Algorithms (Tier 3)

**Built on top of dominance.**

- [ ] **`src/algo/dominance_frontiers.rs`**
  - `dominance_frontiers()` - Cytron et al. algorithm
  - `iterated_dominance_frontiers()` - for SSA φ-placement
  - Tests with expected DF results
  - **Why:** SSA construction, variable liveness

- [ ] **`src/algo/loops.rs`**
  - `detect_natural_loops()` - find back-edges where head dominates tail
  - `find_loop_headers()` - identify all loop headers
  - `loop_nesting_tree()` - hierarchical loop structure
  - Tests: single loop, nested loops, irreducible loops
  - **Why:** Loop optimization, unrolling decisions, invariant code motion

---

### Phase 4: Path Analysis

**Enumerate and classify execution paths.**

- [ ] **`src/algo/paths.rs`**
  - `enumerate_paths()` - DFS with cycle detection
  - `enumerate_paths_with_feasibility()` - with dominance pruning
  - `classify_path()` - Normal / Error / Degenerate / Infinite
  - Tests: bounding, cycle handling, dominance constraints
  - **Why:** Test generation, coverage analysis, explanation

---

### Phase 5: Dependency & Build Systems

**Critical path and bottleneck analysis.**

- [ ] **`src/algo/critical_path.rs`**
  - `critical_path()` - longest path in DAG (not shortest!)
  - `find_bottlenecks()` - identify slowest chains
  - Tests: weighted DAGs, multiple paths, complex dependencies
  - **Why:** Build optimization, CI scheduling, kernel execution ordering

- [ ] **`src/algo/cycle_basis.rs`**
  - `minimal_cycle_basis()` - explain "why" not just "that"
  - `find_all_cycles()` - enumerate cycles (bounded)
  - Tests: simple cycle, overlapping cycles, complex feedback
  - **Why:** Debug recursive dependencies, explain deadlocks

---

### Phase 6: Program Analysis & Tooling

**Mirage/Magellan adjacent algorithms.**

- [ ] **`src/algo/slicing.rs`**
  - `backward_slice(target)` - "what can affect this node?"
  - `forward_slice(source)` - "what does this node affect?"
  - `program_slice()` - combined + slicing criteria
  - Tests: bug isolation, refactor safety
  - **Why:** Bug isolation, refactoring safety, explanation systems

- [ ] **`src/algo/call_graph_collapse.rs`**
  - `collapse_recursion()` - merge SCCs in call graphs
  - `collapse_libraries()` - merge external dependencies
  - `collapse_sccs_to_supernodes()` - general SCC collapse
  - Tests: mutual recursion, deep call chains
  - **Why:** Makes call graphs readable, analyses tractable

---

### Phase 7: Databases & Distributed Systems

**sqlitegraph's natural home.**

- [ ] **`src/algo/min_cut.rs`**
  - `min_cut(source, target)` - smallest edge cut
  - `min_vertex_cut(source, target)` - smallest node cut
  - `blast_radius(node)` - "how far does damage propagate?"
  - Tests: fault tolerance, security boundaries
  - **Why:** Fault tolerance, security boundaries, blast radius estimation

- [ ] **`src/algo/partitioning.rs`**
  - `greedy_partition(num_parts, max_size)` - simple heuristic
  - `bfs_partition(seeds)` - seed-based partitioning
  - `size_bounded_partition(max_part_size)` - balanced partitions
  - Tests: sharding, locality optimization
  - **Why:** Sharding, caching, locality optimization

---

### Phase 8: Observability & Runtime

**Tracing and causal analysis.**

- [ ] **`src/algo/causal_graph.rs`**
  - `happens_before_analysis()` - event ordering
  - `detect_potential_races()` - lightweight race detection
  - Tests: concurrent traces, distributed logs
  - **Why:** Trace analysis, concurrency reasoning, distributed logs

- [ ] **`src/algo/impact_radius.rs`**
  - `impact_radius(node, max_depth)` - bounded reachability with weights
  - `blast_zone(source)` - what breaks if this fails
  - Tests: ops scenarios, infra debugging
  - **Why:** Ops, infra, refactors, inference debugging

---

### Phase 9: ML / Inference / Compute Graphs

**SimdFlow relevance.**

- [ ] **`src/algo/pattern_matching.rs`**
  - `find_pattern(graph, pattern)` - subgraph isomorphism (bounded)
  - `rewrite_pattern(graph, pattern, replacement)` - graph rewriting
  - `find_repeated_subgraphs()` - common subexpression detection
  - Tests: compiler patterns, ML graph fusion
  - **Why:** Compilers, ML frameworks, optimizers, inference engines

- [ ] **`src/algo/graph_isomorphism.rs`**
  - `is_isomorphic(g1, g2)` - practical check (not NP-perfect)
  - `structural_similarity(g1, g2)` - similarity score
  - Tests: regression detection, refactor verification
  - **Why:** Regression detection, verifying refactors, optimizer equivalence

---

### Phase 10: Graph Diff (Tier 4)

**Compare two snapshots.**

- [ ] **`src/algo/graph_diff.rs`**
  - `graph_diff(old, new)` - structural delta
  - `semantic_drift(old, new)` - did meaning change?
  - `validate_refactor(before, after)` - did I break anything?
  - Tests: refactors, optimizer verification
  - **Why:** Refactor validation, optimizer verification, regression analysis

---

### Phase 11: Security & Compliance

**Unexpected but useful.**

- [ ] **`src/algo/taint_propagation.rs`**
  - `propagate_taint(graph, sources)` - taint tracking on graph
  - `find_taint_sinks(source, sinks)` - does source reach sink?
  - Tests: security analysis, data flow auditing
  - **Why:** Security analysis, data flow auditing, compliance tooling

---

### Phase 12: CLI Commands

**User-facing algorithms.**

```bash
# Core graph theory
sqlitegraph --backend sqlite --db codegraph.db scc
sqlitegraph --backend sqlite --db codegraph.db topo-sort
sqlitegraph --backend sqlite --db codegraph.db wcc
sqlitegraph --backend sqlite --db codegraph.db transitive-reduction
sqlitegraph --backend sqlite --db codegraph.db transitive-closure

# Reachability
sqlitegraph --backend sqlite --db codegraph.db reachable --from <id>
sqlitegraph --backend sqlite --db codegraph.db reverse-reachable --to <id>
sqlitegraph --backend sqlite --db codegraph.db can-reach --from <id> --to <id>

# Dominance
sqlitegraph --backend sqlite --db codegraph.db cfg-dominators --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-post-dominators --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-cdg --function <id>

# Derived
sqlitegraph --backend sqlite --db codegraph.db cfg-frontiers --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-loops --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-paths --function <id> --max-length 100

# Dependency analysis
sqlitegraph --backend sqlite --db codegraph.db critical-path
sqlitegraph --backend sqlite --db codegraph.db explain-cycle

# Slicing
sqlitegraph --backend sqlite --db codegraph.db backward-slice --to <id>
sqlitegraph --backend sqlite --db codegraph.db forward-slice --from <id>

# Structural
sqlitegraph --backend sqlite --db codegraph.db min-cut --source <id> --target <id>
sqlitegraph --backend sqlite --db codegraph.db partition --num-parts 4

# Diff
sqlitegraph --backend sqlite --db codegraph.db graph-diff --old <db> --new <db>

# Pattern matching
sqlitegraph --backend sqlite --db codegraph.db match-pattern --pattern <file>
sqlitegraph --backend sqlite --db codegraph.db find-repeated --min-size 3

# Security
sqlitegraph --backend sqlite --db codegraph.db taint-propagate --sources <ids>
```

---

## Files to Create

1. `src/algo/scc.rs` - Strongly Connected Components
2. `src/algo/topological_sort.rs` - Topological sort with cycle explanation
3. `src/algo/wcc.rs` - Weakly Connected Components
4. `src/algo/transitive_reduction.rs` - Edge reduction for DAGs
5. `src/algo/transitive_closure.rs` - Reachability cache
6. `src/algo/reachability.rs` - Forward/backward reachability
7. `src/algo/dominators.rs` - Dominator tree computation
8. `src/algo/post_dominators.rs` - Post-dominator computation
9. `src/algo/control_dependence.rs` - Control Dependence Graph
10. `src/algo/dominance_frontiers.rs` - Dominance frontier computation
11. `src/algo/loops.rs` - Natural loop detection
12. `src/algo/paths.rs` - Path enumeration with feasibility
13. `src/algo/critical_path.rs` - Longest path / bottleneck analysis
14. `src/algo/cycle_basis.rs` - Minimal cycle basis / explanation
15. `src/algo/slicing.rs` - Program slicing (backward/forward)
16. `src/algo/call_graph_collapse.rs` - Call graph pruning
17. `src/algo/min_cut.rs` - Minimum cut / vertex cut
18. `src/algo/partitioning.rs` - Graph partitioning heuristics
19. `src/algo/causal_graph.rs` - Happens-before analysis
20. `src/algo/impact_radius.rs` - Blast radius estimation
21. `src/algo/pattern_matching.rs` - Subgraph matching / rewriting
22. `src/algo/graph_isomorphism.rs` - Structural similarity
23. `src/algo/graph_diff.rs` - Graph diff / semantic drift
24. `src/algo/taint_propagation.rs` - Taint tracking
25. `tests/algo/graph_tests.rs` - Integration tests
26. Update `src/algo/mod.rs` - Re-exports

---

## Acceptance Criteria

- All algorithms pass unit tests
- Results match petgraph reference implementation (cross-validation)
- Progress tracking works for all algorithms
- CLI commands functional
- Documentation updated
- Performance benchmarks pass
- Each algorithm has at least 3 test cases (simple, medium, complex)

---

## Next Steps

When ready to begin:

1. Read `CFG_ALGORITHMS.md` for algorithm specifications
2. Start with **Phase 0** (Core Graph Theory) - foundational, low regret
3. Follow the existing pattern in `src/algo/pagerank.rs`
4. Run `cargo test --workspace` after each phase
5. Update this file as implementation progresses
6. Cross-validate against petgraph where applicable

---

## Notes

- **Algorithms are well-studied** — reference papers in `CFG_ALGORITHMS.md`
- **petgraph has reference implementations** we can adapt
- **Test heavily** with real-world graphs from indexed Rust code
- **Consider edge cases:** empty graphs, single node, disconnected components, irreducible graphs
- **Think in decades** — this is a standard library for graph reasoning
- **Generic over graph type** — these work on CFGs, call graphs, inference graphs, etc.

---

## "If sqlitegraph shipped just these, it would already be exceptional:"

1. SCC
2. Topological sort + cycle explanation
3. Dominators / post-dominators
4. Control dependence graph
5. Reachability / slicing
6. Transitive reduction
7. Critical path
8. Graph diff
9. Minimum cut
10. Pattern matching (basic)

**That's a decade-proof core.**

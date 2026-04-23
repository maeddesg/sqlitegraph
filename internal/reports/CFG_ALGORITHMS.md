# Graph Algorithms for SQLiteGraph

**Status:** Proposal
**Last Updated:** 2026-02-02
**Priority:** High (enables Mirage integration)

---

## Overview

This document proposes adding graph algorithms to SQLiteGraph. These algorithms are currently provided by `petgraph` but used by Mirage for path-aware code intelligence.

By integrating these algorithms into SQLiteGraph, we:
1. Keep the entire analysis stack in one ecosystem
2. Enable Magellan to store graphs during indexing
3. Allow Mirage to query algorithms without additional dependencies
4. Provide consistent progress tracking and performance monitoring

---

## Design Principle: The "sqlitegraph Sweet Spot"

**If an algorithm satisfies:**
> "I can use this for CFGs, call graphs, and inference graphs"

**Then it belongs in sqlitegraph.**

**If it's:**
- CFG-only
- Language-specific
- Semantics-heavy

**It belongs in Mirage.**

All algorithms below are generic graph algorithms applicable to:
- Control Flow Graphs (CFGs)
- Call graphs
- Dataflow graphs
- Inference graphs

---

## Background: What is CFG Analysis?

Control Flow Graph (CFG) analysis operates on **function-level** graphs:

- **Nodes**: Basic blocks (maximal straight-line code sequences)
- **Edges**: Control flow (fallthrough, branches, calls, returns)
- **Entry**: Function entry point
- **Exits**: Return, panic, abort, infinite loop

**Key Applications:**
- Path enumeration (all execution paths through a function)
- Dominance analysis (must-execute code for SSA construction)
- Loop detection (natural loops via back-edges)
- Reachability (can block A reach block B?)
- Impact analysis (blast zone from code changes)

---

## Required Algorithms

### Tier 1: Graph Primitives (Almost Certainly Want These)

#### 0. Strongly Connected Components (Tarjan)

**Purpose:** Find maximal subgraphs where every node can reach every other node.

**Why This Is Foundational:**
- Detect loops in CFGs (loops are SCCs)
- Collapse SCCs → DAG for dominance analysis
- Identify recursion cycles in call graphs
- Detect feedback loops in inference graphs
- Prerequisite for topological sort

**Applies To:**
- Mirage (loop regions)
- Magellan (recursive symbols)
- SimdFlow (feedback / residual structures)

**Algorithm:** Tarjan's SCC (single-pass, O(|V| + |E|))

**Input:**
```rust
graph: &dyn GraphBackend
```

**Output:**
```rust
pub struct SccResult {
    /// Each component is a set of nodes that are mutually reachable
    pub components: Vec<HashSet<NodeId>>,
    /// Maps each node to its component index
    pub node_to_component: HashMap<NodeId, usize>,
    /// The condensed DAG (each SCC becomes a supernode)
    pub condensed_dag: GraphSnapshot,
}
```

**Complexity:** O(|V| + |E|)

**SQLiteGraph Integration:**
```rust
pub fn strongly_connected_components(
    graph: &dyn GraphBackend,
) -> Result<SccResult, SqliteGraphError> {
    let snapshot = graph.snapshot()?;
    let mut index_counter = 0;
    let mut stack = Vec::new();
    let mut on_stack = HashSet::new();
    let mut indices = HashMap::new();
    let mut lowlink = HashMap::new();
    let mut components = Vec::new();

    // Tarjan's algorithm: single DFS pass
    for node in snapshot.all_node_ids()? {
        if !indices.contains_key(&node) {
            strongconnect(&snapshot, node, &mut index_counter, &mut stack,
                         &mut on_stack, &mut indices, &mut lowlink,
                         &mut components)?;
        }
    }

    Ok(SccResult { /* ... */ })
}
```

**References:**
- R. E. Tarjan, "Depth-First Search and Linear Graph Algorithms"

---

#### 1. Topological Sort (with Cycle Reporting)

**Purpose:** Linear ordering of nodes such that every edge points from earlier to later.

**Why This Matters:**
- Kernel verification order (what must execute first)
- Build / execution ordering
- Partial order validation
- **Cycle explanation** — when topo sort fails, tell user WHY

**You already wrote scripts doing this externally — this belongs in sqlitegraph.**

**Algorithm:** Kahn's algorithm (with cycle detection)

**Input:**
```rust
graph: &dyn GraphBackend
```

**Output:**
```rust
pub enum TopoError {
    CycleDetected {
        cycle: Vec<NodeId>,
        explanation: String,
    },
}

pub fn topological_sort(
    graph: &dyn GraphBackend,
) -> Result<Vec<NodeId>, TopoError> {
    // If graph has cycles, return the cycle with explanation
    // Otherwise, return valid linear ordering
}
```

**Complexity:** O(|V| + |E|)

**SQLiteGraph Integration:**
```rust
pub fn topological_sort(
    graph: &dyn GraphBackend,
) -> Result<Vec<NodeId>, TopoError> {
    let snapshot = graph.snapshot()?;

    // First, check if graph is a DAG using SCC
    let scc = strongly_connected_components(graph)?;
    let non_trivial_sccs: Vec<_> = scc.components
        .into_iter()
        .filter(|c| c.len() > 1)
        .collect();

    if !non_trivial_sccs.is_empty() {
        // Graph has cycles — identify and explain
        return Err(TopoError::CycleDetected {
            cycle: extract_cycle_path(&snapshot, &non_trivial_sccs[0])?,
            explanation: format!("Found {} cycle(s)", non_trivial_sccs.len()),
        });
    }

    // Run Kahn's algorithm on the DAG
    let mut in_degree = HashMap::new();
    // ... compute in-degrees and process nodes

    Ok(sorted)
}
```

**Key Insight:** Topological sort is only valid on DAGs. SCC decomposition is the prerequisite.

---

#### 2. Reachability / Slice Analysis

**Purpose:** Find all nodes reachable from a start (forward) or all nodes that can reach a target (backward).

**Why This Is Huge:**
- "Which blocks affect this output?" (backward slice)
- "What paths can reach this exit?" (forward slice)
- "What code is dead relative to entry?" (complement of forward slice)
- **This is program slicing in its simplest form**

**Composes with dominators beautifully:**
- Forward slice + dominators = precise impact analysis
- Backward slice + post-dominators = "why does this execute?"

**Algorithm:** BFS/DFS traversal

**Input:**
```rust
graph: &dyn GraphBackend,
start: NodeId  // for forward
// or
target: NodeId  // for backward
```

**Output:**
```rust
pub fn reachable_from(
    graph: &dyn GraphBackend,
    start: NodeId,
) -> Result<HashSet<NodeId>, SqliteGraphError> {
    // All nodes reachable from start
}

pub fn reverse_reachable_from(
    graph: &dyn GraphBackend,
    target: NodeId,
) -> Result<HashSet<NodeId>, SqliteGraphError> {
    // All nodes that can reach target (backward traversal)
}
```

**Complexity:** O(|V| + |E|)

**SQLiteGraph Integration:**
```rust
pub fn reachable_from(
    graph: &dyn GraphBackend,
    start: NodeId,
) -> Result<HashSet<NodeId>, SqliteGraphError> {
    let snapshot = graph.snapshot()?;
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(start);
    visited.insert(start);

    while let Some(current) = queue.pop_front() {
        let query = NeighborQuery::outgoing(current);
        for neighbor in snapshot.neighbors(query)? {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    Ok(visited)
}

pub fn reverse_reachable_from(
    graph: &dyn GraphBackend,
    target: NodeId,
) -> Result<HashSet<NodeId>, SqliteGraphError> {
    // Same as above, but use NeighborQuery::incoming()
}
```

---

### Tier 2: Core CFG Algorithms

#### 3. Dominators (Lengauer-Tarjan / Cooper et al.)

**Purpose:** Find which nodes MUST execute on any path from entry to exit.

**Algorithm:** `simple_fast` from petgraph (Cooper et al.)

**Input:**
```rust
graph: &dyn GraphBackend,
entry: NodeId,
```

**Output:**
```rust
HashMap<NodeId, HashSet<NodeId>>  // node -> set of dominators
HashMap<NodeId, Option<NodeId>>   // node -> immediate dominator
```

**Complexity:** O(|V|²) worst case, O(|E| α(|V|)) in practice

**References:**
- T. Lengauer and R. Tarjan, "A Fast Algorithm for Finding Dominators in a Flow Graph"
- Keith D. Cooper, Timothy J. Harvey, and Ken Kennedy, "A Simple, Fast Dominance Algorithm"

---

#### 4. Post-Dominators

**Purpose:** Find which nodes MUST execute on any path from node to exit.

**Algorithm:** Dominators on reversed graph

**Input:**
```rust
graph: &dyn GraphBackend,
exit: NodeId,
```

**Output:**
```rust
HashMap<NodeId, HashSet<NodeId>>  // node -> post-dominators
HashMap<NodeId, Option<NodeId>>   // node -> immediate post-dominator
```

**Complexity:** O(|V|²) worst case

---

#### 5. Control Dependence Graph (CDG)

**Purpose:** "This block executes because of that condition"

**Why This Is The Semantic Twin of Dominators:**
- Dominators: "must execute" (structural)
- CDG: "executes because of" (semantic, control-flow-sensitive)

**Required For:**
- Precise explanations ("this block only runs if X is true")
- Safe refactors ("what breaks if I change this condition?")
- Impact analysis ("what code does this branch affect?")

**Derived From:**
- CFG
- Post-dominators (which you already have)

**Definition:** Node Y is control-dependent on node X iff:
1. There exists a path from X to Y where X does not post-dominate Y
2. Y's immediate post-dominator is not reachable from X without going through Y

**Input:**
```rust
graph: &dyn GraphBackend,
post_dominators: &HashMap<NodeId, HashSet<NodeId>>,
```

**Output:**
```rust
HashMap<NodeId, HashSet<NodeId>>  // node -> nodes control-dependent on it
```

**Complexity:** O(|E|) after post-dominators computed

**SQLiteGraph Integration:**
```rust
pub fn control_dependence_graph(
    graph: &dyn GraphBackend,
) -> Result<HashMap<NodeId, HashSet<NodeId>>, SqliteGraphError> {
    // First compute post-dominators
    let exit = find_exit_node(graph)?;
    let post_doms = post_dominators(graph, exit)?;

    let snapshot = graph.snapshot()?;
    let mut cdg = HashMap::new();

    // For each edge (X -> Y) in CFG:
    for edge in snapshot.all_edges()? {
        let (from, to) = (edge.from, edge.to);

        // Y is control-dependent on X iff:
        // 1. from does NOT post-dominate to
        // 2. to's immediate post-dominator is not on a path from 'from' without going through 'to'
        if !post_dominates(&post_doms, from, to) {
            let ipdom = immediate_post_dominator(&post_doms, to);
            if ipdom != Some(from) {
                cdg.entry(from).or_default().insert(to);
            }
        }
    }

    Ok(cdg)
}
```

**This is a killer Mirage feature** — enables "explain why this code executes" queries.

---

### Tier 3: Derived CFG Algorithms

#### 6. Dominance Frontiers (Cytron et al.)

**Purpose:** Compute iterated dominance frontier for SSA φ-placement.

**Algorithm:** Cytron et al. efficient DF computation

**Input:**
```rust
graph: &dyn GraphBackend,
dominators: &HashMap<NodeId, HashSet<NodeId>>,
```

**Output:**
```rust
HashMap<NodeId, HashSet<NodeId>>  // node -> dominance frontier
```

**Complexity:** O(N²) for N nodes

**References:**
- Ron Cytron, Jeanne Ferrante, Ken Zadeck, "Efficiently Computing Static Single Assignment Form"

---

#### 7. Natural Loop Detection

**Purpose:** Identify natural loops (back-edges where head dominates tail).

**Algorithm:** Find back-edges, check if head dominates tail

**Input:**
```rust
graph: &dyn GraphBackend,
dominators: &HashMap<NodeId, HashSet<NodeId>>,
```

**Output:**
```rust
pub struct NaturalLoop {
    pub header: NodeId,           // Loop header (dominates back-edge target)
    pub back_edges: Vec<(NodeId, NodeId)>,  // (tail -> header)
    pub body: HashSet<NodeId>,    // All nodes in loop (excluding header)
}
```

**Complexity:** O(|E|) to find all back-edges

---

#### 8. Path Enumeration with Feasibility Pruning

**Purpose:** Enumerate all execution paths from entry to exit, with pruning.

**Why Pruning Matters:**
- Avoid exploding path enumeration (exponential in loops)
- Skip impossible branch combinations (dominance constraints)
- Bound analysis cost (user limits)

**Lightweight Feasibility Rules:**
```rust
pub struct PathLimits {
    pub max_depth: usize,
    pub max_paths: usize,
    pub max_visits_per_node: usize,  // Revisit cap (prevents infinite loops)
    pub respect_dominance: bool,     // Prune impossible paths
}

pub enum PathKind {
    Normal,
    Error,       // Ends in panic/abort
    Degenerate,  // Ends in unreachable
    Infinite,    // Exceeded revisit cap (likely infinite loop)
}
```

**Algorithm:** DFS with cycle detection + pruning

**Input:**
```rust
graph: &dyn GraphBackend,
entry: NodeId,
limits: PathLimits,
```

**Output:**
```rust
pub struct Path {
    pub blocks: Vec<NodeId>,
    pub kind: PathKind,
    pub feasibility_score: f32,  // 0.0 = impossible, 1.0 = certain
}
```

**Complexity:** Exponential worst case, bounded by limits

---

### Tier 4: Strategic (Optional)

#### 9. Graph Diff / Structural Delta

**Purpose:** Compare two graphs and detect structural differences.

**Why This Aligns With Your Philosophy:**
- Compare two CFGs (before/after refactor)
- Compare two call graphs (dependency drift)
- Detect semantic changes (regression analysis)

**Applies To:**
- Refactor validation (did I break anything?)
- Optimizer verification (graph preserved?)
- Inference engine changes (structure intact?)

**Input:**
```rust
old: &dyn GraphBackend,
new: &dyn GraphBackend,
```

**Output:**
```rust
pub struct GraphDelta {
    pub nodes_added: HashSet<NodeId>,
    pub nodes_removed: HashSet<NodeId>,
    pub edges_added: HashSet<(NodeId, NodeId)>,
    pub edges_removed: HashSet<(NodeId, NodeId)>,
    pub structural_similarity: f32,  // 0.0 = completely different
}
```

**Very few tools do this well.**

---

#### 10. Minimum Cut / Cut Sets

**Purpose:** Find smallest set of nodes/edges that separate source from target.

**Why This Is Surprisingly Useful:**
- "What is the minimal set of nodes that breaks this path?"
- Fault isolation (where to insert logging?)
- Security boundaries (what protects this asset?)
- Failure containment (what prevents propagation?)

**Applies Beyond CFGs:**
- Dependency graphs (circular dependency breaking)
- Execution graphs (bottleneck identification)

**Algorithm:** Stoer-Wagner min-cut (global) or max-flow min-cut (s-t)

**Input:**
```rust
graph: &dyn GraphBackend,
source: NodeId,
target: NodeId,
```

**Output:**
```rust
pub struct MinCut {
    pub cut_nodes: Vec<NodeId>,
    pub cut_edges: Vec<(NodeId, NodeId)>,
    pub cut_size: usize,  // Number of elements to remove
}
```

**Advanced but very powerful.**

---

## Data Structures

### CFG Node

```rust
pub struct CfgNode {
    pub id: NodeId,
    pub kind: CfgNodeKind,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub terminator: Terminator,
}

pub enum CfgNodeKind {
    Entry,
    Exit,
    Normal,
}

pub enum Terminator {
    Return,
    Goto { target: NodeId },
    SwitchInt { targets: Vec<NodeId>, otherwise: NodeId },
    Call { target: Option<NodeId>, unwind: Option<NodeId> },
    Unreachable,
}
```

### CFG Edge

```rust
pub enum CfgEdgeType {
    Fallthrough,
    TrueBranch,
    FalseBranch,
    Switch,
    SwitchDefault,
    Call,
    CallReturn,
    Unwind,
}
```

---

## Implementation Plan

### Phase 0: Graph Primitives (Tier 1)

1. **`src/algo/scc.rs`**
   - `strongly_connected_components()` - Tarjan's algorithm
   - `condense_dag()` - build condensed DAG from SCCs

2. **`src/algo/topological_sort.rs`**
   - `topological_sort()` - Kahn's algorithm with cycle detection
   - `explain_cycle()` - human-readable cycle explanation

3. **`src/algo/reachability.rs`**
   - `reachable_from()` - forward slice
   - `reverse_reachable_from()` - backward slice
   - `find_unreachable()` - complement of reachable

### Phase 1: Core CFG Algorithms (Tier 2)

4. **`src/algo/dominators.rs`**
   - `dominators()` - compute all dominators
   - `immediate_dominators()` - compute immediate dominators only

5. **`src/algo/post_dominators.rs`**
   - `post_dominators()` - compute post-dominators
   - `immediate_post_dominators()` - compute immediate post-dominators

6. **`src/algo/control_dependence.rs`**
   - `control_dependence_graph()` - CDG from post-dominators
   - `explain_control_dependence()` - human-readable explanations

### Phase 2: Derived Algorithms (Tier 3)

7. **`src/algo/dominance_frontiers.rs`**
   - `dominance_frontiers()` - compute DF for all nodes
   - `iterated_dominance_frontiers()` - iterated DF for SSA

8. **`src/algo/loops.rs`**
   - `detect_natural_loops()` - find all natural loops
   - `find_loop_headers()` - identify all loop headers

9. **`src/algo/paths.rs`**
   - `enumerate_paths()` - DFS with feasibility pruning
   - `enumerate_paths_with_progress()` - with progress tracking

### Phase 3: CLI Commands

```bash
sqlitegraph --backend sqlite --db codegraph.db scc
sqlitegraph --backend sqlite --db codegraph.db topo-sort
sqlitegraph --backend sqlite --db codegraph.db reachable --from <id>
sqlitegraph --backend sqlite --db codegraph.db reverse-reachable --to <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-dominators --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-post-dominators --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-cdg --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-frontiers --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-loops --function <id>
sqlitegraph --backend sqlite --db codegraph.db cfg-paths --function <id> --max-length 100
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_scc_linear_chain() {
        // Linear chain: 0 -> 1 -> 2 -> 3
        // Each node is its own SCC
    }

    #[test]
    fn test_scc_with_loop() {
        // 0 -> 1 -> 2 -> 1 (cycle)
        // SCCs: {0}, {1, 2}
    }

    #[test]
    fn test_topological_sort_dag() {
        // Diamond DAG
        // Valid ordering: [0, 1, 2, 3] or [0, 2, 1, 3]
    }

    #[test]
    fn test_topological_sort_cycle() {
        // Graph with cycle should return CycleDetected
    }

    #[test]
    fn test_reachability_forward() {
        // Forward reachable from entry
    }

    #[test]
    fn test_reachability_backward() {
        // Backward reachable to exit (slice)
    }

    #[test]
    fn test_dominators_linear_chain() {
        // Linear CFG: 0 dominates 1,2,3; 1 dominates 2,3; etc.
    }

    #[test]
    fn test_dominators_diamond() {
        // Diamond CFG: 0 dominates all; 1,2 dominate 3
    }

    #[test]
    fn test_cdg_diamond() {
        // Block 3 is control-dependent on block 0 (branch)
    }
}
```

### Integration Tests

- Test with real CFGs from indexed code
- Compare results against petgraph (cross-validation)
- Performance benchmarks on large functions

---

## Complexity Summary

| Algorithm | Time Complexity | Space Complexity |
|-----------|-----------------|------------------|
| SCC (Tarjan) | O(\|V\| + \|E\|) | O(\|V\|) |
| Topological Sort | O(\|V\| + \|E\|) | O(\|V\|) |
| Reachability | O(\|V\| + \|E\|) | O(\|V\|) |
| Dominators | O(\|V\|²) worst, faster in practice | O(\|V\|) |
| Post-dominators | O(\|V\|²) worst | O(\|V\|) |
| Control Dependence | O(\|E\|) after post-doms | O(\|E\|) |
| Dominance Frontiers | O(N²) | O(N²) |
| Natural Loops | O(\|E\|) | O(\|E\|) |
| Path Enumeration | O(2^n) bounded by limits | O(depth × paths) |
| Graph Diff | O(\|V\| + \|E\|) | O(\|V\|) |
| Min Cut | O(\|V\|³) or O(\|E\| √\|V\|) | O(\|V\|²) |

---

## Dependencies

No additional dependencies required! These algorithms use:
- Existing `GraphBackend` trait
- Existing `NeighborQuery` API
- Existing `ProgressCallback` infrastructure

---

## Comparison: petgraph vs. sqlitegraph

| Aspect | petgraph (current) | sqlitegraph (proposed) |
|--------|-------------------|----------------------|
| Storage | In-memory `DiGraph` | Database-backed |
| Algorithms | `simple_fast`, `has_path_connecting` | Same algorithms, different API |
| Progress Tracking | None | Built-in support |
| Persistence | Manual serialization | Already persisted |
| Multi-user | Single-user | Concurrent via MVCC |
| SCC | `tarjan_scc` | Integrated with progress |
| Topo Sort | `toposort` (panics on cycle) | Returns `CycleError` with explanation |

---

## Migration Strategy for Mirage

### Current Mirage (petgraph-based):

```rust
use petgraph::algo::dominators::simple_fast;

let dominators = simple_fast(&cfg, entry);
```

### Proposed Mirage (sqlitegraph-based):

```rust
use sqlitegraph::algo;

// CFG stored in sqlitegraph during indexing
let scc = algo::strongly_connected_components(&cfg)?;
let dominators = algo::dominators(&cfg, entry)?;
let cdg = algo::control_dependence_graph(&cfg)?;
```

**Benefits:**
- No petgraph dependency in Mirage
- CFGs persisted during indexing
- Multiple Mirage instances can share results
- Consistent with Magellan architecture
- Better error messages (cycle explanation)

---

## Open Questions

1. **Graph Representation**: Should CFG be stored as:
   - Native sqlitegraph nodes/edges?
   - Separate tables (`cfg_blocks`, `cfg_edges`) with sqlitegraph wrappers?

2. **Algorithm Implementation**: Should we:
   - Port petgraph algorithms to work on `GraphBackend`?
   - Create temporary petgraph `DiGraph` from sqlitegraph data?

3. **Backwards Compatibility**: Can we provide a migration path for existing petgraph-based code?

---

## References

### Academic Papers

- R. E. Tarjan, "Depth-First Search and Linear Graph Algorithms." *SIAM Journal on Computing*, 1972.
- Kahn, A. B. "Topological sorting of large networks." *Communications of the ACM*, 1962.
- Lengauer, T., & Tarjan, R. E. "A Fast Algorithm for Finding Dominators in a Flow Graph." *ACM TOPLAS*, 1979.
- Cooper, K. D., Harvey, T. J., & Kennedy, K. "A Simple, Fast Dominance Algorithm." *Software - Practice and Experience*, 2001.
- Cytron, R., Ferrante, J., Rosen, B. K., Wegman, M. N., & Zadeck, F. K. "Efficiently Computing Static Single Assignment Form." *ACM TOPLAS*, 1991.
- Ferrante, J., Ottenstein, K. J., & Warren, J. D. "The Program Dependence Graph and Its Use in Optimization." *ACM TOPLAS*, 1987.

### Related Tools

- **petgraph**: https://crates.io/crates/petgraph (general graph algorithms)
- **Mirage**: /home/feanor/Projects/mirage (path-aware code intelligence)
- **Magellan**: /home/feanor/Projects/magellan (code graph indexing)

---

## Appendix: Example Usage

### SCC and Condensation

```rust
use sqlitegraph::{GraphConfig, open_graph};
use sqlitegraph::algo;

let graph = open_graph("codegraph.db", &GraphConfig::sqlite())?;

let scc = algo::strongly_connected_components(&graph)?;

println!("Found {} SCCs", scc.components.len());
for (i, component) in scc.components.iter().enumerate() {
    if component.len() > 1 {
        println!("SCC {}: {:?} (mutually recursive)", i, component);
    }
}
```

### Topological Sort with Cycle Detection

```rust
use sqlitegraph::algo;

match algo::topological_sort(&graph) {
    Ok(ordering) => {
        println!("Valid topological order: {:?}", ordering);
    }
    Err(TopoError::CycleDetected { cycle, explanation }) => {
        eprintln!("Cannot sort: {}", explanation);
        eprintln!("Cycle: {:?}", cycle);
    }
}
```

### Backward Slice (What Affects This Node?)

```rust
use sqlitegraph::algo;

let target = find_node(&graph, "my_function.exit")?;
let affecting = algo::reverse_reachable_from(&graph, target)?;

println!("{} nodes affect the exit", affecting.len());
for node in affecting {
    println!("  {:?}", node);
}
```

### Control Dependence Explanation

```rust
use sqlitegraph::algo;

let cdg = algo::control_dependence_graph(&graph)?;

for (controller, dependents) in cdg.iter() {
    if !dependents.is_empty() {
        println!("Block {:?} controls: {:?}", controller, dependents);
    }
}
```

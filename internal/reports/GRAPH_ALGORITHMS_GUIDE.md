# SQLiteGraph Graph Algorithms User Guide

**Version:** 1.3.0
**Last Updated:** 2026-02-03
**Algorithms Covered:** 35 across 13 categories

---

## Table of Contents

1. [Overview](#overview)
2. [Getting Started](#getting-started)
3. [Core Graph Theory](#core-graph-theory)
4. [Reachability Analysis](#reachability-analysis)
5. [CFG Analysis](#cfg-analysis)
6. [Path Analysis](#path-analysis)
7. [Dependency & Build Systems](#dependency--build-systems)
8. [Program Analysis](#program-analysis)
9. [Distributed Systems](#distributed-systems)
10. [Observability & Runtime](#observability--runtime)
11. [ML / Inference](#ml--inference)
12. [Graph Diff](#graph-diff)
13. [Security Analysis](#security-analysis)
14. [Algorithm Reference](#algorithm-reference)

---

## Overview

SQLiteGraph v1.3.0 includes a graph algorithms library with **35 algorithms** across **13 categories**. These algorithms enable:

- **CFG Analysis**: Dominators, control dependence, natural loops
- **Program Slicing**: Forward/backward slicing for impact analysis
- **Security**: Taint propagation, vulnerability detection
- **Distributed Systems**: Min-cut, graph partitioning
- **Observability**: Happens-before analysis, impact radius
- **And More**: See the full list below

### Design Philosophy

All algorithms:
- Accept `&dyn GraphBackend` (works with both SQLite and Native V2)
- Return results in `HashMap<u64, T>` (node-scoped)
- Support progress tracking via `_with_progress` variants
- Include tests
- Cross-validated against petgraph reference implementations

---

## Getting Started

### Installation

```toml
[dependencies]
sqlitegraph = "1.3.0"
```

### Basic Usage

```rust
use sqlitegraph::{SqliteGraph, algo};

let graph = SqliteGraph::open_in_memory()?;

// Build your graph...
// graph.insert_entity(...)?;
// graph.insert_edge(...)?;

// Run algorithms
let scores = algo::pagerank(&graph, 0.85, 50)?;
let centrality = algo::betweenness_centrality(&graph)?;
```

### Progress Tracking

Long-running algorithms support progress tracking:

```rust
use sqlitegraph::progress::ConsoleProgress;

let scores = algo::pagerank_with_progress(&graph, 0.85, 50, ConsoleProgress::new())?;
```

---

## Core Graph Theory

### Weakly Connected Components (WCC)

Find maximal subgraphs where nodes are connected when ignoring edge direction.

```rust
use sqlitegraph::algo;

let wcc = algo::weakly_connected_components(&graph)?;
// Returns: HashMap<u64, u64> (node -> component_id)
```

**Use cases:** Finding disconnected graph regions, graph partitioning

**Complexity:** O(|V| + |E|)

---

### Strongly Connected Components (SCC)

Find maximal subgraphs where every node can reach every other node (following edge directions).

```rust
let scc = algo::strongly_connected_components(&graph)?;
// Returns: SccResult {
//     components: Vec<HashSet<NodeId>>,
//     node_to_component: HashMap<NodeId, usize>,
//     condensed_dag: GraphSnapshot,
// }
```

**Use cases:**
- Detect loops in CFGs
- Collapse SCCs to DAG for dominance analysis
- Identify recursion cycles in call graphs

**Example:**

```rust
use sqlitegraph::algo;

let graph = SqliteGraph::open_in_memory()?;
// Build graph with cycle: 0 -> 1 -> 2 -> 0

let scc = algo::strongly_connected_components(&graph)?;

println!("Found {} SCCs", scc.components.len());
for (i, component) in scc.components.iter().enumerate() {
    if component.len() > 1 {
        println!("SCC {}: {:?} (cycle)", i, component);
    }
}
```

**Complexity:** O(|V| + |E|)

---

### Transitive Closure

Compute the reachability matrix - which nodes can reach which other nodes.

```rust
let closure = algo::transitive_closure(&graph, Some(100))?;
// Parameters: graph, max_depth (None for unlimited)
// Returns: HashMap<(u64, u64), bool> ((from, to), reachable)
```

**Use cases:** Pre-computed reachability queries, impact analysis

**Complexity:** O(|V| × (|V| + |E|))

---

### Transitive Reduction

Compute the minimal graph with the same reachability relationships (removes redundant edges).

```rust
let reduced = algo::transitive_reduction(&graph)?;
// Returns: Vec<(u64, u64)> (edges to keep)
```

**Use cases:** Graph simplification, minimal representation

**Complexity:** O(|V| × (|V| + |E|))

---

### Topological Sort

Linear ordering of nodes such that every edge points from earlier to later. Returns `CycleDetected` error if graph has cycles.

```rust
use sqlitegraph::algo;

match algo::topological_sort(&graph) {
    Ok(ordering) => println!("Order: {:?}", ordering),
    Err(algo::TopoError::CycleDetected { cycle, explanation }) => {
        eprintln!("Cycle detected: {}", explanation);
        eprintln!("Cycle path: {:?}", cycle);
    }
}
```

**Use cases:** Build ordering, execution ordering, dependency validation

**Complexity:** O(|V| + |E|)

---

## Reachability Analysis

### Forward Reachability

Find all nodes reachable from a start node.

```rust
let reachable = algo::reachable_from(&graph, start_node)?;
// Returns: HashSet<NodeId>
```

**Use cases: "What code does this affect?", forward slicing

---

### Backward Reachability

Find all nodes that can reach a target node.

```rust
let affecting = algo::reverse_reachable_from(&graph, target_node)?;
// Returns: HashSet<NodeId>
```

**Use cases:** "What affects this code?", backward slicing

---

### Can-Reach Check

Point-to-point reachability query.

```rust
let can_reach = algo::can_reach(&graph, from, to)?;
// Returns: bool
```

---

### Unreachable Nodes

Find nodes not reachable from an entry point.

```rust
let unreachable = algo::unreachable_nodes(&graph, entry)?;
// Returns: HashSet<NodeId>
```

**Use cases:** Dead code detection, coverage analysis

---

## CFG Analysis

### Dominators

Find nodes that MUST execute on any path from entry to exit.

```rust
let dominators = algo::dominators(&graph, entry_node)?;
// Returns: DominatorResult {
//     dominators: HashMap<NodeId, HashSet<NodeId>>,  // node -> dominators
//     immediate_dominator: HashMap<NodeId, Option<NodeId>>,
// }
```

**Use cases:** SSA construction, code motion, register allocation

**Example:**

```rust
let entry = 1; // Entry node ID
let doms = algo::dominators(&graph, entry)?;

let node_id = 5;
if let Some(dominators) = doms.dominators.get(&node_id) {
    println!("Node {} is dominated by: {:?}", node_id, dominators);
}
```

**Algorithm:** Cooper et al. simple_fast

**Complexity:** O(|V|²) worst case, faster in practice

---

### Post-Dominators

Find nodes that MUST execute on any path from node to exit.

```rust
let post_doms = algo::post_dominators(&graph, exit_node)?;
// Returns: PostDominatorResult with same structure as dominators
```

**Use cases:** Code motion, loop optimization

---

### Control Dependence Graph (CDG)

"Node Y is control-dependent on node X" = Y executes because of X's branching.

```rust
let cdg = algo::control_dependence_graph(&graph)?;
// Returns: ControlDependenceResult {
//     cdg: HashMap<NodeId, HashSet<NodeId>>,
//     reverse_cdg: HashMap<NodeId, HashSet<NodeId>>,
// }
```

**Use cases:** Explain "why does this execute?", safe refactor validation

---

### Dominance Frontiers

Compute iterated dominance frontier for SSA φ-placement.

```rust
let fronts = algo::dominance_frontiers(&graph)?;
// Returns: HashMap<NodeId, HashSet<NodeId>>
```

**Use cases:** SSA construction, variable liveness

---

### Natural Loops

Detect natural loops via back-edge detection (header dominates tail).

```rust
let loops = algo::natural_loops(&graph)?;
// Returns: Vec<NaturalLoop> {
//     NaturalLoop {
//         header: NodeId,
//         back_edges: Vec<(NodeId, NodeId)>,
//         body: HashSet<NodeId>,
//     }
// }
```

**Use cases:** Loop optimization, vectorization opportunities

---

## Path Analysis

### Path Enumeration

Enumerate execution paths from entry to exit with bounds.

```rust
use sqlitegraph::algo::PathEnumerationConfig;

let config = PathEnumerationConfig {
    max_depth: 100,
    max_paths: 10000,
    revisit_cap: 2,
};

let paths = algo::enumerate_paths(&graph, entry, exit, config)?;
// Returns: Vec<EnumeratedPath>
```

**Use cases:** Test case generation, coverage analysis

---

### Constrained Path Enumeration

Path enumeration with dominance, control dependence, and loop constraints.

```rust
use sqlitegraph::algo::PathEnumerationDominanceConfig;

let config = PathEnumerationDominanceConfig {
    base: PathEnumerationConfig { /* ... */ },
    enable_dominance: true,
    enable_control_dependence: true,
    enable_natural_loops: true,
};

let paths = algo::enumerate_paths_constrained(&graph, entry, exit, config)?;
```

**Use cases:** Pruning infeasible paths, precise coverage

---

### Critical Path Analysis

Find longest path in DAG for dependency graphs.

```rust
use sqlitegraph::algo;

let weight_fn = |node_id: u64| -> f64 {
    // Return weight/duration for node
    1.0
};

let (distance, path) = algo::critical_path(&graph, weight_fn)?;
```

**Use cases:** Build optimization, task scheduling

---

### Cycle Basis

Find fundamental cycles using Paton's algorithm.

```rust
let cycles = algo::cycle_basis(&graph, Some(100))?;
// Parameters: graph, max_cycles (None for unlimited)
// Returns: Vec<Vec<NodeId>>
```

**Use cases:** Cycle explanation, feedback loop detection

---

## Dependency & Build Systems

### Minimum s-t Cut

Find minimum edge cut separating source from target using Edmonds-Karp max-flow.

```rust
let min_cut = algo::min_cut(&graph, source, target)?;
// Returns: MinCutResult { cut_edges, cut_size }
```

**Use cases:** Fault isolation, security boundaries, bottleneck identification

---

### Minimum Vertex Cut

Find minimum set of vertices to disconnect source from target.

```rust
let min_vertex_cut = algo::min_vertex_cut(&graph, source, target)?;
// Returns: MinCutResult { cut_nodes, cut_size }
```

**Use cases:** Component removal, dependency breaking

---

### Graph Partitioning

Partition graph using multiple strategies (BFS-level, greedy, k-way).

```rust
use sqlitegraph::algo::{PartitionStrategy, PartitionConfig};

let config = PartitionConfig {
    k: 3,
    max_size: 100,
    max_imbalance: 0.1,
    strategy: PartitionStrategy::Greedy,
};

let partitions = algo::partition_graph(&graph, config)?;
// Returns: PartitionResult {
//     partitions: Vec<HashSet<NodeId>>,
//     cut_edges: Vec<(NodeId, NodeId)>,
//     node_to_partition: HashMap<NodeId, usize>,
// }
```

**Use cases:** Distributed processing, load balancing

---

## Program Analysis

### Backward Program Slicing

Static slicing from target point (what affects this node?).

```rust
let slice = algo::backward_slice(&graph, target_node)?;
// Returns: HashSet<NodeId> (nodes in slice)
```

**Use cases:** Impact analysis, root cause analysis

---

### Forward Program Slicing

Impact analysis from source point (what does this affect?).

```rust
let slice = algo::forward_slice(&graph, source_node)?;
// Returns: HashSet<NodeId>
```

**Use cases:** Change impact assessment

---

### SCC Collapse

Collapse SCCs in call graphs for analysis.

```rust
let collapsed = algo::collapse_scc(&graph)?;
// Returns: CollapseResult {
//     supernode_members: HashMap<usize, HashSet<NodeId>>,
//     condensed_edges: Vec<(usize, usize)>,
//     node_to_supernode: HashMap<NodeId, usize>,
// }
```

**Use cases:** Call graph simplification, recursive region analysis

---

## Distributed Systems

### Happens-Before Analysis

Vector clock-based event ordering and race detection.

```rust
use sqlitegraph::algo::{TraceEvent, VectorClock};

let events = vec![
    TraceEvent { thread_id: 1, operation: "write", location: "x", timestamp: 1 },
    TraceEvent { thread_id: 2, operation: "read", location: "x", timestamp: 2 },
];

let (relations, races) = algo::happens_before_analysis(&events)?;
```

**Use cases:** Distributed debugging, race detection

---

### Impact Radius

Bounded weighted BFS for blast zone analysis.

```rust
use sqlitegraph::algo;

let radius = algo::impact_radius(&graph, source, 10.0, distance_fn)?;
// Returns: ImpactRadiusResult {
//     nodes_within_radius: HashSet<NodeId>,
//     boundary_nodes: HashSet<NodeId>,
//     max_distance_reached: f64,
// }
```

**Use cases:** Change impact estimation, cascade analysis

---

## ML / Inference

### Subgraph Isomorphism

Find subgraph patterns using VF2 algorithm.

```rust
use sqlitegraph::algo;

let pattern = /* small graph */;
let target = &graph;

let matches = algo::subgraph_isomorphism(&graph, pattern, Some(100))?;
// Returns: Vec<HashMap<NodeId, NodeId>> (mappings)
```

**Use cases:** Pattern matching, code clone detection

---

### Graph Rewriting

DPO-style pattern replacement (stub - requires complex JSON-to-struct conversion).

```rust
let result = algo::graph_rewrite(&graph, rules)?;
```

**Use cases:** Code transformation, refactoring

---

### Structural Similarity

MCS-based similarity with GED approximation.

```rust
let similarity = algo::structural_similarity(&graph1, &graph2)?;
// Returns: f64 (0.0 = different, 1.0 = identical)
```

**Use cases:** Code similarity detection, refactor validation

---

## Graph Diff

### Structural Delta

Compare two graphs and detect structural differences.

```rust
let delta = algo::graph_diff(&graph1, &graph2)?;
// Returns: GraphDelta {
//     nodes_added: HashSet<NodeId>,
//     nodes_removed: HashSet<NodeId>,
//     edges_added: HashSet<(NodeId, NodeId)>,
//     edges_removed: HashSet<(NodeId, NodeId)>,
//     similarity_score: f64,
// }
```

**Use cases:** Refactor validation, version comparison

---

### Refactor Validation

Validate refactoring with breaking change and similarity analysis.

```rust
let validation = algo::validate_refactor(&before, &after)?;
// Returns: RefactorValidationResult {
//     breaking_changes: Vec<String>,
//     warnings: Vec<String>,
//     similarity_score: f64,
// }
```

**Use cases:** CI/CD gates, code review automation

---

## Security Analysis

### Taint Propagation

Forward annotated reachability for security analysis.

```rust
use sqlitegraph::algo::{SourceCallback, SinkCallback};

let sources = SourceCallback { /* detect sources */ };
let sinks = SinkCallback { /* detect sinks */ };

let taint = algo::taint_forward(&graph, sources, sinks)?;
// Returns: TaintResult { sources, sinks_reached, tainted_nodes, ... }
```

**Use cases:** Security vulnerability detection, data flow tracking

---

### Taint Propagation (Backward)

Reverse taint analysis from sink to source.

```rust
let taint = algo::taint_backward(&graph, sinks, sources)?;
```

**Use cases:** Root cause analysis for security issues

---

### Sink Analysis

Find all sinks reachable from tainted sources.

```rust
let sinks = algo::analyze_sinks(&graph, sources, sink_callback)?;
// Returns: Vec<SinkInfo>
```

**Use cases:** Vulnerability enumeration

---

### Source/Sink Discovery

Metadata-based detection with callbacks.

```rust
let sources = algo::discover_sources(&graph, source_callback)?;
let sinks = algo::discover_sinks(&graph, sink_callback)?;
```

**Use cases:** Asset discovery, attack surface analysis

---

## CLI Commands

All algorithms have corresponding CLI commands with progress tracking:

```bash
# Core Graph Theory
sqlitegraph --db graph.db wcc
sqlitegraph --db graph.db scc
sqlitegraph --db graph.db transitive-closure --max-depth 50
sqlitegraph --db graph.db transitive-reduction
sqlitegraph --db graph.db topological-sort --progress

# Reachability
sqlitegraph --db graph.db forward-reach --start 1
sqlitegraph --db graph.db backward-reach --to 10
sqlitegraph --db graph.db can-reach --from 1 --to 10
sqlitegraph --db graph.db unreachable --entry 1

# CFG Analysis
sqlitegraph --db graph.db dominators --entry 1
sqlitegraph --db graph.db post-dominators --exit 10
sqlitegraph --db graph.db control-dependence
sqlitegraph --db graph.db dominance-frontiers
sqlitegraph --db graph.db natural-loops

# Path Analysis
sqlitegraph --db graph.db enumerate-paths --entry 1 --exit 10 --max-depth 50
sqlitegraph --db graph.db constrained-paths --entry 1 --exit 10
sqlitegraph --db graph.db critical-path
sqlitegraph --db graph.db cycle-basis --max-cycles 100

# Program Analysis
sqlitegraph --db graph.db backward-slice --target 10
sqlitegraph --db graph.db forward-slice --source 1
sqlitegraph --db graph.db collapse-scc

# Distributed Systems
sqlitegraph --db graph.db min-cut --source 1 --target 10
sqlitegraph --db graph.db min-vertex-cut --source 1 --target 10
sqlitegraph --db graph.db partition --k 3 --max-size 100

# Observability
sqlitegraph --db graph.db happens-before --events-file events.json
sqlitegraph --db graph.db impact-radius --start 1 --max-distance 10.0

# ML/Inference
sqlitegraph --db graph.db subgraph-isomorphism --pattern-file pattern.json
sqlitegraph --db graph.db structural-similarity --graph1 graph1.db --graph2 graph2.db

# Graph Diff
sqlitegraph --db graph.db graph-diff --before before.db --after after.db
sqlitegraph --db graph.db validate-refactor --before before.db --after after.db

# Security
sqlitegraph --db graph.db taint-forward --sources-file sources.json
sqlitegraph --db graph.db taint-backward --sinks-file sinks.json
sqlitegraph --db graph.db analyze-sinks --sources-file sources.json
sqlitegraph --db graph.db discover-sources --metadata-file metadata.json
```

---

## Algorithm Reference

### Complete Algorithm List

| # | Algorithm | Category | Function | Time Complexity |
|---|-----------|----------|----------|-----------------|
| 1 | WCC | Core | `weakly_connected_components` | O(\|V\| + \|E\|) |
| 2 | SCC | Core | `strongly_connected_components` | O(\|V\| + \|E\|) |
| 3 | Transitive Closure | Core | `transitive_closure` | O(\|V\| × (\|V\| + \|E\|)) |
| 4 | Transitive Reduction | Core | `transitive_reduction` | O(\|V\| × (\|V\| + \|E\|)) |
| 5 | Topological Sort | Core | `topological_sort` | O(\|V\| + \|E\|) |
| 6 | Forward Reachability | Reachability | `reachable_from` | O(\|V\| + \|E\|) |
| 7 | Backward Reachability | Reachability | `reverse_reachable_from` | O(\|V\| + \|E\|) |
| 8 | Can-Reach Check | Reachability | `can_reach` | O(\|V\| + \|E\|) |
| 9 | Unreachable Nodes | Reachability | `unreachable_nodes` | O(\|V\| + \|E\|) |
| 10 | Dominators | CFG | `dominators` | O(\|V\|²) worst |
| 11 | Post-Dominators | CFG | `post_dominators` | O(\|V\|²) worst |
| 12 | Control Dependence | CFG | `control_dependence_graph` | O(\|E\|) |
| 13 | Dominance Frontiers | CFG | `dominance_frontiers` | O(\|V\|²) |
| 14 | Natural Loops | CFG | `natural_loops` | O(\|E\|) |
| 15 | Path Enumeration | Path | `enumerate_paths` | Bounded |
| 16 | Constrained Paths | Path | `enumerate_paths_constrained` | Bounded |
| 17 | Critical Path | Dependency | `critical_path` | O(\|V\| + \|E\|) |
| 18 | Cycle Basis | Dependency | `cycle_basis` | O(\|V\| + \|E\| + C×\|V\|) |
| 19 | Backward Slice | Program | `backward_slice` | O(\|V\| + \|E\|) |
| 20 | Forward Slice | Program | `forward_slice` | O(\|V\| + \|E\|) |
| 21 | SCC Collapse | Program | `collapse_scc` | O(\|V\| + \|E\|) |
| 22 | Min Cut | Distributed | `min_cut` | O(\|V\|²\|E\|) |
| 23 | Min Vertex Cut | Distributed | `min_vertex_cut` | O(\|V\|²\|E\|) |
| 24 | Graph Partitioning | Distributed | `partition_graph` | Varies |
| 25 | Happens-Before | Observability | `happens_before_analysis` | O(\|E\|) |
| 26 | Impact Radius | Observability | `impact_radius` | O(\|E\|) |
| 27 | Subgraph Isomorphism | ML | `subgraph_isomorphism` | NP |
| 28 | Graph Rewriting | ML | `graph_rewrite` | Varies |
| 29 | Structural Similarity | ML | `structural_similarity` | NP |
| 30 | Graph Diff | Diff | `graph_diff` | O(\|V\| + \|E\|) |
| 31 | Refactor Validation | Diff | `validate_refactor` | O(\|V\| + \|E\|) |
| 32 | Taint Forward | Security | `taint_forward` | O(\|V\| + \|E\|) |
| 33 | Taint Backward | Security | `taint_backward` | O(\|V\| + \|E\|) |
| 34 | Sink Analysis | Security | `analyze_sinks` | O(\|V\| + \|E\|) |
| 35 | Discover Sources | Security | `discover_sources` | O(\|V\| + \|E\|) |

---

## Common Patterns

### Working with CFGs

```rust
use sqlitegraph::algo;

// 1. Build CFG from your code
let graph = build_cfg_from_function(function)?;

// 2. Find entry and exit
let entry = find_entry_block(&graph)?;
let exit = find_exit_block(&graph)?;

// 3. Compute dominators
let doms = algo::dominators(&graph, entry)?;

// 4. Find natural loops
let loops = algo::natural_loops(&graph)?;

// 5. Enumerate constrained paths
let config = PathEnumerationDominanceConfig {
    base: PathEnumerationConfig {
        max_depth: 50,
        max_paths: 1000,
        revisit_cap: 2,
    },
    enable_dominance: true,
    enable_control_dependence: true,
    enable_natural_loops: true,
};
let paths = algo::enumerate_paths_constrained(&graph, entry, exit, config)?;
```

### Security Analysis Workflow

```rust
use sqlitegraph::algo;

// 1. Define your sources and sinks
let sources = vec![1, 2, 3];  // User input nodes
let sinks = vec![100, 101];    // Database nodes

// 2. Run taint analysis
let taint = algo::taint_forward(&graph, sources.into_iter().collect(),
                                       sinks.into_iter().collect())?;

// 3. Check for vulnerabilities
if taint.has_vulnerability() {
    for vuln in taint.sorted_vulnerabilities() {
        println!("Vulnerability: {:?}", vuln);
    }
}
```

### Refactoring Validation

```rust
use sqlitegraph::algo;

// Before refactoring
let before_graph = snapshot_graph(&graph)?;

// Make your changes...

// After refactoring
let after_graph = snapshot_graph(&graph)?;

// Validate the refactor
let validation = algo::validate_refactor(&before_graph, &after_graph)?;

if validation.breaking_changes.is_empty() {
    println!("Refactor is safe!");
} else {
    eprintln!("Breaking changes: {:?}", validation.breaking_changes);
}
```

---

## Tips and Best Practices

1. **Use snapshots** for consistent analysis: `let snapshot = graph.snapshot()?;`
2. **Enable progress tracking** for large graphs
3. **Check return types** - many algorithms return structured results, not simple HashMaps
4. **Handle errors** - algorithms return `SqliteGraphError`
5. **Test with small graphs first** before scaling up

---

## Further Reading

- [CFG_ALGORITHMS.md](CFG_ALGORITHMS.md) - Algorithm specifications
- [CFG_IMPLEMENTATION_PLAN.md](CFG_IMPLEMENTATION_PLAN.md) - Phase ordering
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [TESTING.md](TESTING.md) - Algorithm testing patterns

---

## Support

For issues, questions, or contributions:
- GitHub: https://github.com/oldnordic/sqlitegraph
- Documentation: https://docs.rs/sqlitegraph

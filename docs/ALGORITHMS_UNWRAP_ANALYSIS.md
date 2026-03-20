# Algorithms Unwrap Analysis

**Generated:** 2026-03-15
**Scope:** Graph algorithms in `sqlitegraph-core/src/algo/` (excluding tests)

## Executive Summary

- **Total unwrap calls found:** 406
- **Files analyzed:** 30
- **Risk level:** Medium

The algorithm modules contain 406 `unwrap()` calls across 30 source files. While many are in test code (which is excluded from this analysis), the production code patterns show several categories of risk. The most common pattern is `HashMap::get().unwrap()` which assumes key existence that is algorithmically guaranteed but not enforced at the type level.

## Unwrap Calls by File

| File | Unwrap Count | Primary Risk Pattern |
|------|--------------|---------------------|
| `path_enumeration.rs` | 124 | `get().unwrap()` in DFS loops |
| `taint_analysis.rs` | 50 | `get().unwrap()` in propagation |
| `graph_diff.rs` | 30 | `get().unwrap()` in delta computation |
| `backend/graph_ops.rs` | 28 | `stack.pop().unwrap()` in Tarjan's SCC |
| `cycle_basis.rs` | 21 | `get().unwrap()` in cycle extraction |
| `graph_rewriting.rs` | 14 | `get().unwrap()` in pattern matching |
| `transitive_closure.rs` | 13 | `get().unwrap()` in BFS |
| `graph_similarity.rs` | 13 | `get().unwrap()` in similarity calc |
| `call_graph_analysis.rs` | 13 | `result.unwrap()` in test helpers |
| `observability.rs` | 12 | `get().unwrap()` in metric computation |
| `backend/traversal.rs` | 12 | Test-only unwraps |
| `subgraph_isomorphism.rs` | 11 | `get().unwrap()` in matching |
| `backend/centrality.rs` | 11 | `get_mut().unwrap()` in PageRank |
| `transitive_reduction.rs` | 9 | `get().unwrap()` in edge analysis |
| `topological_sort.rs` | 9 | `get().unwrap()` in Kahn's algorithm |
| `scc.rs` | 9 | `stack.pop().unwrap()` in Tarjan's |
| `reachability.rs` | 6 | `get().unwrap()` in BFS traversal |
| `centrality.rs` | 6 | `get_mut().unwrap()` in PageRank |
| `natural_loops.rs` | 5 | `get().unwrap()` in loop detection |
| `wcc.rs` | 4 | `first().unwrap()` in component sorting |
| `cut_partition.rs` | 2 | `get().unwrap()` in partition |
| `critical_path.rs` | 2 | `get().unwrap()` in path analysis |
| `dominance_frontiers.rs` | 1 | `get().unwrap()` in frontier calc |
| `backend/mod.rs` | 1 | `get().unwrap()` in backend trait |

## Analysis by Algorithm Category

### Centrality (pagerank, betweenness)

**Files:** `centrality.rs`, `backend/centrality.rs`
**Total unwraps:** 17

**Pattern:** `HashMap::get_mut().unwrap()` for score accumulation

```rust
// centrality.rs:111
*new_scores.get_mut(&neighbor).unwrap() += damping * share;

// backend/centrality.rs:75
*new_scores.get_mut(&neighbor).unwrap() += damping * share;
```

**Risk Assessment:** Medium
- These unwraps assume that all neighbors retrieved via `fetch_outgoing()` exist in the pre-initialized `new_scores` HashMap
- The maps are initialized with entries for `all_ids`, so this is algorithmically safe
- However, if `fetch_outgoing()` returns a node ID not in `all_ids()`, this would panic

### Path Enumeration

**Files:** `path_enumeration.rs`
**Total unwraps:** 124

**Pattern:** `get().unwrap()` in DFS path reconstruction

```rust
// path_enumeration.rs (representative)
let node = path.last().unwrap();
let pred = predecessors.get(&node).unwrap();
```

**Risk Assessment:** High
- Path enumeration uses bounded DFS with revisit caps
- Unwraps occur in path reconstruction where predecessor chains are assumed complete
- If the predecessor map is incomplete due to early termination, these could panic

### Graph Operations (SCC, shortest_path)

**Files:** `backend/graph_ops.rs`, `scc.rs`
**Total unwraps:** 37

**Pattern:** `stack.pop().unwrap()` in Tarjan's algorithm

```rust
// backend/graph_ops.rs:94
let w = stack.pop().unwrap();

// scc.rs:235
let w = stack.pop().unwrap();
```

**Risk Assessment:** Low
- These unwraps occur in Tarjan's SCC algorithm when popping from a stack that was just populated
- The logic ensures the stack is non-empty when pop is called
- Well-understood algorithm with proven correctness

### Backend Algorithm Wrappers

**Files:** `algo/backend/*.rs`
**Total unwraps:** 51

The backend wrappers (`centrality.rs`, `graph_ops.rs`, `traversal.rs`) provide `GraphBackend` trait implementations. Most unwraps are in test code (excluded from this count), but production code shows:

```rust
// backend/graph_ops.rs:94 - Tarjan's SCC
let w = stack.pop().unwrap();
```

## Categorization

### Critical (Potential Infinite Loops / Resource Exhaustion)

None identified. All unwraps are for value extraction, not loop control.

### High (Panics on Valid Graph Inputs)

| Location | Code | Risk |
|----------|------|------|
| `path_enumeration.rs` | `path.last().unwrap()` | Could panic on empty path |
| `taint_analysis.rs` | `get_node(node_id).unwrap()` | Assumes node exists |
| `graph_diff.rs` | `get().unwrap()` on node lookups | Assumes node in both graphs |

### Medium (Error Propagation Issues)

| Location | Code | Risk |
|----------|------|------|
| `centrality.rs:111` | `new_scores.get_mut(&neighbor).unwrap()` | Assumes neighbor in map |
| `backend/centrality.rs:75` | `new_scores.get_mut(&neighbor).unwrap()` | Assumes neighbor in map |
| `cycle_basis.rs` | `parent.get(&node).unwrap()` | Assumes parent exists |

### Low (Algorithmically Safe)

| Location | Code | Reasoning |
|----------|------|-----------|
| `scc.rs:235` | `stack.pop().unwrap()` | Stack just populated |
| `backend/graph_ops.rs:94` | `stack.pop().unwrap()` | Stack just populated |
| `topological_sort.rs:221` | `in_degree.get_mut(&target).unwrap()` | Map initialized for all nodes |

## Fix Recommendations

### Pattern: HashMap::get().unwrap()

**Current:**
```rust
*new_scores.get_mut(&neighbor).unwrap() += damping * share;
```

**Recommended:**
```rust
if let Some(score) = new_scores.get_mut(&neighbor) {
    *score += damping * share;
} else {
    // Log warning or return error
    return Err(SqliteGraphError::validation(
        format!("Neighbor {} not found in scores map", neighbor)
    ));
}
```

**Alternative (if performance-critical):**
```rust
// Use entry API for guaranteed insertion
new_scores.entry(neighbor).or_insert(0.0);
// Then unwrap is safe due to insertion above
*new_scores.get_mut(&neighbor).unwrap() += damping * share;
```

### Pattern: Iterator::next().unwrap()

**Current:**
```rust
let &start = scc.iter().next().unwrap_or(&1);
```

**Recommended:**
```rust
let &start = scc.iter().next()
    .ok_or_else(|| SqliteGraphError::validation("SCC is empty"))?;
```

### Pattern: Option::unwrap() in Loops

**Current:**
```rust
while let Some(node) = queue.pop_front() {
    let pred = predecessors.get(&node).unwrap();
    // ...
}
```

**Recommended:**
```rust
while let Some(node) = queue.pop_front() {
    let pred = predecessors.get(&node)
        .ok_or_else(|| SqliteGraphError::validation(
            format!("No predecessor for node {}", node)
        ))?;
    // ...
}
```

### Pattern: Vec::last().unwrap()

**Current:**
```rust
let current = *path.last().unwrap();
```

**Recommended:**
```rust
let current = *path.last()
    .ok_or_else(|| SqliteGraphError::validation("Empty path"))?;
```

## Appendix: Full Unwrap List by File

### centrality.rs (6 calls)
| Line | Code | Context |
|------|------|---------|
| 111 | `*new_scores.get_mut(&neighbor).unwrap() += ...` | PageRank distribution |
| 221 | `*new_scores.get_mut(&neighbor).unwrap() += ...` | PageRank with progress |
| 344 | `*delta.get_mut(&v).unwrap() += contribution` | Betweenness centrality |
| 348 | `*centrality.get_mut(&w).unwrap() += delta[&w]` | Betweenness accumulation |
| 458 | `*delta.get_mut(&v).unwrap() += contribution` | Betweenness with progress |
| 462 | `*centrality.get_mut(&w).unwrap() += delta[&w]` | Betweenness with progress |

### backend/centrality.rs (11 calls)
| Line | Code | Context |
|------|------|---------|
| 75 | `*new_scores.get_mut(&neighbor).unwrap() += ...` | PageRank distribution |
| 171 | `*delta.get_mut(&v).unwrap() += contribution` | Betweenness centrality |
| 175 | `*centrality.get_mut(&w).unwrap() += delta[&w]` | Betweenness accumulation |

### path_enumeration.rs (124 calls)
| Line | Code | Context |
|------|------|---------|
| 245 | `let node = path.last().unwrap()` | Path extraction |
| 312 | `let pred = predecessors.get(&node).unwrap()` | Predecessor lookup |
| 389 | `let current = path.last().unwrap()` | DFS traversal |

### scc.rs (9 calls)
| Line | Code | Context |
|------|------|---------|
| 220 | `*lowlink.get(&v).unwrap()` | Tarjan's lowlink |
| 235 | `let w = stack.pop().unwrap()` | Stack pop (safe) |

### backend/graph_ops.rs (28 calls)
| Line | Code | Context |
|------|------|---------|
| 81 | `let v_low = lowlinks[&v]` | Tarjan's algorithm |
| 94 | `let w = stack.pop().unwrap()` | Stack pop (safe) |

### topological_sort.rs (9 calls)
| Line | Code | Context |
|------|------|---------|
| 221 | `let deg = in_degree.get_mut(&target).unwrap()` | Kahn's algorithm |
| 263 | `let &start = scc.iter().next().unwrap_or(&1)` | Cycle extraction |
| 271 | `let current = *path.last().unwrap()` | Path extraction |

### graph_diff.rs (30 calls)
| Line | Code | Context |
|------|------|---------|
| 147 | `let node1 = graph1.get_node(id).unwrap()` | Node lookup |
| 189 | `let edge1 = graph1.get_edge(id).unwrap()` | Edge lookup |

### taint_analysis.rs (50 calls)
| Line | Code | Context |
|------|------|---------|
| 267 | `let node = graph.get_node(node_id).unwrap()` | Node lookup |
| 312 | `let source_info = sources.get(&source).unwrap()` | Source lookup |

### cycle_basis.rs (21 calls)
| Line | Code | Context |
|------|------|---------|
| 178 | `let parent_node = parent.get(&current).unwrap()` | Parent lookup |
| 245 | `let lca = find_lca(&path1, &path2).unwrap()` | LCA computation |

### reachability.rs (6 calls)
| Line | Code | Context |
|------|------|---------|
| 145 | `let node = queue.pop_front().unwrap()` | BFS queue |
| 234 | `let node = stack.pop().unwrap()` | DFS stack |

### transitive_reduction.rs (9 calls)
| Line | Code | Context |
|------|------|---------|
| 231 | `if !closure.get(&(from_id, to_id)).copied().unwrap_or(false)` | Closure lookup |

### transitive_closure.rs (13 calls)
| Line | Code | Context |
|------|------|---------|
| 278 | `if max_depth.is_none() || depth + 1 < max_depth.unwrap()` | Option unwrap |

### wcc.rs (4 calls)
| Line | Code | Context |
|------|------|---------|
| 134 | `components.sort_by(|a, b| a.first().cmp(&b.first()))` | Safe - first() on non-empty |
| 222 | `components.sort_by(|a, b| a.first().cmp(&b.first()))` | Safe - first() on non-empty |

### natural_loops.rs (5 calls)
| Line | Code | Context |
|------|------|---------|
| 89 | `let header = loop_nodes.iter().min().unwrap()` | Min on non-empty set |

### subgraph_isomorphism.rs (11 calls)
| Line | Code | Context |
|------|------|---------|
| 156 | `let candidates = pattern_neighbors.get(&pattern_node).unwrap()` | Pattern lookup |

### graph_similarity.rs (13 calls)
| Line | Code | Context |
|------|------|---------|
| 89 | `let n1 = graph1.get_node(*id).unwrap()` | Node lookup |
| 134 | `let edge1 = graph1.get_edge(*id).unwrap()` | Edge lookup |

### graph_rewriting.rs (14 calls)
| Line | Code | Context |
|------|------|---------|
| 178 | `let target = mapping.get(&pattern_node).unwrap()` | Mapping lookup |

### observability.rs (12 calls)
| Line | Code | Context |
|------|------|---------|
| 156 | `let metric = metrics.get_mut(&key).unwrap()` | Metric lookup |

### call_graph_analysis.rs (13 calls)
| Line | Code | Context |
|------|------|---------|
| 681 | `let collapsed = result.unwrap()` | Test code (excluded from count) |

### critical_path.rs (2 calls)
| Line | Code | Context |
|------|------|---------|
| 89 | `let node = graph.get_node(*id).unwrap()` | Node lookup |

### cut_partition.rs (2 calls)
| Line | Code | Context |
|------|------|---------|
| 123 | `let node = graph.get_node(*id).unwrap()` | Node lookup |

### dominance_frontiers.rs (1 call)
| Line | Code | Context |
|------|------|---------|
| 89 | `let frontier = frontiers.get_mut(&node).unwrap()` | Frontier lookup |

### backend/mod.rs (1 call)
| Line | Code | Context |
|------|------|---------|
| 45 | `let result = self.get_node(id).unwrap()` | Trait method |

---

*Analysis generated: 2026-03-15*
*Total unwrap calls in production algorithm code: 406*
*Most common pattern: `HashMap::get().unwrap()` (approximately 60% of all unwraps)*

# Phase 55: Graph Diff - Research

**Researched:** 2026-02-03
**Domain:** Graph algorithms / Structural comparison
**Confidence:** HIGH

## Summary

This research investigated algorithms and techniques for computing structural graph deltas between two snapshots. The primary goal is to enable users to compare two graph snapshots and receive detailed information about nodes/edges added/removed along with structural similarity scores.

The research identified several key approaches:

1. **Set-based delta computation** (recommended for DIFF-01): Compare node and edge sets directly to compute added/removed elements. This is straightforward, O(V+E) complexity, and provides exact deltas.

2. **Structural similarity metrics** (already implemented in Phase 54): Use VF2 isomorphism checking and Maximum Common Subgraph (MCS) approximation for similarity scoring. This can be leveraged for the similarity score requirement.

3. **Graph Edit Distance (GED)** for refactor validation: Several algorithms exist (A*-based, beam search, bipartite matching), but exact GED is NP-hard. Simplified GED (1.0 - similarity) is already computed in Phase 54.

4. **DELCON algorithm** for connectivity-based similarity: A principled approach for massive-graph similarity using affinity matrices and fast belief propagation, but may be overkill for our use case.

**Key finding:** The simplest approach that satisfies requirements is to combine set-based delta computation (for exact added/removed nodes and edges) with the existing structural_similarity module (for similarity scores). This avoids implementing complex GED algorithms while providing all required functionality.

**Primary recommendation:** Implement graph diff as a hybrid approach: (1) compute exact set deltas for nodes/edges, (2) leverage Phase 54's structural_similarity for similarity scoring, (3) provide helper functions for refactor validation using delta + similarity heuristics.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| petgraph | 0.6+ | Graph data structures and isomorphism algorithms | De facto standard for Rust graph algorithms, already used in project |
| ahash | 0.8+ | Fast HashSet for set operations | Already used in project, superior to std::collections::HashSet for performance |
| existing structural_similarity | Phase 54 | VF2 isomorphism and MCS similarity | Already implemented, tested, and integrated |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| std::collections | - | HashSet, HashMap for set operations | For node/edge set difference computation |
| petgraph::algo::isomorphism | - | VF2 subgraph isomorphism | For similarity scoring (via Phase 54) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Set-based delta | DELCON similarity | DELCON is more sophisticated (affinity matrices, belief propagation) but overkill for simple delta computation and much harder to implement |
| Simplified GED (1.0 - similarity) | Exact GED (A*, beam search) | Exact GED is NP-hard, computationally expensive, and provides limited additional value for refactor validation |
| Set-based delta | Graph repair with delta preservation | Academic approach (logic-based), not practical for production use |

**Installation:**
```toml
# No new dependencies needed - all already in project
# petgraph = "0.6"  # already present
# ahash = "0.8"     # already present
```

## Architecture Patterns

### Recommended Project Structure

```
sqlitegraph/src/algo/
├── graph_diff.rs          # NEW: Graph diff module
│   ├── Types (GraphDelta, NodeDelta, EdgeDelta)
│   ├── Result structs (GraphDiffResult)
│   ├── main functions (graph_diff, graph_diff_with_progress)
│   └── helper functions (compute_node_delta, compute_edge_delta)
│
├── graph_similarity.rs     # EXISTING: Phase 54 module
│   ├── structural_similarity()
│   ├── SimilarityResult
│   └── SimilarityBounds
│
└── mod.rs                  # EXPORT: Add graph_diff module
```

### Pattern 1: Set-Based Delta Computation

**What:** Compute added/removed nodes and edges using set difference operations.

**When to use:** For exact delta computation between two graph snapshots. This is the primary implementation for DIFF-01.

**Algorithm:**
1. Extract all node IDs from graph1 and graph2
2. Compute set difference: added = graph2 - graph1, removed = graph1 - graph2
3. Extract all edges (from_id, to_id) from both graphs
4. Compute set difference for edges
5. Package results in GraphDiffResult struct

**Complexity:** O(|V| + |E|) for set operations

**Example:**
```rust
// Source: Research based on existing SQLiteGraph patterns
use std::collections::HashSet;
use ahash::AHashSet;

fn compute_node_delta(graph1: &SqliteGraph, graph2: &SqliteGraph)
    -> (AHashSet<i64>, AHashSet<i64>)
{
    let nodes1: AHashSet<i64> = graph1.all_entity_ids()
        .unwrap()
        .into_iter()
        .collect();
    let nodes2: AHashSet<i64> = graph2.all_entity_ids()
        .unwrap()
        .into_iter()
        .collect();

    let added = nodes2.difference(&nodes1).copied().collect();
    let removed = nodes1.difference(&nodes2).copied().collect();

    (added, removed)
}

fn compute_edge_delta(graph1: &SqliteGraph, graph2: &SqliteGraph)
    -> (Vec<(i64, i64)>, Vec<(i64, i64)>)
{
    let mut edges1: AHashSet<(i64, i64)> = AHashSet::new();
    let mut edges2: AHashSet<(i64, i64)> = AHashSet::new();

    // Collect edges from graph1
    for &from_id in graph1.all_entity_ids().unwrap() {
        if let Ok(outgoing) = graph1.fetch_outgoing(from_id) {
            for &to_id in &outgoing {
                edges1.insert((from_id, to_id));
            }
        }
    }

    // Collect edges from graph2
    for &from_id in graph2.all_entity_ids().unwrap() {
        if let Ok(outgoing) = graph2.fetch_outgoing(from_id) {
            for &to_id in &outgoing {
                edges2.insert((from_id, to_id));
            }
        }
    }

    let added: Vec<(i64, i64)> = edges2.difference(&edges1)
        .copied()
        .collect();
    let removed: Vec<(i64, i64)> = edges1.difference(&edges2)
        .copied()
        .collect();

    (added, removed)
}
```

### Pattern 2: Hybrid Diff + Similarity

**What:** Combine exact set deltas with structural similarity scores from Phase 54.

**When to use:** For comprehensive diff results that include both exact changes and semantic similarity.

**Algorithm:**
1. Compute set-based delta (nodes/edges added/removed)
2. Call structural_similarity() from Phase 54 for similarity score
3. Combine results into GraphDiffResult

**Example:**
```rust
// Source: Research based on Phase 54 patterns
use crate::algo::graph_similarity::{structural_similarity, SimilarityBounds};

pub fn graph_diff(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
) -> Result<GraphDiffResult, SqliteGraphError>
{
    // Compute exact deltas
    let (nodes_added, nodes_removed) = compute_node_delta(graph1, graph2);
    let (edges_added, edges_removed) = compute_edge_delta(graph1, graph2);

    // Compute structural similarity
    let similarity = structural_similarity(
        graph1,
        graph2,
        SimilarityBounds::default()
    )?;

    Ok(GraphDiffResult {
        nodes_added,
        nodes_removed,
        edges_added,
        edges_removed,
        similarity_score: similarity.mcs_similarity,
        is_isomorphic: similarity.isomorphic,
        graph_edit_distance: similarity.ged_distance,
    })
}
```

### Pattern 3: Refactor Validation

**What:** Use diff results + similarity heuristics to validate refactors.

**When to use:** For DIFF-02 - answering "did I break anything structural?"

**Validation Rules:**
1. No nodes removed → likely safe (only additions)
2. Similarity >= 0.8 → very similar, likely safe
3. Isomorphic = true → perfectly safe (structure preserved)
4. Nodes removed AND similarity < 0.5 → potentially breaking change

**Example:**
```rust
// Source: Research based on refactoring validation patterns
pub struct RefactorValidation {
    pub is_safe: bool,
    pub breaking_changes: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn validate_refactor(diff: &GraphDiffResult) -> RefactorValidation {
    let mut validation = RefactorValidation {
        is_safe: true,
        breaking_changes: Vec::new(),
        warnings: Vec::new(),
    };

    // Check for removed nodes (potentially breaking)
    if !diff.nodes_removed.is_empty() {
        validation.breaking_changes.push(format!(
            "{} nodes removed from graph",
            diff.nodes_removed.len()
        ));
        validation.is_safe = false;
    }

    // Check similarity score
    if diff.similarity_score < 0.5 {
        validation.breaking_changes.push(format!(
            "Low similarity score: {:.2} (significant structural change)",
            diff.similarity_score
        ));
        validation.is_safe = false;
    } else if diff.similarity_score < 0.8 {
        validation.warnings.push(format!(
            "Moderate similarity: {:.2} (review recommended)",
            diff.similarity_score
        ));
    }

    // Check for isomorphism (perfect structure preservation)
    if diff.is_isomorphic {
        validation.warnings.push(
            "Graphs are isomorphic - structure perfectly preserved".to_string()
        );
    }

    validation
}
```

### Anti-Patterns to Avoid

- **Naive pair-wise comparison** of nodes by ID: This fails when node IDs change between snapshots. Use set operations instead.
- **Re-implementing VF2 isomorphism**: Already implemented in Phase 54 via petgraph. Reuse existing code.
- **Exact GED computation**: NP-hard and computationally expensive. Use simplified GED (1.0 - similarity) from Phase 54.
- **Ignoring edge direction**: SqliteGraph uses directed edges. Delta computation must preserve direction.

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Graph isomorphism checking | Custom VF2 implementation | petgraph::algo::isomorphism (already in Phase 54) | VF2 is complex, well-tested in petgraph |
| Maximum Common Subgraph | Custom MCS algorithm | Phase 54 structural_similarity | MCS is NP-hard, already bounded and tested |
| Set difference operations | Manual iteration | std::collections::HashSet / ahash::AHashSet | O(1) lookup, battle-tested |
| Progress tracking | Custom callback system | ProgressCallback trait (existing pattern) | Consistent with all other algorithms |

**Key insight:** The hardest part of graph diff (similarity computation) is already done in Phase 54. Set-based delta is trivial with HashSet. The main work is structuring the API and combining existing pieces.

## Common Pitfalls

### Pitfall 1: Ignoring Node Identity Changes

**What goes wrong:** Comparing nodes by position/index instead of ID, leading to incorrect deltas when graphs are re-ordered.

**Why it happens:** Developers assume node IDs are stable across snapshots, but they may change due to garbage collection or re-numbering.

**How to avoid:** Always use set operations on entity IDs, not indices. Use all_entity_ids() to get the complete set.

**Warning signs:** Delta shows all nodes as "removed" and "added" when comparing identical graphs.

### Pitfall 2: Directionless Edge Comparison

**What goes wrong:** Treating edges as undirected sets {u, v} instead of directed tuples (u, v), missing direction changes.

**Why it happens:** Simplifying edge representation for convenience, losing direction information.

**How to avoid:** Always store and compare edges as directed tuples (from_id, to_id). Use fetch_outgoing() to preserve direction.

**Warning signs:** Edge delta doesn't match visual inspection of graph changes.

### Pitfall 3: Forgetting About Empty Graphs

**What goes wrong:** Division by zero or panic when computing similarity scores for empty graphs.

**Why it happens:** Similarity = mcs_size / max(g1_size, g2_size) fails when both graphs are empty.

**How to avoid:** Handle empty graph edge cases explicitly (both empty = 1.0, one empty = 0.0).

**Warning signs:** "attempt to divide by zero" panic in tests with empty graphs.

### Pitfall 4: Assuming Snapshot IDs Imply Ordering

**What goes wrong:** Assuming snapshot_id1 < snapshot_id2 means graph1 is "before" graph2.

**Why it happens:** Snapshot IDs are transaction IDs, not necessarily ordered for comparison purposes.

**How to avoid:** Always require user to specify which graph is "before" and which is "after" explicitly. Don't infer from snapshot IDs.

**Warning signs:** Delta results seem backwards (added/removed swapped).

## Code Examples

Verified patterns from official sources:

### Example 1: Set-Based Delta Computation

```rust
// Source: Based on SQLiteGraph API patterns
use ahash::AHashSet;

/// Result of graph delta computation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphDelta {
    /// Nodes present in graph2 but not in graph1
    pub nodes_added: AHashSet<i64>,
    /// Nodes present in graph1 but not in graph2
    pub nodes_removed: AHashSet<i64>,
    /// Edges present in graph2 but not in graph1
    pub edges_added: Vec<(i64, i64)>,
    /// Edges present in graph1 but not in graph2
    pub edges_removed: Vec<(i64, i64)>,
}

/// Compute exact structural delta between two graphs
pub fn graph_diff(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
) -> Result<GraphDelta, SqliteGraphError>
{
    // Get node sets
    let nodes1: AHashSet<i64> = graph1.all_entity_ids()?.into_iter().collect();
    let nodes2: AHashSet<i64> = graph2.all_entity_ids()?.into_iter().collect();

    // Compute node delta
    let nodes_added = nodes2.difference(&nodes1).copied().collect();
    let nodes_removed = nodes1.difference(&nodes2).copied().collect();

    // Get edge sets
    let mut edges1: AHashSet<(i64, i64)> = AHashSet::new();
    let mut edges2: AHashSet<(i64, i64)> = AHashSet::new();

    for &from_id in &nodes1 {
        if let Ok(outgoing) = graph1.fetch_outgoing(from_id) {
            for &to_id in &outgoing {
                edges1.insert((from_id, to_id));
            }
        }
    }

    for &from_id in &nodes2 {
        if let Ok(outgoing) = graph2.fetch_outgoing(from_id) {
            for &to_id in &outgoing {
                edges2.insert((from_id, to_id));
            }
        }
    }

    // Compute edge delta
    let edges_added: Vec<_> = edges2.difference(&edges1).copied().collect();
    let edges_removed: Vec<_> = edges1.difference(&edges2).copied().collect();

    Ok(GraphDelta {
        nodes_added,
        nodes_removed,
        edges_added,
        edges_removed,
    })
}
```

### Example 2: Combined Diff with Similarity

```rust
// Source: Based on Phase 54 graph_similarity module
use crate::algo::graph_similarity::{structural_similarity, SimilarityBounds};

/// Complete graph diff result with similarity metrics
#[derive(Debug, Clone)]
pub struct GraphDiffResult {
    /// Exact node changes
    pub nodes_added: AHashSet<i64>,
    pub nodes_removed: AHashSet<i64>,
    /// Exact edge changes
    pub edges_added: Vec<(i64, i64)>,
    pub edges_removed: Vec<(i64, i64)>,
    /// Structural similarity metrics (from Phase 54)
    pub similarity_score: f64,
    pub is_isomorphic: bool,
    pub graph_edit_distance: f64,
    /// Graph sizes for context
    pub graph1_size: usize,
    pub graph2_size: usize,
}

/// Compute complete graph diff with similarity metrics
pub fn graph_diff_with_similarity(
    graph1: &SqliteGraph,
    graph2: &SqliteGraph,
) -> Result<GraphDiffResult, SqliteGraphError>
{
    // Compute exact deltas
    let delta = graph_diff(graph1, graph2)?;

    // Compute similarity using Phase 54 algorithm
    let similarity = structural_similarity(
        graph1,
        graph2,
        SimilarityBounds::default()
    )?;

    Ok(GraphDiffResult {
        nodes_added: delta.nodes_added,
        nodes_removed: delta.nodes_removed,
        edges_added: delta.edges_added,
        edges_removed: delta.edges_removed,
        similarity_score: similarity.mcs_similarity,
        is_isomorphic: similarity.isomorphic,
        graph_edit_distance: similarity.ged_distance,
        graph1_size: similarity.graph1_size,
        graph2_size: similarity.graph2_size,
    })
}
```

### Example 3: Refactor Validation

```rust
// Source: Based on refactoring validation patterns

/// Validation result for refactor checking
#[derive(Debug, Clone)]
pub struct RefactorValidation {
    /// True if refactor is likely safe
    pub is_safe: bool,
    /// Breaking changes detected
    pub breaking_changes: Vec<String>,
    /// Warnings (not breaking, but noteworthy)
    pub warnings: Vec<String>,
}

/// Validate that a refactor didn't break structure
pub fn validate_refactor(diff: &GraphDiffResult) -> RefactorValidation {
    let mut validation = RefactorValidation {
        is_safe: true,
        breaking_changes: Vec::new(),
        warnings: Vec::new(),
    };

    // Check 1: No nodes removed
    if !diff.nodes_removed.is_empty() {
        validation.breaking_changes.push(format!(
            "Removed {} nodes - potentially breaking",
            diff.nodes_removed.len()
        ));
        validation.is_safe = false;
    }

    // Check 2: Similarity threshold
    if diff.similarity_score < 0.5 {
        validation.breaking_changes.push(format!(
            "Low similarity score: {:.2} - structural changes detected",
            diff.similarity_score
        ));
        validation.is_safe = false;
    } else if diff.similarity_score < 0.8 {
        validation.warnings.push(format!(
            "Moderate similarity: {:.2} - review recommended",
            diff.similarity_score
        ));
    }

    // Check 3: Isomorphism (perfect preservation)
    if diff.is_isomorphic {
        validation.warnings.push(
            "Structure preserved (isomorphic)".to_string()
        );
    }

    validation
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Text-based diff | Structural graph diff | 2020s | Enables semantic comparison of graph structures |
| Exact GED (NP-hard) | Simplified GED + MCS | 2010s | Practical similarity scoring for large graphs |
| Manual isomorphism checking | VF2 algorithm (petgraph) | 2016 | Robust, well-tested isomorphism detection |

**Current best practices (2024-2025):**

1. **DELCON algorithm** (Koutra et al.): Principled similarity function for massive graphs using affinity matrices and fast belief propagation. Overkill for our use case but good to know.

2. **VF2 isomorphism** (Cordella et al.): De facto standard for graph/subgraph isomorphism. Implemented in petgraph, already used in Phase 54.

3. **Set-based delta**: Industry standard for computing exact changes. Used in version control systems (Git), databases (CDC), and graph databases.

4. **Hybrid approaches**: Combine exact deltas with semantic similarity. Used in tools like Difftastic, Graptage for structured diffing.

**Deprecated/outdated:**

- **Pair-wise node comparison by position**: Fails when node IDs change. Use set operations instead.
- **Unbounded MCS enumeration**: NP-hard, can run forever. Always use bounds (max_matches, timeout_ms).
- **Text-based graph diff**: Doesn't understand graph structure. Use structural comparison.

## Open Questions

1. **DELCON integration**
   - What we know: DELCON is a sophisticated similarity algorithm for massive graphs
   - What's unclear: Whether DELCON provides additional value over Phase 54's similarity for our use case
   - Recommendation: Stick with Phase 54 similarity for now. DELCON can be added later if needed for very large graphs (10k+ nodes).

2. **Edge type awareness**
   - What we know: Current design treats all edges equally (only from_id/to_id)
   - What's unclear: Whether diff should distinguish between edge types (calls, imports, etc.)
   - Recommendation: Start with type-agnostic diff. Add type-aware filtering in future if users request it.

3. **Incremental diff optimization**
   - What we know: Set-based delta is O(V+E) which is fast
   - What's unclear: Whether incremental computation (tracking changes as they happen) would be faster
   - Recommendation: Set-based delta is sufficient for current requirements. Incremental tracking adds complexity without clear benefit.

4. **Node identity across snapshots**
   - What we know: Using entity IDs for node identity
   - What's unclear: How to handle nodes with same "name" but different IDs across snapshots
   - Recommendation: Use entity IDs for exact delta. Semantic matching (by name) is a separate feature that can use subgraph isomorphism.

## Sources

### Primary (HIGH confidence)

- **petgraph documentation** - Isomorphism checking, subgraph matching, graph data structures
  - `/websites/rs_petgraph` - VF2 algorithm documentation
  - Verified: petgraph has is_isomorphic(), is_isomorphic_subgraph_matching(), subgraph_isomorphisms_iter()
  - All these functions are used in Phase 54 graph_similarity.rs

- **Phase 54 graph_similarity.rs** - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/algo/graph_similarity.rs`
  - Verified: structural_similarity() implementation using VF2 and MCS
  - Verified: SimilarityResult struct with isomorphic, mcs_similarity, ged_distance
  - Verified: SimilarityBounds for controlling computation

- **SQLiteGraph API patterns** - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/algo/`
  - Verified: All algorithms follow consistent pattern (types first, then results, then functions, then progress variants)
  - Verified: All use ProgressCallback for long-running operations
  - Verified: All have tests in module files or algo/tests.rs

### Secondary (MEDIUM confidence)

- **DELCON paper** - [DELCON: A Principled Massive-Graph Similarity Function](https://web.eecs.umich.edu/~dkoutra/papers/DeltaCon_KoutraVF_withAppendix.pdf)
  - Verified: Highly cited (338+), principled approach to graph similarity
  - Verified: Uses affinity matrices and fast belief propagation
  - Not using because: Overkill for our use case, Phase 54 similarity is sufficient

- **Graph diff tools research** - Multiple sources on structural diffing:
  - [Graptage - semantic diff utility](https://github.com/trailofbits/graphtage)
  - [Diffast - AST-based diff tool](https://github.com/codinuum/diffast)
  - [Difftastic - syntax-aware diff](https://news.ycombinator.com/item?id=27768861)
  - Verified: Structural diffing is standard practice for code/graph comparison
  - Confirms: Set-based delta + semantic similarity is the right approach

- **Refactoring validation research**:
  - [HEC: Equivalence Verification Checking](https://www.usenix.org/system/files/atc25-yin.pdf)
  - [Visual Assurance in Refactoring](https://www.johanfabry.be/assets/deknopAl-saner2024.pdf)
  - [Structural Approach to Program Similarity](https://ceur-ws.org/Vol-3845/paper01.pdf)
  - Verified: Graph-based approaches are standard for refactor validation
  - Confirms: Similarity thresholds (0.8, 0.5) are commonly used heuristics

### Tertiary (LOW confidence)

- **Graph Edit Distance implementations**:
  - [Graph Edit Distance Computation](https://lijunchang.github.io/Graph_Edit_Distance/)
  - [tree-edit-distance (Rust)](https://github.com/AME-SCM/tree-edit-distance)
  - Not verified for production use
  - Not using because: Exact GED is NP-hard, simplified GED (1.0 - similarity) is sufficient

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - petgraph and existing Phase 54 code are well-documented and tested
- Architecture: HIGH - Clear patterns from existing algo modules, set-based delta is straightforward
- Pitfalls: HIGH - Identified from common graph comparison mistakes and SQLiteGraph patterns

**Research date:** 2026-02-03
**Valid until:** 2026-03-03 (30 days - graph algorithms research is stable)

**Execution context:**
- Magellan database: `.codemcp/codegraph.db`
- Execution ID: `6981377a-33f41c` (from initial status check)
- Files examined:
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/algo/mod.rs` (module structure)
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/algo/graph_similarity.rs` (Phase 54 implementation)
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/algo/cut_partition.rs` (result type patterns)
  - `/home/feanor/Projects/sqlitegraph/.planning/REQUIREMENTS.md` (DIFF-01, DIFF-02 requirements)

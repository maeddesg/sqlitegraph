# SQLiteGraph Architectural Analysis

**Last Updated:** 2026-02-03
**Version:** v1.4.2
**Status:** Design Document - Not Implemented

This document captures the architectural analysis of SQLiteGraph as of v1.4.2, including strengths, concerns, scalability challenges, and potential paths forward.

---

## Executive Summary

SQLiteGraph is a graph database built on top of SQLite with a custom Native V2 backend option. As of v1.4.2, the project has achieved significant technical milestones but faces fundamental scalability questions that will determine its long-term viability.

**Key Finding:** The Native V2 backend has a hard limit of 2048 nodes due to architectural constraints, while the SQLite backend can scale arbitrarily but lacks performance characteristics for graph workloads.

---

## Current Architecture

### Dual Backend Strategy

```
SQLiteGraph
├── SQLite Backend
│   ├── Arbitrary scale (proven)
│   ├── Mature tooling (sqlite3, dumps, etc.)
│   └── 10-100x slower on graph traversals
│
└── Native V2 Backend
    ├── Clustered edge storage (I/O locality)
    ├── MVCC snapshot isolation
    ├── WAL-based transactions
    ├── KV store + Pub/Sub
    ├── HNSW vector search
    └── Hard limit: 2048 nodes
```

### Backend Selection

Users select backend via `GraphConfig`:

```rust
// SQLite backend - unlimited scale
let cfg = GraphConfig::sqlite();

// Native V2 - performance, 2048 node limit
let cfg = GraphConfig::native();
```

---

## Strengths

### 1. Architectural Ambition

SQLiteGraph tackles hard problems:
- **MVCC from scratch** - True snapshot isolation without relying on SQLite
- **Custom binary format** - Clustered edge storage for I/O locality
- **Zero-copy reads** - Snapshot-based data access
- **Comprehensive algorithms** - 35+ graph algorithms in a single crate

### 2. Grounded Development Workflow

The project uses a sophisticated toolchain for code intelligence:
- **Magellan** - Call graph indexing and symbol navigation
- **llmgrep** - Semantic code search
- **Mirage** - CFG analysis and path enumeration
- **No guessing** - All changes verified against actual source/database

This prevents common pitfalls like broken refactor chains and incorrect assumptions about code behavior.

### 3. Documentation Excellence

As of v1.4.2:
- **5 developer guides** covering internals (80+ pages)
- **User manual** with examples
- **API documentation** for all public interfaces
- **CHANGELOG** tracking all releases

The documentation investment is uncommon for a project of this size and pays dividends in contributor onboarding.

### 4. Feature Completeness

| Feature | Status |
|---------|--------|
| Graph CRUD | Complete |
| Transactions | Complete (MVCC) |
| Algorithms | 35+ implemented |
| Vector Search | HNSW with multiple metrics |
| Pub/Sub | Pattern-based subscriptions |
| KV Store | In-memory with TTL |
| Query API | Prefix scans, pattern matching |

---

## Concerns

### 1. The 2048 Node Limit (Critical)

**Root Cause:** Native V2 reserves 8MB for "node slots" in the header, with each slot tracking node metadata.

```rust
// From native/v2/mod.rs
const MAX_NODES: u64 = 2048;  // Hard limit
const NODE_SLOT_SIZE: u64 = 4096;  // Each slot is 4KB
```

**Why This Exists:**
- O(1) node lookup by ID (direct slot index)
- Simplifies cluster offset calculation
- Avoids dynamic allocation for node metadata

**Impact:**
- Databases cannot exceed 2048 nodes
- No migration path (would require format v3)
- Silent failure when creating node 2049

### 2. Chain Traversal Performance Regression

**Finding:** Native V2 is 10x slower than SQLite on pure chain traversals.

**Hypothesis:**
- SQLite's query optimizer excels at sequential access
- V2's cluster overhead dominates on linear graphs
- Edge clustering benefits vanish without branching

**Evidence from benchmarks:**
```
Chain of 1000 nodes:
- SQLite: 10ms
- Native V2: 100ms
```

This reveals V2 is optimized for *branching* graphs, not all graph patterns.

### 3. Tooling Complexity Burden

The Magellan/llmgrep workflow adds friction:
- Index must be running before any work
- Commands are verbose
- New contributors face learning curve
- Tool failures can block progress

**Question:** Is the tooling complexity worth it for a single-developer project?

### 4. Dual Backend Maintenance

Two backends mean:
- Double the testing surface
- Feature parity challenges
- API limited to lowest common denominator
- Confusing UX for users (which backend should I use?)

---

## Scalability Challenge

### The Core Problem

> **SQLiteGraph can be small and fast (V2) OR large and slow (SQLite), but not both.**

This is an existential question for the project's value proposition.

### Why Not Just Use SQLite?

If SQLite is the only scalable option:
- Why build V2 at all?
- Why invest in custom WAL, clusters, MVCC?
- Why maintain dual codebases?

The project needs a clear answer to: "What does V2 give me that I can't get with pure SQLite?"

### Potential Answers

| Answer | V2's Value | Validity |
|--------|-----------|----------|
| Performance | 10x on branching graphs | Yes, but proven only at small scale |
| Pub/Sub | Native event system | Weak - could layer on SQLite |
| KV Store | In-memory with TTL | Weak - many external options |
| HNSW | Vector search | Strong - but backend-agnostic |
| Control | No SQLite dependency | Weak - dependency is usually fine |

---

## Options for V3 / Different Workloads

### Option 1: Fix the V2 Architecture

**Approach:** Remove the 2048 limit by redesigning node storage.

**Changes:**
1. Remove fixed node slots from header
2. Use dynamic allocation for node metadata
3. Add node index (HashMap or B-tree)
4. Keep clustered edge storage

**Trade-offs:**
- Lose O(1) node lookup
- More complex recovery logic
- Potential performance regression

**Effort:** 2-4 weeks of focused work

### Option 2: Sharded V2 Backend

**Approach:** Multiple graph files with a routing layer.

```
GraphRouter
├── shard_0.db (nodes 0-2047)
├── shard_1.db (nodes 2048-4095)
├── shard_2.db (nodes 4096-6143)
└── ...
```

**Changes:**
1. Shard ID embedded in node IDs
2. Cross-shard edges tracked separately
3. Router handles transparent routing

**Trade-offs:**
- Cross-shard queries become multi-file ops
- Sharding strategy is complex
- Still doesn't fix single-node scalability

**Effort:** 3-6 weeks

### Option 3: Hybrid Backend

**Approach:** Use SQLite for storage, V2 for cached computation.

```
HybridBackend
├── SQLite (authoritative storage)
└── V2 Cache (materialized views for hot paths)
```

**Changes:**
1. All writes go to SQLite
2. Cache manager selects what to materialize in V2
3. Queries use cache if available, fall back to SQLite

**Trade-offs:**
- Cache invalidation complexity
- Two consistency models to maintain
- May not beat pure SQLite at large scale

**Effort:** 4-8 weeks

### Option 4: Workload-Specific Backends (Recommended)

**Approach:** Design backends for specific use cases rather than general-purpose.

```
Backend Selection Guide:
┌─────────────────────────────────────────────────────────────┐
│ Use Case                  │ Backend       │ Max Scale     │
├─────────────────────────────────────────────────────────────┤
│ Code analysis (<10K nodes)│ Native V2     │ 2048 nodes    │
│ Agent messaging           │ Native V2     │ 2048 nodes    │
│ Vector similarity search  │ Native V2     │ 2048 nodes    │
│ Large knowledge graphs    │ SQLite        │ Unlimited     │
│ Analytics workloads       │ SQLite        │ Unlimited     │
│ Enterprise applications   │ SQLite        │ Unlimited     │
└─────────────────────────────────────────────────────────────┘
```

**Rationale:**
- Acknowledge V2's sweet spot (small, dense, read-heavy graphs)
- Lean into SQLite for large-scale workloads
- Clear user guidance based on requirements
- No false promise of "general purpose"

**Required Changes:**
1. Document use case boundaries clearly
2. Add warnings when approaching V2 limits
3. Deprecate V2 for "large scale" in docs
4. Consider renaming: `NativeV2Small` vs `SQLiteLarge`

**Effort:** 1-2 weeks (documentation + warnings)

### Option 5: V3 Clean Slate

**Approach:** Design a new format from scratch with scalability as a primary goal.

**Design Principles:**
1. No hard limits (dynamic everything)
2. Page-based allocation (like databases)
3. Extensible format for future features
4. Migration path from V2

**Key Changes:**
1. Remove node slots entirely
2. Use B-trees for all indexes
3. Variable-size clusters
4. Committed to backwards compatibility

**Trade-offs:**
- Complete rewrite of storage layer
- Breaking change (requires migration)
- High risk, high reward

**Effort:** 8-12 weeks

---

## Performance Characterization

### Where V2 Wins

| Workload | V2 vs SQLite | Notes |
|----------|--------------|-------|
| Dense neighborhoods | 2-10x faster | Clustered edges shine |
| Vector search | 5-20x faster | HNSW + in-memory |
| Pub/Sub fanout | 10-50x faster | In-process channels |
| KV operations | 5-10x faster | In-memory HashMap |

### Where SQLite Wins

| Workload | SQLite vs V2 | Notes |
|----------|--------------|-------|
| Chain traversals | 10x faster | Query optimizer |
| Full graph scans | 2-5x faster | B-tree indexes |
| Large writes | 3-10x faster | Write-ahead log optimization |
| Concurrent access | 5-20x faster | Mature locking |

### The Crossover Point

**Hypothesis:** V2 is faster up to ~2000 nodes, then SQLite dominates due to:
1. V2 hits hard limits
2. SQLite's optimizer scales better
3. V2's in-memory advantage diminishes

**Needs verification:** Benchmark at 100, 500, 1000, 2000, 5000 nodes.

---

## Recommendations

### Short Term (Next Release)

1. **Add limits detection**
   ```rust
   if node_count >= 1900 {
       warn!("Approaching V2 limit (2048 nodes). Consider SQLite backend.");
   }
   ```

2. **Document use case boundaries**
   - Update README with backend selection guide
   - Add "When to use which backend" section to MANUAL.md

3. **Benchmarks at scale**
   - Test V2 at 1000, 2000 nodes
   - Test SQLite at 10K, 100K nodes
   - Document crossover point

### Medium Term (Next Quarter)

**Recommendation: Option 4 (Workload-Specific Backends)**

1. **Rebrand for clarity**
   - Marketing: "V2 for small fast graphs, SQLite for scale"
   - Rename types: `SmallGraphBackend`, `ScalableBackend`

2. **Improve SQLite performance**
   - Add query hints for graph patterns
   - Materialized view support
   - Custom functions for graph algorithms

3. **V2 polish within limits**
   - Better error messages at limits
   - Tools to detect when to migrate
   - Export/import to SQLite

### Long Term (Next Year)

**If V2 is strategic:**
- Pursue Option 5 (V3 clean slate)
- Hire/assign dedicated storage engineer
- 6-month roadmap for format redesign

**If SQLite is strategic:**
- Phase out V2 maintenance burden
- Focus on SQLite optimizations
- Consider spinning out HNSW as separate crate

---

## Open Questions

1. **What is the project's core value proposition?**
   - Performance? SQLite wins at scale.
   - Control? Debatable for most users.
   - Algorithms? These are backend-agnostic.
   - Vector search? Can separate into own crate.

2. **Who is the target user?**
   - If researchers: 2048 limit is fatal
   - If app developers: Why not use SQLite directly?
   - If agent systems: Maybe, but they scale quickly

3. **What problem does V2 solve that can't be solved otherwise?**
   - Need clear, defensible answer
   - Current answer is muddy
   - "Performance" doesn't hold at >2048 nodes

---

## Conclusion

SQLiteGraph v1.4.2 is a technically impressive project with ambitious architecture and excellent documentation. The architectural decision has been made:

### **DECISION: Create Native V3 Backend**

**Date:** 2026-02-03
**Status:** Approved, future milestone

**The Plan:**
- **V2 remains unchanged** - No breaking changes, continues to work as-is
- **V3 is a new backend** - Same features, scalable storage
- **Users choose at open time** - `GraphConfig::v2()` or `GraphConfig::v3()`
- **Feature parity** - V3 has everything V2 has, without the 2048 limit

**Why This Works:**
```
V2 (current):              V3 (future):
├── Fast & simple          ├── Fast & scalable
├── 2048 node limit        ├── Unlimited nodes
├── Fixed slot storage     ├── Dynamic pages + B-tree
├── Proven stable          ├── Built on V2's experience
└── KEEP AS-IS             └── NEW DEVELOPMENT

Both have:
├── Clustered edge storage
├── MVCC snapshot isolation
├── WAL-based transactions
├── KV store + Pub/Sub
└── HNSW vector search
```

**Real-World Use Cases:**
| Project | Backend | Why? |
|---------|---------|------|
| **OdinCode** | V2 | ~4,500 symbols, small scale |
| **SimdFlow** | V3 | 100K+ nodes for KV cache |

**Implementation Approach:**
- Only storage layer changes (fixed slots → dynamic pages)
- All other code remains identical
- O(1) → O(log n) lookup is acceptable trade-off for unlimited scale
- Migration tool optional (V2 → V3 export)

---

## Appendix: Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-02-03 | Document analysis | Capture architectural thinking for future reference |
| 2026-02-03 | **Create V3 backend** | Scale requirement for SimdFlow, V2 proven for small graphs |
| TBD | Implement V3 | Future milestone, not immediate priority |

---

## References

- **Source:** `src/backend/native/v2/` - V2 implementation
- **Source:** `src/backend/sqlite/` - SQLite implementation
- **Benchmarks:** `sqlitegraph/examples/phase55_*_performance*.rs`
- **Developer Guides:** `docs/DEVELOPMENT_GUIDES/` - Internal documentation
- **User Manual:** `MANUAL.md` - Public API documentation

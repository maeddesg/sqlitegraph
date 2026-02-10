# Design Philosophy

**Last Updated:** 2026-02-10
**Version:** v1.5.7

The core principles that guide SQLiteGraph's development.

---

## 1. Embedded First

**Principle:** SQLiteGraph is a library, not a server.

**What this means:**
- No separate server process to manage
- No network protocol
- No authentication/authorization concerns
- Deploy as a single binary with your application

**Why:**
- Simplicity: Less operational overhead
- Performance: No network round-trips
- Reliability: Fewer moving parts
- Security: No attack surface from network exposure

**Trade-off:**
- Single-machine only (no distributed queries)
- Single writer at a time (enforced by design)

---

## 2. Correct Over Clever

**Principle:** Simple, correct code beats clever but bug-prone optimizations.

**Examples:**

### Clustered Storage (Native V2)

We *could* make clustered storage faster with:
- Complex rebalancing algorithms
- Prediction-based cluster allocation
- Compression for edge records

**Instead:**
- Simple 64KB fixed-size clusters
- Sequential allocation
- No compression (keeps code simple)

**Why:** Complexity introduces bugs. The 1.6-10x performance win from clustering alone is enough. Additional complexity would have diminishing returns.

### MVCC-Lite

Full MVCC includes:
- Lock management
- Conflict detection
- Validation phase
- Retry logic

**SQLiteGraph uses:**
- Snapshot-based reads
- No write-write conflict detection
- SQLite handles transactions internally

**Why:** Full MVCC is complex. For embedded use cases, simplified snapshot isolation is sufficient.

---

## 3. Batteries Included

**Principle:** Don't make users build basic tools themselves.

**What's included:**

| Feature | Why It's Included |
|---------|-------------------|
| 35 graph algorithms | Users shouldn't implement PageRank from scratch |
| HNSW vector search | Vector similarity is a common need |
| Pub/Sub events | Real-time monitoring is table stakes |
| CLI tool | Debugging and ad-hoc queries |
| JSON export/import | Data portability |
| Introspection API | Debugging without external tools |

**Why:**
- Reduces time-to-value
- Ensures algorithm quality (tested, benchmarked)
- Provides a complete solution, not just a storage engine

**Trade-off:**
- Larger API surface
- More maintenance burden
- Acceptable for a library targeting developers

---

## 4. Dual Backend Strategy

**Principle:** Choice matters. Different workloads have different optimal solutions.

**Why two backends:**

| SQLite Backend | Native V2 Backend |
|----------------|-------------------|
| Proven reliability | Performance for star patterns |
| Ecosystem tooling | Smaller file sizes |
| Unlimited scale | Pub/Sub events |
| SQL query language | Clustered edge storage |

**Unified API:**
```rust
// Same code works with both backends
use sqlitegraph::{GraphConfig, open_graph};

let cfg = GraphConfig::sqlite();  // or GraphConfig::native()
let graph = open_graph("mygraph.db", &cfg)?;

// All operations identical
let node_id = graph.insert_node(spec)?;
let neighbors = graph.neighbors(NeighborQuery::outgoing(node_id))?;
```

**Why not just pick one?**
- No single solution is optimal for all workloads
- Users understand their needs better than we do
- Migration path lets users start safe (SQLite) and optimize later (Native V2)

---

## 5. LLM-Friendly Design

**Principle:** The library should work well with AI-assisted development.

**What this means:**

### Structured JSON Output

```rust
// Every algorithm returns structured data
let scores = algo::pagerank(&graph, 0.85, 50)?;
// scores: HashMap<u64, f64> → serializable to JSON

// Introspection API returns structured data
let stats = graph.introspection()?;
println!("{}", serde_json::to_string_pretty(&stats)?);
```

**Why:** LLMs can parse JSON output more reliably than free-form text.

### Span-Based Operations (in Splice)

The companion tool `splice` uses byte spans for refactoring:
- Extract spans from tree-sitter
- Validate at byte boundaries
- Apply replacements atomically

**Why:** LLMs can reason about precise locations without understanding entire codebases.

### CLI as Stable Interface

Each tool has a CLI with:
- Stable flags and arguments
- Versioned JSON output schema
- Exit codes for error handling

**Why:** LLMs can "call tools" by generating shell commands, which is more reliable than generating library code.

**Example LLM Workflow:**

```bash
# LLM generates these commands
magellan watch --root ./src --db .codemcp/codegraph.db
llmgrep search --db .codemcp/codegraph.db --query "process" --output json
splice rename --symbol abc123 --file src/lib.rs --to new_process_name
```

Each command:
- Has stable, documented interface
- Returns parseable JSON
- Can be composed into workflows

---

## 6. Pragmatic Constraints

**Principle:** Acknowledge limitations rather than over-engineering.

### Known Limitations

| Limitation | Why It Exists | Plan |
|------------|---------------|------|
| ~2,048 nodes (Native V2) | 8MB node region keeps format simple | V3 backend with dynamic allocation |
| Single writer | Simplifies concurrency | Future: multi-writer with conflict resolution |
| No distributed queries | Embedded-only design | Not planned |
| Chain traversal regression (V2) | Cluster lookup overhead | Not planned; use SQLite for chains |

**Why document limitations?**
- Users can make informed decisions
- No surprises
- Honest about use cases

### Semantic Versioning

**Principle:** Breaking changes are OK with major version bumps.

**What's a breaking change:**
- API signature changes
- Removed features
- Behavior changes that affect correctness

**What's NOT a breaking change:**
- Performance improvements
- Bug fixes
- New features
- Documentation updates

**Why:** SemVer lets users trust that minor version upgrades are safe.

---

## 7. Testing Over Performance (Initially)

**Principle:** Correctness first, optimization later.

**Workflow:**
1. Write the feature
2. Write comprehensive tests
3. Benchmark to establish baseline
4. Optimize if needed
5. Ensure tests still pass

**Example: HNSW Vector Search**

```rust
// Initial implementation: correct but slow
fn hnsw_search_naive(query: &[f32]) -> Vec<VecId>> {
    // Linear scan through all vectors
    all_vectors.iter()
        .map(|v| (v.id, distance(query, &v.vector)))
        .sorted_by_key(|(_, d)| *d)
        .take(10)
        .map(|(id, _)| id)
        .collect()
}

// Optimized implementation: fast and correct
fn hnsw_search_indexed(query: &[f32]) -> Vec<VecId> {
    // Use HNSW index for ANN search
    index.search(query, 10)
}
```

**Why:**
- Naive version is easy to verify
- Optimized version can be compared against naive
- Tests ensure both return same results (within tolerance)

---

## 8. Developer Experience Matters

**Principle:** The library should be pleasant to use.

**What this means:**

### Clear Error Messages

```rust
// Bad
panic!("Node not found");

// Good
return Err(SqliteGraphError::NotFoundError {
    id: node_id,
    message: format!("Node {} not found in graph", node_id),
    hint: "Check if the node was deleted or never inserted",
});
```

### Sensible Defaults

```rust
// Don't force users to make choices for common cases

// Bad
let graph = SqliteGraph::open("mygraph.db")
    .with_cache_size(256)?
    .with_auto_checkpoint(true)?
    .with_wal_sync(true)?
    .with_compression(CompressionLevel::Medium)?;

// Good
let graph = SqliteGraph::open("mygraph.db")?;  // Sensible defaults

// Optional configuration for power users
let cfg = GraphConfig::native()
    .with_cache_size(1000)?;  // Only if needed
```

### Documentation

Every public API has:
- Doc comments
- Examples
- Error conditions documented
- Panics documented (if any)

**Why:** Developers shouldn't have to read source code to use the library.

---

## 9. No Hidden Magic

**Principle:** The library should behave predictably.

**Examples:**

### Explicit Transactions

```rust
// Bad: Implicit transaction management
graph.insert_node(spec1)?;
graph.insert_node(spec2)?;
// When is data committed? Unclear.

// Good: Explicit transaction
let mut txn = graph.begin_txn()?;
txn.insert_node(spec1)?;
txn.insert_node(spec2)?;
txn.commit()?;  // Clear when data is written
```

### Explicit Feature Flags

```toml
# Bad: Features enabled by default
sqlitegraph = "1.5.7"  # What features are enabled?

# Good: Explicit opt-in
sqlitegraph = { version = "1.5.7", features = ["native-v2"] }
sqlitegraph = "1.5.7"  # SQLite only, no features
```

### No Global State

```rust
// Bad: Global graph instance
lazy_static! {
    static ref GLOBAL_GRAPH: SqliteGraph = SqliteGraph::open("graph.db").unwrap();
}

// Good: Explicit ownership
fn process_graph(graph: &SqliteGraph) -> Result<()> {
    // ...
}
```

**Why:**
- Easier to reason about
- Easier to test
- Fewer surprises
- Thread-safe by default

---

## 10. Community Over Code

**Principle:** The ecosystem matters more than any single tool.

**Integration with other tools:**

```
┌─────────────────────────────────────────────────┐
│                  The Ecosystem                   │
│                                                  │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
│  │Magellan │─→│codegraph│←─│ splice  │         │
│  └─────────┘  │  .db   │  └─────────┘         │
│              └────┬────┘                       │
│  ┌─────────┐      │                           │
│  │ llmgrep │─────┘                           │
│  └─────────┘                                  │
│                   ↓                             │
│  ┌───────────────────────────────────────────┐ │
│  │         OdinCode (LLM Editor)              │ │
│  │      Coordinates tool usage               │ │
│  └───────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

**Why:**
- Each tool does one thing well
- Shared database format enables composition
- LLM can orchestrate tools via CLI
- Users can adopt tools incrementally

---

## Counter-Examples: What We Don't Do

### We Don't: Hide Complexity Behind Magic

```rust
// Bad: "Smart" API that does too much
graph.smart_insert("User", "Alice", json!({"age": 30}))?;
// What does this do? How do I control behavior?

// Good: Explicit API
let spec = NodeSpec {
    kind: "User".to_string(),
    name: "Alice".to_string(),
    file_path: None,
    data: json!({"age": 30}),
};
let user_id = graph.insert_node(spec)?;
```

### We Don't: Prioritize Benchmarks Over Correctness

**Example:** A faster BFS that misses some nodes is worse than a slower BFS that's correct.

```rust
// Bad: Fast but incorrect
fn bfs_fast(graph: &Graph, start: u64) -> HashSet<u64> {
    let mut visited = HashSet::new();
    // Fast lookup table, but misses some paths
    // ...
}

// Good: Correct with reasonable performance
fn bfs_correct(graph: &Graph, start: u64) -> HashSet<u64> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    // Correct BFS implementation
    // ...
}
```

### We Don't: Break Compatibility Without Major Version Bump

```rust
// v1.x: This API exists
pub fn get_node(&self, id: u64) -> Result<NodeData, Error>;

// v2.0: Can change behavior
pub fn get_node(&self, id: u64) -> Result<NodeData, Error>;

// v1.x: This is NOT acceptable
pub fn get_node(&self, id: u64) -> Result<NodeData, Error>;  // Breaking change!
```

---

## Summary

SQLiteGraph's design philosophy:

1. **Embedded First** — Library, not server
2. **Correct Over Clever** — Simple beats bug-prone
3. **Batteries Included** — Complete solution
4. **Dual Backend** — Choice matters
5. **LLM-Friendly** — Structured I/O, stable CLI
6. **Pragmatic Constraints** — Honest limitations
7. **Test First, Optimize Later** — Correctness before speed
8. **Developer Experience** — Pleasant to use
9. **No Hidden Magic** — Predictable behavior
10. **Community Over Code** — Ecosystem focus

These principles guide every decision in SQLiteGraph's development. When in doubt, we refer back to these principles rather than following trends or hype.

---

## Further Reading

- [ARCHITECTURE.md](ARCHITECTURE.md) - System design and data flow
- [MIGRATION.md](MIGRATION.md) - Backend migration guide
- [TESTING.md](TESTING.md) - Testing philosophy and patterns
- [README.md](../README.md) - Feature overview and benchmarks

# SQLiteGraph

[![crates.io](https://img.shields.io/crates/v/sqlitegraph.svg)](https://crates.io/crates/sqlitegraph)
[![Documentation](https://docs.rs/sqlitegraph/badge.svg)](https://docs.rs/sqlitegraph)

Embedded graph database with dual backend architecture, graph algorithms,
Cypher-inspired queries, and HNSW vector search.

**Positioning:** Single-binary embedded database (no server). Persistent storage with
atomic batch commits. Graph algorithms + HNSW vector search in one engine.
SQLite: stable, mature, and easy to inspect with standard tooling. Native V3:
graph-oriented storage with cache, KV, pub/sub, and traversal features. See the
benchmarks below for workload-specific behavior.

## What's New in v3.0.0

`v3.0.0` is the first release that ships the full Cypher engine, the
expanded CLI surface, and the cross-session HNSW persistence fix:

- Cypher-inspired `MATCH`, `CREATE`, `SET`, `DELETE`, multi-hop, variable-depth,
  star/multi-pattern joins, `WHERE` with precedence/parentheses, and HNSW
  vector search via `CALL db.index.vector.queryNodes(...)`.
- CLI coverage for PageRank, betweenness, WCC/SCC, Louvain, label propagation,
  cycle search, dominators, topological sort, critical path, and HNSW
  create/insert/search/list/delete.
- Python `Graph.query()` plus algorithm bindings for SCC, label propagation,
  cycle search, dominators, critical path, and HNSW index deletion.
- V3 durability fixes and HNSW autoload persistence fixes.

See [CHANGELOG](https://github.com/oldnordic/sqlitegraph/blob/main/CHANGELOG.md) for full history.

## Backends

| Feature | SQLite | Native V3 |
|---------|--------|-----------|
| Status | Stable | Stable |
| Storage | `.db` file | `.graph` file |
| Capacity model | Storage-limited | Storage-limited |
| Graph algorithms | 35+ | 35+ |
| HNSW vectors | Yes | Yes |
| Pub/Sub | Yes | Yes |
| LRU Cache | No | Yes (v2.1.0) |
| Parallel BFS | No | Yes (v2.1.1) |

## Benchmarks

See [Architecture](https://github.com/oldnordic/sqlitegraph/blob/main/docs/ARCHITECTURE.md) for system design details.

**Backend Comparison (v2.2.4 - AMD Ryzen 7 7800X3D, tmpfs, Rust 1.95.0):**

| Benchmark | SQLite | V3 | Ratio |
|-----------|--------|----|-------|
| BFS 1K nodes / 5K edges | 2.5ms | 0.45ms | V3 **5.5x faster** |
| BFS 10K / 50K | 26ms | 27ms | ~parity |
| BFS 50K / 250K | 160ms | 586ms | SQLite 3.6x faster |
| DFS 1K / 5K | 2.4ms | 0.46ms | V3 **5.2x faster** |
| Point lookup 1K | 15us | 82us | SQLite 5.4x faster |
| Point lookup 10K | 27us | 503us | SQLite 18x faster |
| Shortest path 1K | 304us | 393us | SQLite 29% faster |

V3 excels at small-scale traversals. SQLite dominates at larger graph sizes and
point lookups.

**Run benchmarks yourself:**

```bash
git clone https://github.com/oldnordic/sqlitegraph.git
cd sqlitegraph/sqlitegraph-core
cargo run --example test_performance_comparison --features native-v3
cargo bench --features native-v3 -- backend_comparison
```

See [examples/](https://github.com/oldnordic/sqlitegraph/tree/main/sqlitegraph-core/examples) for reproducible performance tests.

## Quick Start

```toml
[dependencies]
# SQLite backend (default)
sqlitegraph-core = "3.0"

# OR Native V3 backend (graph-oriented storage)
sqlitegraph-core = { version = "3.0", features = ["native-v3"] }
```

```rust
use sqlitegraph_core::backend::{GraphBackend, NodeSpec};
use sqlitegraph_core::backend::sqlite::SqliteGraphBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = SqliteGraphBackend::in_memory()?;

    let node_id = backend.insert_node(NodeSpec {
        kind: "User".to_string(),
        name: "Alice".to_string(),
        file_path: None,
        data: serde_json::json!({"age": 30}),
    })?;

    println!("Created node: {}", node_id);
    Ok(())
}
```

## CLI

```bash
cargo install sqlitegraph-cli

# Query
sqlitegraph --db graph.db query "MATCH (n:User) RETURN n.name"

# Algorithms
sqlitegraph --db graph.db bfs --start 1 --depth 3
sqlitegraph --db graph.db algo pagerank --iterations 100
```

## Copy-Paste CLI Demo

```bash
rm -f /tmp/sqlitegraph-demo.db

sqlitegraph --db /tmp/sqlitegraph-demo.db --write insert --kind User --name Alice --data '{"age":30}'
sqlitegraph --db /tmp/sqlitegraph-demo.db --write insert --kind User --name Bob --data '{"age":31}'
sqlitegraph --db /tmp/sqlitegraph-demo.db --write query 'CREATE (1)-[:KNOWS]->(2)'

sqlitegraph --db /tmp/sqlitegraph-demo.db query 'MATCH (a:User)-[:KNOWS]->(b:User) RETURN a.name, b.name'
sqlitegraph --db /tmp/sqlitegraph-demo.db algo scc
```

## Hybrid Runtime Demo

This crate includes a runnable demo that combines ordinary SQLite rows, Native
V3 graph metadata, SQLite-backed HNSW vectors, and V3 pub/sub:

```bash
cargo run -p sqlitegraph --example hybrid_sqlite_v3_hnsw_pubsub --features native-v3
```

## Safety Invariants

- Orphan edges are detected by verifying every edge endpoint references a stored entity before any reasoning or subgraph extraction runs.
- Duplicate edges (identical `(from,to,type)` tuples) are tallied so traversal/pipeline counts stay deterministic and regressions surface quickly.
- Invalid label/property references (metadata rows pointing at missing entities) are reported by the safety-check helpers.
- Integrity sweeps perform a deep table walk (entities/edges/labels/properties), verifying sorted IDs, valid JSON payloads, and metadata references before committing to pipelines or migrations.

## DSL Constraints

- Supported clauses are limited to deterministic `pattern`, `k-hop`, `filter type=…`, and `score` steps; ordering matters and only one filter clause is allowed.
- Combination syntax (`CALLS*2`, `CALLS->USES`) must not introduce conflicting filters or unknown tokens—ambiguous or unsupported input causes parser errors surfaced to the CLI/tests.

## Performance & Instrumentation

Performance thresholds in sqlitegraph_bench.json gate releases. Benchmarks produce HTML reports under `target/criterion`. Use `cargo bench --bench bench_insert` (etc.) to isolate suites. The `bench_driver` binary runs all benches sequentially and surfaces pass/fail summaries.

Runtime instrumentation is exposed through the core APIs used by benchmarks and
integration tests: prepare/execute counts, transaction begins/commits/rollbacks,
and cache hits/misses can be captured while reproducing workloads.

## Schema Compatibility Matrix

| Version | Description |
|---------|-------------|
| 1 | Base tables (`graph_entities`, `graph_edges`, `graph_labels`, `graph_properties`) plus indexes and `graph_meta`. |
| 2 | Adds `graph_meta_history` rows so each migration application is recorded; exposed via `run_pending_migrations` / CLI `migrate`. |
| Future | The CLI refuses to open DBs whose version exceeds the compiled `SCHEMA_VERSION`. |

Upgrade workflow:
1. Inspect the database with `sqlitegraph --db <path> status`.
2. Review pending migrations through the library migration helpers.
3. Apply migrations atomically through the library helper; history entries are appended automatically.

## Ecosystem

Tools built on SQLiteGraph:

| Tool | Purpose | Repository | crates.io |
|------|---------|------------|-----------|
| **Magellan** | Code graph indexing, symbol navigation | [github.com/oldnordic/magellan](https://github.com/oldnordic/magellan) | [crates.io/crates/magellan](https://crates.io/crates/magellan) |
| **llmgrep** | Semantic code search | [github.com/oldnordic/llmgrep](https://github.com/oldnordic/llmgrep) | [crates.io/crates/llmgrep](https://crates.io/crates/llmgrep) |
| **Mirage** | CFG analysis, path enumeration | [github.com/oldnordic/mirage](https://github.com/oldnordic/mirage) | [crates.io/crates/mirage-analyzer](https://crates.io/crates/mirage-analyzer) |
| **splice** | Precision code editing | [github.com/oldnordic/splice](https://github.com/oldnordic/splice) | [crates.io/crates/splice](https://crates.io/crates/splice) |

## Documentation

- [Architecture](https://github.com/oldnordic/sqlitegraph/blob/main/docs/ARCHITECTURE.md) - System design
- [Manual](https://github.com/oldnordic/sqlitegraph/blob/main/MANUAL.md) - API guide
- [Query Language](https://github.com/oldnordic/sqlitegraph/blob/main/docs/QUERY_LANGUAGE.md) - Cypher-inspired query reference
- [Changelog](https://github.com/oldnordic/sqlitegraph/blob/main/CHANGELOG.md) - Version history
- [SnapshotId Migration Guide](https://github.com/oldnordic/sqlitegraph/blob/main/docs/SNAPSHOTID_MIGRATION.md) - v2.1.2 API changes

## License

GPL-3.0-only

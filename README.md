# SQLiteGraph

[![crates.io](https://img.shields.io/crates/v/sqlitegraph.svg)](https://crates.io/crates/sqlitegraph)
[![Documentation](https://docs.rs/sqlitegraph/badge.svg)](https://docs.rs/sqlitegraph)

Embedded graph database with dual backend architecture, graph algorithms,
Cypher-inspired queries, and HNSW vector search.

**Positioning:** Single-binary embedded database (no server). Persistent storage with
atomic batch commits. Graph algorithms + HNSW vector search in one engine.
SQLite: stable, mature, excellent for adjacency queries. V3: high-performance,
designed for large-scale graphs, faster for bulk traversals. See benchmarks below.

## Current Main

The current unreleased tree builds on `v2.4.0` with a larger query and
tooling surface:

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
| Max nodes | Unlimited | Unlimited |
| Graph algorithms | 35+ | 35+ |
| HNSW vectors | Yes | Yes |
| Pub/Sub | Yes | Yes |
| LRU Cache | No | Yes (v2.1.0) |
| Parallel BFS | No | Yes (v2.1.1) |

## Benchmarks

See [Architecture](https://github.com/oldnordic/sqlitegraph/blob/main/docs/ARCHITECTURE.md) for system design details.

**Backend Comparison (v2.2.4 — AMD Ryzen 7 7800X3D, tmpfs, Rust 1.95.0):**

| Benchmark | SQLite | V3 | Ratio |
|-----------|--------|----|-------|
| BFS 1K nodes / 5K edges | 2.5ms | 0.45ms | V3 **5.5x faster** |
| BFS 10K / 50K | 26ms | 27ms | ~parity |
| BFS 50K / 250K | 160ms | 586ms | SQLite 3.6x faster |
| DFS 1K / 5K | 2.4ms | 0.46ms | V3 **5.2x faster** |
| Point lookup 1K | 15µs | 82µs | SQLite 5.4x faster |
| Point lookup 10K | 27µs | 503µs | SQLite 18x faster |
| Shortest path 1K | 304µs | 393µs | SQLite 29% faster |

V3 excels at small-scale traversals (contiguous page storage, LRU cache). SQLite dominates at scale (mmap, WAL, FTS5) and point lookups.

**Run benchmarks yourself:**

```bash
git clone https://github.com/oldnordic/sqlitegraph.git
cd sqlitegraph/sqlitegraph-core
cargo bench --features native-v3 --bench backend_comparison
```

## Quick Start

```toml
[dependencies]
# SQLite backend (default)
sqlitegraph = "2.4"

# OR Native V3 backend (faster traversals)
sqlitegraph = { version = "2.4", features = ["native-v3"] }
```

```rust
use sqlitegraph::backend::{GraphBackend, NodeSpec};
use sqlitegraph::backend::sqlite::SqliteGraphBackend;

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

## Copy-Paste Demos

### Python: CRUD, Query, Algorithms, HNSW

```python
from sqlitegraph import Graph

g = Graph.open_in_memory()

alice = g.add_node(kind="User", name="Alice", data={"age": 30})
bob = g.add_node(kind="User", name="Bob", data={"age": 31})
g.add_edge(alice, bob, "KNOWS")

print(g.query("MATCH (a:User)-[:KNOWS]->(b:User) RETURN a.name, b.name"))
print(g.strongly_connected_components())

idx = g.create_hnsw_index("embeddings", dimension=3, metric="cosine")
idx.insert_vector([1.0, 0.8, 0.1], {"label": "graph databases"})
idx.insert_vector([0.1, 0.2, 1.0], {"label": "baking"})
print(idx.search([1.0, 0.9, 0.0], 1))
g.delete_hnsw_index("embeddings")
```

### CLI: From Empty Database to Query Result

```bash
rm -f /tmp/sqlitegraph-demo.db

sqlitegraph --db /tmp/sqlitegraph-demo.db --write insert --kind User --name Alice --data '{"age":30}'
sqlitegraph --db /tmp/sqlitegraph-demo.db --write insert --kind User --name Bob --data '{"age":31}'
sqlitegraph --db /tmp/sqlitegraph-demo.db --write query 'CREATE (1)-[:KNOWS]->(2)'

sqlitegraph --db /tmp/sqlitegraph-demo.db query 'MATCH (a:User)-[:KNOWS]->(b:User) RETURN a.name, b.name'
sqlitegraph --db /tmp/sqlitegraph-demo.db algo scc
```

### Rust: Hybrid Runtime

The Rust example below shows the full mixed-runtime story: ordinary SQLite rows,
Native V3 graph metadata, SQLite-backed HNSW vectors, and V3 pub/sub:

```bash
cargo run -p sqlitegraph --example hybrid_sqlite_v3_hnsw_pubsub --features native-v3
```

## Python

A Python wrapper around the same Rust engine is published to PyPI as
[`sqlitegraph`](https://pypi.org/project/sqlitegraph/). The source lives in
[`sqlitegraph-py/`](./sqlitegraph-py/) and ships as a single `abi3` wheel per
platform (Python 3.10+).

```bash
pip install sqlitegraph
```

```python
from sqlitegraph import Graph

g = Graph.open_in_memory()
alice = g.add_node(kind="User", name="Alice", data={"age": 30})
order = g.add_node(kind="Order", name="Order-123")
g.add_edge(alice, order, "placed")

print(g.neighbors(alice))
```

The Python surface covers node/edge CRUD, BFS/k-hop/shortest path, PageRank,
Louvain, connected components, SCC, label propagation, cycle search,
dominators, critical path, `Graph.query()`, HNSW vector indexes, typed
exceptions (`GraphError`, `NotFoundError`, `InvalidArgumentError`,
`BackendError`), and type stubs for editors. See
[`sqlitegraph-py/README.md`](./sqlitegraph-py/README.md) for the full Python API
and examples.

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

GPL-3.0

# SQLiteGraph

[![crates.io](https://img.shields.io/crates/v/sqlitegraph.svg)](https://crates.io/crates/sqlitegraph)
[![Documentation](https://docs.rs/sqlitegraph/badge.svg)](https://docs.rs/sqlitegraph)

Embedded graph database with dual backend architecture.

**Positioning:** Single-binary embedded database (no server). Persistent storage with
atomic batch commits. Graph algorithms + HNSW vector search in one engine.
SQLite: stable, mature, excellent for adjacency queries. V3: high-performance, 
unlimited scale, faster for bulk traversals. See benchmarks below.

## Backends

| Feature | SQLite | Native V3 |
|---------|--------|-----------|
| Status | Stable | Beta |
| Storage | `.db` file | `.graph` file |
| Max nodes | Unlimited | Unlimited |
| Graph algorithms | 35+ | 35+ |
| HNSW vectors | Yes | Yes |
| Pub/Sub | Yes | Yes |

## Benchmarks

See [BENCHMARK_REPORT.md](https://github.com/oldnordic/sqlitegraph/blob/main/docs/BENCHMARK_REPORT.md) for detailed comparisons.

**Quick Summary:**
- **Point lookup**: V3 ~3-4× faster
- **Adjacency fetch**: SQLite ~20× faster (optimized prepared statements)
- **Bulk traversal**: V3 10-20× faster (contiguous storage)

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
sqlitegraph = "2.0"

# OR Native V3 backend (faster traversals)
sqlitegraph = { version = "2.0", features = ["native-v3"] }
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

# Query (read-only by default)
sqlitegraph --db graph.db query "MATCH (n:User) RETURN n.name"

# Algorithms
sqlitegraph --db graph.db bfs --start 1 --max-depth 3
sqlitegraph --db graph.db pagerank --iterations 100
```

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
- [Algorithms](https://github.com/oldnordic/sqlitegraph/blob/main/docs/GRAPH_ALGORITHMS_GUIDE.md) - 35 graph algorithms
- [Changelog](https://github.com/oldnordic/sqlitegraph/blob/main/CHANGELOG.md) - Version history

## License

GPL-3.0-or-later

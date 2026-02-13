# SQLiteGraph

[![crates.io](https://img.shields.io/crates/v/sqlitegraph.svg)](https://crates.io/crates/sqlitegraph)
[![Documentation](https://docs.rs/sqlitegraph/badge.svg)](https://docs.rs/sqlitegraph)

Embedded graph database with dual backend architecture.

**Positioning:** Single-binary embedded database (no server). Persistent storage with
atomic batch commits. Graph algorithms + HNSW vector search in one engine.
SQLite backend: best for point lookups. Native V3: 10-20× faster for traversals.

## What's New in v2.0.0

**Native V3 Backend** - Production-ready high-performance backend
- B+Tree-based storage with unlimited node capacity (no 2048 limit)
- 10-20× faster traversals compared to SQLite backend
- Full feature parity: graph algorithms, HNSW vectors, Pub/Sub, KV store
- WAL integration for durability and crash recovery

**SQLite Backend Pub/Sub** - Now fully supported
- In-process event notification for graph changes
- Multiple subscriber support with filtered events
- Works across all backends

**HNSW Vector Storage for V3** - Vector search with V3 backend
- `V3VectorStorage` - stores vectors in V3's KV store
- Same HNSW algorithm, different persistence layer
- Unified API across backends

**Reshaped CLI** (`sqlitegraph-cli` crate)
- Read-only by default (`--write` flag for modifications)
- Cypher-like query support: `MATCH (n:User) RETURN n.name`
- Dual backend support (`--backend sqlite|v3`)

## Backends

| Feature | SQLite | Native V3 |
|---------|--------|-----------|
| Status | Stable | Production |
| Storage | `.db` file | `.graph` file |
| Max nodes | Unlimited | Unlimited |
| Graph algorithms | 35+ | 35+ |
| HNSW vectors | Yes | Yes |
| Pub/Sub | Yes | Yes |
| KV Store | Yes | Yes (lazy init) |

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

## Features

- **Dual Backend Architecture**: SQLite (stable, debuggable) or Native V3 (high performance)
- **Graph Algorithms**: 35+ algorithms including BFS, DFS, PageRank, community detection
- **HNSW Vector Search**: Approximate nearest neighbor search with multiple distance metrics
- **ACID Transactions**: Full durability with WAL support
- **Pub/Sub Events**: Real-time notifications for graph changes
- **KV Store**: Key-value storage with TTL and MVCC

## Ecosystem

| Tool | Purpose | Repository | crates.io |
|------|---------|------------|-----------|
| **Magellan** | Code graph indexing | [github.com/oldnordic/magellan](https://github.com/oldnordic/magellan) | [crates.io/crates/magellan](https://crates.io/crates/magellan) |
| **llmgrep** | Semantic code search | [github.com/oldnordic/llmgrep](https://github.com/oldnordic/llmgrep) | [crates.io/crates/llmgrep](https://crates.io/crates/llmgrep) |
| **Mirage** | CFG analysis | [github.com/oldnordic/mirage](https://github.com/oldnordic/mirage) | [crates.io/crates/mirage-analyzer](https://crates.io/crates/mirage-analyzer) |
| **splice** | Precision code editing | [github.com/oldnordic/splice](https://github.com/oldnordic/splice) | [crates.io/crates/splice](https://crates.io/crates/splice) |

## Documentation

- [API Documentation](https://docs.rs/sqlitegraph)
- [Architecture Guide](../docs/ARCHITECTURE.md)
- [Graph Algorithms](../docs/GRAPH_ALGORITHMS_GUIDE.md)
- [Changelog](../CHANGELOG.md)

## License

GPL-3.0-or-later

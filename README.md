# SQLiteGraph

[![crates.io](https://img.shields.io/crates/v/sqlitegraph.svg)](https://crates.io/crates/sqlitegraph)
[![Documentation](https://docs.rs/sqlitegraph/badge.svg)](https://docs.rs/sqlitegraph)

**Embedded Graph Database with Native V2 Backend**

## What's New in v1.2

**Pub/Sub Event System** - In-process event notification for graph changes
- Four event types: `NodeChanged`, `EdgeChanged`, `KVChanged`, `SnapshotCommitted`
- ID-only design for decoupled event schemas
- Channel-based delivery with filtering by event type and entity IDs
- Native V2 backend only

**Full ACID Transactions** - Complete transaction correctness
- Atomicity with full rollback support
- Consistency validation at runtime
- Isolation via MVCC snapshots
- Durability with WAL recovery

**Developer Documentation** - Comprehensive guides for contributors
- [Architecture](docs/ARCHITECTURE.md) - System design and data flow
- [Testing Guide](docs/TESTING.md) - Test patterns and utilities
- [Debugging Guide](docs/DEBUGGING.md) - Profiling and troubleshooting
- [Contributing](docs/CONTRIBUTING.md) - Development workflow

**Test Coverage**: 380+ tests passing (59 pubsub + 42 WAL + 53 MVCC + 27 algorithms + 134 HNSW + 65 others)

---

SQLiteGraph is an embedded graph database in Rust featuring a dual backend architecture. It provides SQLite and Native V2 storage options with graph algorithms, HNSW vector search, and MVCC snapshots.

See [CHANGELOG.md](CHANGELOG.md) for version history.

SQLiteGraph provides two backend options:
- **SQLite Backend**: SQLite storage with ACID transactions
- **Native V2 Backend**: Clustered adjacency storage with WAL

## Features

### Native V2 Architecture
- **Clustered Adjacency Storage**: Stores edges in clusters for locality
- **Write-Ahead Logging (WAL)**: Transaction logging with crash recovery
- **Snapshot System**: Export/import with lifecycle management
- **Cross-Platform Atomic Operations**: Concurrent access across platforms
- **Storage Format**: Binary format with 70%+ size reduction vs legacy V1
- **Pub/Sub Events**: In-process event notification for graph changes (Native V2 only)

### Dual Backend Architecture
- **SQLite Backend**: Traditional SQLite with full ACID transactions
- **Native V2 Backend**: Clustered adjacency for traversal-heavy workloads
- **Unified API**: Single API works with both backends
- **Runtime Selection**: Switch backends via configuration

### Core Graph Operations
- **Entity/Node Management**: Insert, update, retrieve, delete
- **Edge Management**: Create and manage typed relationships
- **JSON Data Storage**: Arbitrary JSON metadata on entities and edges
- **Bulk Operations**: Batch insert for higher throughput

### Traversal & Querying
- **Neighbor Queries**: Get incoming/outgoing connections
- **Pattern Matching**: Graph pattern queries
- **Traversal Algorithms**: BFS, shortest path, connected components

### Graph Algorithms (Phase 8)
- **PageRank**: Importance ranking (O(|E|) iterations)
- **Betweenness Centrality**: Node importance via shortest paths (O(|V||E|))
- **Label Propagation**: Fast community detection (O(|E|))
- **Louvain Method**: Modularity-based clustering (O(|E| log |V|))

### Performance & Reliability
- **MVCC Snapshots**: Read isolation with snapshot views
- **Parallel WAL Recovery**: 2-3x speedup for large WAL files (500+ transactions)
- **Automated Benchmarks**: Criterion-based regression detection
- **Safety Tools**: Orphan edge detection and integrity checks

### Vector Search (HNSW)
- **HNSW Algorithm**: Hierarchical Navigable Small World for ANN search
- **Supported Metrics**: Cosine, Euclidean, Dot Product, Manhattan
- **OpenAI Compatible**: Support for 1536-dimensional embeddings
- **Flexible Dimensions**: Any size from 1-4096

### Developer Tools (Phase 9)
- **Introspection API**: `GraphIntrospection` for statistics and debugging
- **Progress Tracking**: `ProgressCallback` with `ConsoleProgress`
- **CLI Debug Commands**: `debug-stats`, `debug-dump`, `debug-trace`
- **Algorithm CLI Commands**: `pagerank`, `betweenness`, `louvain` with progress bars

## Performance Benchmarks

**Benchmark Methodology:**
- Hardware: Linux x86_64 (kernel 6.18+)
- Sizes: 100-500 nodes (V2 backend has 8MB node region limit, ~2048 nodes max)
- Cache state: Warm (after warmup iterations)
- Measurements: Criterion-based statistical analysis (95% confidence interval)

**Native V2 vs SQLite Backend (Phase 24, 2026-01-21):**

| Operation | Size | Native V2 | SQLite | Ratio |
|-----------|------|-----------|--------|-------|
| Node Insert | 100 | 1.14 ms | 3.63 ms | 3.2x faster |
| Node Insert | 500 | 4.91 ms | 10.57 ms | 2.2x faster |
| Edge Insert (star) | 100 | 3.85 ms | 7.18 ms | 1.9x faster |
| BFS Traversal (star) | 100 | 4.68 ms | 7.28 ms | 1.6x faster |
| BFS Traversal (chain) | 100 | 15.38 ms | 7.24 ms | 2.1x **slower** |
| BFS Traversal (chain) | 500 | 266.50 ms | 24.98 ms | 10.7x **slower** |
| 1-Hop Query | 100 | 3.87 ms | 6.93 ms | 1.8x faster |

**Key Findings:**
- Native V2 excels at insert operations (1.3-3.2x faster)
- Star-pattern traversals favor Native V2 (clustered adjacency locality)
- Chain traversals show regression (V2 cluster lookup overhead vs SQLite indexed adjacency)
- Workload pattern matters: choose backend based on your graph shape and access patterns

**Connection Pooling:**
- Warm checkout: 205 ns (pooled) vs 16.4 µs (direct) = **79.8x faster**
- First checkout overhead: ~5 ms (pool initialization)

**HNSW Vector Search:**
- Insertion: 3-5 ms for 100 vectors (64-256 dimensions)
- Search: Sub-millisecond typical latency
- Accuracy: 95%+ recall on standard datasets

**Storage Efficiency:**
- Native V2 format: 70%+ size reduction vs legacy V1 format

**Caveats:**
- Numbers are for single-node embedded use (not distributed)
- Performance varies based on graph topology, hardware, and configuration
- V2 backend currently constrained to ~2048 nodes (8MB reserved region)
- In-memory benchmarks show 1000-10000x headroom for future optimization

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
sqlitegraph = "1.1"
```

### SQLite Backend (Default)

```rust
use sqlitegraph::{SqliteGraph, GraphEntity, GraphEdge};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = SqliteGraph::open_in_memory()?;

    let user_entity = GraphEntity {
        id: 0,
        kind: "User".to_string(),
        name: "Alice".to_string(),
        file_path: None,
        data: serde_json::json!({"age": 30}),
    };

    let user_id = graph.insert_entity(&user_entity)?;
    println!("Created entity: {}", user_id);

    Ok(())
}
```

### Native V2 Backend

```toml
[dependencies]
sqlitegraph = { version = "1.0", features = ["native-v2"] }
```

```rust
use sqlitegraph::{GraphConfig, open_graph, NodeSpec, EdgeSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = GraphConfig::native();
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("graph.db");

    let graph = open_graph(&db_path, &cfg)?;

    let node_spec = NodeSpec {
        kind: "User".to_string(),
        name: "Alice".to_string(),
        file_path: None,
        data: serde_json::json!({"age": 30}),
    };
    let user_id = graph.insert_node(node_spec)?;

    println!("Created node: {}", user_id);
    Ok(())
}
```

### Pub/Sub Events (Native V2)

```toml
[dependencies]
sqlitegraph = { version = "1.2", features = ["native-v2"] }
```

```rust
use sqlitegraph::{GraphConfig, open_graph};
use sqlitegraph::backend::SubscriptionFilter;

let cfg = GraphConfig::native();
let graph = open_graph("graph.db", &cfg)?;

// Subscribe to all node change events
let filter = SubscriptionFilter::all();
let (subscriber_id, rx) = graph.subscribe(filter)?;

// In a separate task or thread, receive events
while let Ok(event) = rx.recv() {
    println!("Event: {:?}", event);
    // Events contain only IDs - read actual data from graph using snapshot_id
}

// Unsubscribe when done
graph.unsubscribe(subscriber_id)?;
```

## Backend Selection Guide

| Use Case | Recommended Backend | Why |
|----------|-------------------|-----|
| **Write-Heavy Workloads** | Native V2 Backend | 1.3-3.2x faster insert operations |
| **Star-Pattern Graphs** | Native V2 Backend | Clustered adjacency benefits local queries |
| **Chain-Depth Traversals** | SQLite Backend | V2 has 2-10x chain traversal regression |
| **Enterprise Applications** | SQLite Backend | ACID transactions, tooling ecosystem |
| **Existing SQLite Integration** | SQLite Backend | Direct compatibility |
| **Vector Search Workloads** | Native V2 Backend | HNSW integration |
| **Development/Testing** | Either Backend | Unified API, both support in-memory |
| **Small Graphs (<2K nodes)** | Either Backend | V2 has node region limit, SQLite scales better |

### Feature Flags

```toml
# Default - SQLite backend only
sqlitegraph = "1.2"

# Native V2 backend (with pub/sub support)
sqlitegraph = { version = "1.2", features = ["native-v2"] }

# Development features - I/O tracing
sqlitegraph = { version = "1.0", features = ["trace_v2_io"] }
```

## CLI Tool

```bash
# Basic status
sqlitegraph --command status --database memory

# List entities
sqlitegraph --command list --database mygraph.db

# Export/import
sqlitegraph --command dump-graph --output backup.json --database mygraph.db
sqlitegraph --command load-graph --input backup.json --database mygraph.db

# HNSW vector search
sqlitegraph --backend sqlite --db mygraph.db hnsw-create --dimension 768 --distance-metric cosine
sqlitegraph --backend sqlite --db mygraph.db hnsw-insert --index-name vectors --input vectors.json
sqlitegraph --backend sqlite --db mygraph.db hnsw-search --index-name vectors --input query.json --k 10

# Algorithm commands (with progress bars)
sqlitegraph --backend sqlite --db mygraph.db pagerank --progress
sqlitegraph --backend sqlite --db mygraph.db betweenness --progress
sqlitegraph --backend sqlite --db mygraph.db louvain --progress
```

## Graph Algorithms

```rust
use sqlitegraph::algo;

// PageRank - importance ranking
let scores = algo::pagerank(&graph, 0.85, 50)?;

// Betweenness Centrality - node importance via shortest paths
let centrality = algo::betweenness_centrality(&graph)?;

// Label Propagation - fast community detection
let communities = algo::label_propagation(&graph)?;

// Louvain - modularity-based clustering
let partition = algo::louvain_communities(&graph, 0.01)?;

// With progress tracking
use sqlitegraph::progress::ConsoleProgress;
let scores = algo::pagerank_with_progress(&graph, 0.85, 50, ConsoleProgress::new())?;
```

## Testing

**Test Coverage (v1.2):**
- 59 pubsub tests passing (event emission, filtering, multiple subscribers)
- 42 WAL tests passing (recovery, corruption, checkpoints)
- 53 concurrent MVCC tests passing (snapshots, stress testing)
- 27 algorithm tests passing (PageRank, Betweenness, Louvain, Label Propagation)
- 134 HNSW tests passing
- 65 MVCC lifecycle tests passing

```bash
# Run all tests
cargo test --workspace

# With Native V2 backend
cargo test --workspace --features native-v2

# Run benchmarks
cargo bench

# Documentation tests
cargo test --doc
```

## Grounded Tool Scripts

Keep every change truth-based by running the Magellan stack before touching files:

- `scripts/watch-magellan.sh` — starts `magellan watch --root sqlitegraph/src` with `.codemcp/codegraph.db` scoped to the Rust sources.
- `scripts/toolchain-ready.sh [symbol]` — runs `magellan status` + `llmgrep search` (defaults to `ToolRegistry`) so you can verify tool readiness and capture execution IDs before editing.

Run these before any reading/editing steps so the CLI and LLM focus on deterministic spans instead of guessing through `rg`.

## Documentation

### User Documentation
- **[Operator Manual](MANUAL.md)** - Comprehensive usage guide (14 sections)
- **[API Docs](API.md)** - Quick API reference
- **[CHANGELOG](CHANGELOG.md)** - Version history

### Developer Documentation
- **[Documentation Index](docs/INDEX.md)** - Navigation for all docs
- **[Architecture](docs/ARCHITECTURE.md)** - System architecture and design
- **[Testing Guide](docs/TESTING.md)** - Testing patterns and utilities
- **[Debugging Guide](docs/DEBUGGING.md)** - Debugging and profiling
- **[Contributing](docs/CONTRIBUTING.md)** - Contribution guidelines

### Development Guides
- **[Adding a Graph Algorithm](docs/DEVELOPMENT_GUIDES/adding-a-graph-algorithm.md)**
- **[Adding a Distance Metric](docs/DEVELOPMENT_GUIDES/adding-a-distance-metric.md)**
- **[Adding a CLI Command](docs/DEVELOPMENT_GUIDES/adding-a-cli-command.md)**

## Architecture

### Design Principles
- **300 LOC Module Limit**: Maintainable boundaries
- **TDD Methodology**: Test-driven development
- **Performance Benchmarks**: Criterion-based regression gates

### Module Organization
- Core graph operations with dual backend support
- Graph algorithms (centrality, community detection)
- HNSW vector search with persistence
- MVCC snapshots for read isolation
- Introspection and debugging tools

## Built With

SQLiteGraph was developed using the following tools:

- **[Magellan](https://github.com/therootcompany/magellan)** - Code graph navigation and symbol analysis
- **[Splice](https://github.com/jasonzialcita/splice)** - Safe code editing with span-based operations
- **[LLM-Grep](https://github.com/paul-gauthier/llm-grep)** - Semantic code search
- **[Claude Code](https://claude.ai/code)** - AI pair programming

## License

GPL-3.0-or-later - see [LICENSE](LICENSE) for details.

## Contributing

Contributions welcome. Please:
1. Read the [Contributing Guide](docs/CONTRIBUTING.md)
2. Read the [Architecture](docs/ARCHITECTURE.md) for system understanding
3. Run tests to verify setup
4. Follow TDD methodology
5. Keep modules under 300 LOC
6. Add tests for new features

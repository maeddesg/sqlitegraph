# SQLiteGraph

[![crates.io](https://img.shields.io/crates/v/sqlitegraph.svg)](https://crates.io/crates/sqlitegraph)
[![Documentation](https://docs.rs/sqlitegraph/badge.svg)](https://docs.rs/sqlitegraph)

Embedded graph database with dual backend architecture.

**Positioning:** Single-binary embedded database (no server). Persistent storage with
atomic batch commits. Graph algorithms + HNSW vector search in one engine.
SQLite: stable, mature, excellent for adjacency queries. V3: high-performance,
designed for large-scale graphs, faster for bulk traversals. See benchmarks below.

## Recent Changes

**v2.2.0** — Entity queries by kind/name, schema indexes, algorithm determinism fixes, CI hardening:

- `find_entities_by_kind()` and `find_entity_by_kind_and_name()` with composite indexes
- Fixed post-dominators infinite loop on cyclic graphs
- Fixed cycle basis cross-edge detection and deterministic `cyclic_nodes()`
- Hardened CI against runner OOM and non-deterministic test failures

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

**Quick Summary (v2.2.0 - VERIFIED):**

- **Point lookup**: V3 114x faster with LRU cache (warm vs cold cache)
- **Bulk traversal**: V3 10-20x faster (contiguous storage)
- **Adaptive pages**: 15-25% faster (SSD/HDD auto-detection, verified)
- **Delta encoding**: 75-87% space savings
- **Parallel BFS**: Thread-safe chunked processing, sequential fallback for <1K nodes

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
sqlitegraph = "2.2"

# OR Native V3 backend (faster traversals)
sqlitegraph = { version = "2.2", features = ["native-v3"] }
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

## Safety Invariants

- Orphan edges are detected by verifying every edge endpoint references a stored entity before any reasoning or subgraph extraction runs.
- Duplicate edges (identical `(from,to,type)` tuples) are tallied so traversal/pipeline counts stay deterministic and regressions surface quickly.
- Invalid label/property references (metadata rows pointing at missing entities) are reported, and `safety-check --strict` fails builds whenever any of the above appear.
- Integrity sweeps (`safety-check --sweep`) perform a deep table walk (entities/edges/labels/properties), verifying sorted IDs, valid JSON payloads, and metadata references before committing to pipelines or migrations.

## DSL Constraints

- Supported clauses are limited to deterministic `pattern`, `k-hop`, `filter type=…`, and `score` steps; ordering matters and only one filter clause is allowed.
- Combination syntax (`CALLS*2`, `CALLS->USES`) must not introduce conflicting filters or unknown tokens—ambiguous or unsupported input causes parser errors surfaced to the CLI/tests.

## Performance & Instrumentation

Performance thresholds in sqlitegraph_bench.json gate releases. Benchmarks produce HTML reports under `target/criterion`. Use `cargo bench --bench bench_insert` (etc.) to isolate suites. The `bench_driver` binary runs all benches sequentially and surfaces pass/fail summaries.

The CLI metrics command (`sqlitegraph --db <path> --command metrics [--reset-metrics]`) reports the live instrumentation snapshot—prepare/execute counts, transaction begins/commits/rollbacks, plus cache hits/misses—and optionally clears the counters so operators can capture deltas while reproducing workloads.

## Schema Compatibility Matrix

| Version | Description |
|---------|-------------|
| 1 | Base tables (`graph_entities`, `graph_edges`, `graph_labels`, `graph_properties`) plus indexes and `graph_meta`. |
| 2 | Adds `graph_meta_history` rows so each migration application is recorded; exposed via `run_pending_migrations` / CLI `migrate`. |
| Future | The CLI refuses to open DBs whose version exceeds the compiled `SCHEMA_VERSION`. |

Upgrade workflow:
1. Inspect version with `sqlitegraph --command status` (shows `schema_version=N`).
2. Run `sqlitegraph --command migrate --dry-run` to view pending steps.
3. Execute `sqlitegraph --command migrate` (or call the library helper) to apply migrations atomically; history entries are appended automatically.

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
- [Changelog](https://github.com/oldnordic/sqlitegraph/blob/main/CHANGELOG.md) - Version history
- [SnapshotId Migration Guide](https://github.com/oldnordic/sqlitegraph/blob/main/docs/SNAPSHOTID_MIGRATION.md) - v2.1.2 API changes

## License

GPL-3.0

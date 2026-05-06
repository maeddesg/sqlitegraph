# sqlitegraph-cli

[![crates.io](https://img.shields.io/crates/v/sqlitegraph-cli.svg)](https://crates.io/crates/sqlitegraph-cli)
[![Documentation](https://docs.rs/sqlitegraph-cli/badge.svg)](https://docs.rs/sqlitegraph-cli)

Command-line interface for SQLiteGraph graph database.

## Installation

```bash
cargo install sqlitegraph-cli
```

Or install from source:

```bash
git clone https://github.com/oldnordic/sqlitegraph
cd sqlitegraph/sqlitegraph-cli
cargo install --path .
```

## Quick Start

```bash
# Query using Cypher-like syntax (read-only by default)
sqlitegraph --db mygraph.db query "MATCH (n:User) RETURN n.name"

# Run graph algorithms
sqlitegraph --db mygraph.db bfs --start 1 --max-depth 3
sqlitegraph --db mygraph.db pagerank --iterations 100

# Insert data (requires --write flag)
sqlitegraph --db mygraph.db --write insert --kind User --name Alice
```

## Backend Selection

### SQLite Backend (Default)
- Mature, ACID-compliant storage
- Debuggable with standard SQL tools
- Best for point lookups

```bash
sqlitegraph --backend sqlite --db mygraph.db [command]
```

### Native V3 Backend
- 10-20× faster traversals
- Unlimited node capacity
- Binary format for graph workloads

```bash
sqlitegraph --backend v3 --db mygraph.db [command]
```

## Commands

### Query Commands (Read-Only)
```bash
# Cypher-like queries
sqlitegraph query "MATCH (n:User) RETURN n.name"
sqlitegraph query "MATCH (n {role: 'admin'}) RETURN n"

# Graph traversal
sqlitegraph bfs --start 1 --max-depth 3
sqlitegraph path --from 1 --to 10
sqlitegraph neighbors --id 5 --direction outgoing

# Algorithms
sqlitegraph algo pagerank --iterations 100
sqlitegraph algo betweenness
sqlitegraph algo components
```

### Data Modification (Requires --write)
```bash
# Insert nodes
sqlitegraph --write insert --kind User --name Alice

# Import/Export
sqlitegraph --write export --output graph.json
sqlitegraph --write import --input graph.json
```

### Status
```bash
sqlitegraph status
sqlitegraph list
sqlitegraph list --kind User
```

## Read-Only by Default

The CLI is **read-only by default** for safety. Use `--write` flag to enable modifications:

```bash
# This will fail (tries to modify without --write)
sqlitegraph --db graph.db insert --kind User --name Alice

# This works
sqlitegraph --db graph.db --write insert --kind User --name Alice
```

## Ecosystem

This CLI is part of the SQLiteGraph ecosystem:

| Tool | Purpose | Repository |
|------|---------|------------|
| **sqlitegraph** | Core library | [crates.io/crates/sqlitegraph](https://crates.io/crates/sqlitegraph) |
| **sqlitegraph-cli** | This CLI | [crates.io/crates/sqlitegraph-cli](https://crates.io/crates/sqlitegraph-cli) |
| **magellan** | Code graph indexing | [crates.io/crates/magellan](https://crates.io/crates/magellan) |
| **llmgrep** | Semantic code search | [crates.io/crates/llmgrep](https://crates.io/crates/llmgrep) |
| **mirage** | CFG analysis | [crates.io/crates/mirage-analyzer](https://crates.io/crates/mirage-analyzer) |
| **splice** | Code editing | [crates.io/crates/splice](https://crates.io/crates/splice) |

## License

GPL-3.0

# Migration Guide: SQLite to Native V2

**Last Updated:** 2026-02-10
**Version:** v1.5.7

This guide explains how to migrate from the SQLite backend to the Native V2 backend, and helps you decide when it's worth it.

---

## Table of Contents

1. [Should You Migrate?](#should-you-migrate)
2. [Migration Steps](#migration-steps)
3. [Code Changes Required](#code-changes-required)
4. [Feature Differences](#feature-differences)
5. [Rollback Plan](#rollback-plan)
6. [Common Issues](#common-issues)

---

## Should You Migrate?

### Native V2 Advantages

| Benefit | Magnitude | Best For |
|---------|-----------|----------|
| Faster inserts | 1.3-3.2x | Write-heavy workloads |
| Smaller files | ~70% reduction | CI/CD storage, transfer |
| 1-hop queries | 1.8x faster | Direct neighbor lookups |
| Star pattern traversal | 1.6x faster | Code graphs, social networks |
| Pub/Sub events | N/A | Real-time monitoring |

### SQLite Advantages

| Benefit | When It Matters |
|---------|-----------------|
| Unlimited nodes | Large graphs (>2K) |
| Deep traversal | Chain-like graphs |
| SQL tooling | Debugging, ad-hoc queries |
| Ecosystem maturity | Deployed environments |

### Decision Flowchart

```
Start
  │
  ▼
Do you have >2,000 nodes?
  │ Yes → Use SQLite backend
  │ No
  ▼
Is your graph star-shaped (local connections)?
  │ Yes → Native V2 will help
  │ No
  ▼
Is your graph chain-shaped (deep traversals)?
  │ Yes → SQLite is faster
  │ No
  ▼
Do you need pub/sub events?
  │ Yes → Native V2 required
  │ No
  ▼
Do you need smaller file sizes?
  │ Yes → Native V2 wins
  │ No → Either backend works
```

### Example Use Cases

| Use Case | Recommended Backend | Why |
|----------|-------------------|-----|
| Code graph (magellan/splice) | Native V2 | Star-shaped, <2K functions, local calls |
| Dependency graph (cargo) | Native V2 | Star-shaped, pub/sub for changes |
| Social network (followers) | Native V2 | Star-shaped, fast 1-hop queries |
| Call graph analysis | SQLite | Deep traversals, chain patterns |
| Large dataset (100K+ nodes) | SQLite | Exceeds V2 node limit |
| CI/CD artifact storage | Native V2 | 70% smaller files matter |

---

## Migration Steps

### Step 1: Backup Existing Data

```bash
# Export your SQLite database to JSON
sqlitegraph --backend sqlite --db mygraph.db dump-graph --output backup.json --pretty

# Verify the backup
cat backup.json | head -20
```

### Step 2: Update Cargo.toml

```toml
# Before
[dependencies]
sqlitegraph = "1.5.7"

# After
[dependencies]
sqlitegraph = { version = "1.5.7", features = ["native-v2"] }
```

### Step 3: Update Your Code

```rust
// Before
use sqlitegraph::SqliteGraph;

let graph = SqliteGraph::open("mygraph.db")?;

// After
use sqlitegraph::{GraphConfig, open_graph};

let cfg = GraphConfig::native();
let graph = open_graph("mygraph.db", &cfg)?;
```

See [Code Changes Required](#code-changes-required) for more details.

### Step 4: Import Data

```bash
# Import into new Native V2 database
sqlitegraph --backend native --db mygraph-native.db load-graph --input backup.json

# Verify import
sqlitegraph --backend native --db mygraph-native.db status
```

### Step 5: Test Your Application

```bash
# Run your test suite
cargo test --workspace --features native-v2

# Run integration tests
cargo test --test integration_tests
```

### Step 6: Switch Over

Once you've verified everything works:

```bash
# Optional: Keep SQLite as backup
cp mygraph.db mygraph-sqlite-backup.db

# Replace SQLite database with Native V2
mv mygraph-native.db mygraph.db
```

---

## Code Changes Required

### Opening a Graph

```rust
// SQLite backend
use sqlitegraph::SqliteGraph;
let graph = SqliteGraph::open("mygraph.db")?;

// Native V2 backend
use sqlitegraph::{GraphConfig, open_graph};
let cfg = GraphConfig::native();
let graph = open_graph("mygraph.db", &cfg)?;
```

### Configuration Options

```rust
// Native V2 has additional configuration
use sqlitegraph::{GraphConfig, open_graph};

let cfg = GraphConfig::native()
    .with_auto_checkpoint(true)    // Enable auto-checkpoint
    .with_wal_sync(true);           // Sync WAL to disk on commit

let graph = open_graph("mygraph.db", &cfg)?;
```

### Pub/Sub Events (Native V2 Only)

```rust
use sqlitegraph::backend::SubscriptionFilter;

// Subscribe to all node changes
let filter = SubscriptionFilter::all();
let (subscriber_id, rx) = graph.subscribe(filter)?;

// Receive events in a background task
while let Ok(event) = rx.recv() {
    match event {
        PubSubEvent::NodeChanged { node_id, snapshot_id } => {
            println!("Node {} changed at snapshot {}", node_id, snapshot_id);
        }
        // ... handle other event types
    }
}

// Clean up
graph.unsubscribe(subscriber_id)?;
```

### Forcing WAL Flush

```rust
// Native V2: Force WAL buffer to disk
use sqlitegraph::backend::GraphBackend;

graph.flush()?;  // Ensures KV data is visible to other processes
```

---

## Feature Differences

### SQLite Backend Only

| Feature | Why Not in Native V2 |
|---------|---------------------|
| Raw SQL access | Native V2 uses binary format |
| External tools (sqlite3 CLI) | Custom file format |
| Unlimited nodes | 8MB node region limit |
| Full SQL query language | Graph-optimized storage |

### Native V2 Backend Only

| Feature | Why Not in SQLite |
|---------|-------------------|
| Pub/Sub events | SQLite doesn't have in-process event system |
| Clustered edge storage | SQLite uses row storage |
| WAL-based recovery | SQLite has its own WAL |
| Cross-process KV communication | Designed for multi-process tooling |

### Both Backends

| Feature | Notes |
|---------|-------|
| Graph algorithms (35 algorithms) | Same API, different performance |
| HNSW vector search | Same implementation |
| MVCC snapshots | Both support isolated reads |
| JSON export/import | Same format |

---

## Rollback Plan

If you need to rollback to SQLite:

### Immediate Rollback

```bash
# Stop your application

# Restore from backup
cp mygraph-sqlite-backup.db mygraph.db

# Update Cargo.toml
# [dependencies]
# sqlitegraph = "1.5.7"  # Remove native-v2 feature

# Rebuild
cargo build --release

# Restart application
```

### Export from Native V2 First

If you made changes after migration:

```bash
# Export Native V2 data
sqlitegraph --backend native --db mygraph.db dump-graph --output backup-native.json

# Import into SQLite
sqlitegraph --backend sqlite --db mygraph-sqlite.db load-graph --input backup-native.json
```

---

## Common Issues

### Issue: "Too many open files"

**Symptom:** Error opening Native V2 database with many concurrent connections.

**Solution:** Increase file descriptor limit:
```bash
# Check current limit
ulimit -n

# Increase limit
ulimit -n 4096

# Make permanent (add to ~/.bashrc or ~/.zshrc)
echo "ulimit -n 4096" >> ~/.bashrc
```

### Issue: "Out of node slots"

**Symptom:** Error inserting nodes after ~2,048 insertions.

**Solution:** You've hit the Native V2 node limit. Options:
1. Migrate to SQLite backend (unlimited nodes)
2. Archive old data to a separate graph file
3. Wait for V3 backend (planned)

```rust
// Check how close you are to the limit
use sqlitegraph::introspection::GraphIntrospection;

let stats = graph.introspection()?;
println!("Nodes: {}/2048", stats.node_count());
```

### Issue: "WAL recovery failed"

**Symptom:** Database won't open after crash.

**Solution:** The WAL file may be corrupted.
```bash
# Delete WAL file (main file is safe)
rm mygraph.db.wal

# Reopen database (will create new WAL)
sqlitegraph --backend native --db mygraph.db status
```

### Issue: "Performance worse after migration"

**Symptom:** Native V2 is slower than SQLite for your workload.

**Solution:** Your graph may be chain-shaped (deep traversals). Native V2 excels at star patterns.

**Benchmark your specific workload:**
```bash
# SQLite benchmark
time sqlitegraph --backend sqlite --db mygraph.db bfs --start 1 --max-depth 10

# Native V2 benchmark
time sqlitegraph --backend native --db mygraph.db bfs --start 1 --max-depth 10
```

If SQLite is consistently faster, stick with SQLite. The unified API means switching back is trivial.

### Issue: "Pub/Sub events not firing"

**Symptom:** Subscribers don't receive events.

**Solution:** Events are only emitted on **commit**, not rollback.
```rust
// This emits events
let mut txn = graph.begin_txn()?;
graph.insert_node(node_spec)?;
txn.commit()?;  // ← Events emitted here

// This does NOT emit events
let mut txn = graph.begin_txn()?;
graph.insert_node(node_spec)?;
txn.rollback()?;  // ← No events
```

### Issue: "KV data not visible to other processes"

**Symptom:** magellan writes to KV, but llmgrep doesn't see the data.

**Solution:** Call `flush()` after KV writes.
```rust
// Process A (magellan)
graph.kv_put(key, value)?;
graph.flush()?;  // ← Force WAL buffer to disk

// Process B (llmgrep)
// Now can read the value
let value = graph.kv_get(&key)?;
```

---

## Migration Checklist

Use this checklist to ensure a smooth migration:

- [ ] Read [Should You Migrate?](#should-you-migrate) section
- [ ] Backup existing SQLite database
- [ ] Update `Cargo.toml` with `native-v2` feature
- [ ] Update code to use `GraphConfig::native()`
- [ ] Import data into new Native V2 database
- [ ] Run test suite with `--features native-v2`
- [ ] Benchmark critical operations
- [ ] Test pub/sub if using events
- [ ] Verify `flush()` behavior if using cross-process KV
- [ ] Keep SQLite backup for rollback
- [ ] Monitor node count to stay under 2,048 limit
- [ ] Document migration decision for future team members

---

## Further Reading

- [ARCHITECTURE.md](ARCHITECTURE.md) - Backend architecture details
- [MANUAL.md](../MANUAL.md) - Backend selection guide
- [TESTING.md](TESTING.md) - Testing with both backends
- [README.md](../README.md) - Performance benchmarks

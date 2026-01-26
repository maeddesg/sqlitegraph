# Debugging Guide

**Last Updated:** 2026-01-26
**Version:** v1.2.0

This guide covers debugging and profiling techniques for SQLiteGraph development.

---

## Table of Contents

1. [Debug Builds](#debug-builds)
2. [Logging](#logging)
3. [Introspection APIs](#introspection-apis)
4. [Debugging Tools](#debugging-tools)
5. [Profiling](#profiling)
6. [Common Issues](#common-issues)

---

## Debug Builds

### Standard Debug Build

```bash
# Build with debug symbols
cargo build

# Run with debug output
RUST_LOG=debug cargo run --features debug
```

### Debug Feature

The `debug` feature enables verbose logging:

```toml
[dependencies]
sqlitegraph = { version = "1.2", features = ["debug"] }
```

```bash
cargo build --features debug
```

**Note:** Debug logging is zero-overhead when the feature is disabled.

### Verbose Output

```bash
# Verge compiler output
cargo build --verbose

# Verge test output
cargo test -- --nocapture --verbose

# Verge benchmark output
cargo bench -- --verbose
```

---

## Logging

### Environment Variables

```bash
# Set log level
export RUST_LOG=error     # Only errors
export RUST_LOG=warn      # Warnings and errors
export RUST_LOG=info      # Info, warnings, errors
export RUST_LOG=debug     # Debug, info, warnings, errors
export RUST_LOG=trace     # All logging

# Target specific module
export RUST_LOG=sqlitegraph::backend::native::v2=debug

# Run with logging
RUST_LOG=debug cargo run
```

### Debug Macros in Code

**Location:** `src/debug.rs`

```rust
use crate::debug::{debug_log, info_log, warn_log, error_log};

// Debug: Only compiled with `debug` feature
debug_log!("This is a debug message: {}", value);

// Info: Only compiled with `debug` feature
info_log!("This is an info message");

// Warning: Always compiled
warn_log!("This is a warning: {}", warning);

// Error: Always compiled
error_log!("This is an error: {}", error);
```

### V2 I/O Tracing

The `trace_v2_io` feature enables detailed I/O tracing for the Native V2 backend:

```bash
cargo run --features trace_v2_io
```

This logs:
- File read/write operations
- WAL record details
- Cluster allocation/deallocation
- Cache hits/misses

---

## Introspection APIs

### GraphIntrospection

**Location:** `src/introspection.rs`

```rust
use sqlitegraph::introspection::GraphIntrospection;

let intro = GraphIntrospection::new(&graph)?;

// Get exact node count
let nodes: usize = intro.node_count()?;

// Get edge count estimate (min, max)
let (min_edges, max_edges) = intro.edge_count_estimate()?;

// Get backend-specific info
let info: serde_json::Value = intro.backend_info()?;
println!("{}", serde_json::to_string_pretty(&info)?);

// Export full state as JSON
let json: String = intro.to_json()?;
println!("{}", json);
```

### CLI Debug Commands

```bash
# Show graph statistics
sqlitegraph --backend native --db graph.db debug-stats

# Dump graph structure
sqlitegraph --backend native --db graph.db debug-dump --output graph.json

# Trace specific operation
sqlitegraph --backend native --db graph.db debug-trace bfs --start 1 --max-depth 10
```

### Backend-Specific Info

#### SQLite Backend

```rust
use sqlitegraph::SqliteGraph;

let graph = SqliteGraph::open("graph.db")?;

// Get raw connection for debugging
let conn = graph.conn();

// Run raw SQL queries
use rusqlite::params;
let mut stmt = conn.prepare("SELECT COUNT(*) FROM entities")?;
let count: i64 = stmt.query_row([], |row| row.get(0))?;
```

#### Native V2 Backend

```rust
use sqlitegraph::{GraphConfig, open_graph};

let config = GraphConfig::native();
let graph = open_graph("graph.db", &config)?;

// Access V2 internals (requires unsafe)
use sqlitegraph::backend::native::v2::NativeGraphBackend;
if let Some(native) = graph.as_any().downcast_ref::<NativeGraphBackend>() {
    let file = native.graph_file();
    let wal_manager = native.wal_manager();

    // Inspect WAL state
    let metrics = wal_manager.metrics()?;
    println!("WAL size: {}", metrics.current_size);
}
```

---

## Debugging Tools

### rust-gdb / lldb

```bash
# Start debug session
rust-gdb target/debug/sqlitegraph-cli

# Or with lldb on macOS
rust-lldb target/debug/sqlitegraph-cli
```

**Common GDB commands:**

```
# Set breakpoint
(gdb) break sqlitegraph::backend::native::v2::wal::manager::commit_transaction

# Run program
(gdb) run --backend native --db test.db status

# Backtrace
(gdb) bt

# Print variable
(gdb) print node_id

# Continue
(gdb) c

# Step next
(gdb) n

# Step into function
(gdb) s

# Finish current function
(gdb) fin
```

### rr (Record and Replay)

```bash
# Record execution
rr record target/debug/sqlitegraph-cli --backend native --db test.db status

# Replay
rr replay

# Reverse debugging
(rr) reverse-continue
(rr) reverse-next
```

### Valgrind (Memory Leaks)

```bash
# Check for memory leaks
valgrind --leak-check=full --show-leak-kinds=all target/debug/sqlitegraph-cli

# With suppressions (Rust has false positives)
valgrind --suppressions=/path/to/rust.supp target/debug/sqlitegraph-cli
```

### Address Sanitizer

```bash
# Run with address sanitizer
RUSTFLAGS="-Z sanitizer=address" cargo run

# With thread sanitizer
RUSTFLAGS="-Z sanitizer=thread" cargo run
```

---

## Profiling

### flamegraph

```bash
# Install flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bin sqlitegraph-cli --features native-v2 -- \
    --backend native --db test.db bfs --start 1 --max-depth 100

# Open flamegraph
open flamegraph.svg
```

### perf (Linux)

```bash
# Record perf data
perf record -g target/release/sqlitegraph-cli --backend native --db test.db bfs

# Report
perf report

# Annotate specific function
perf annotate sqlitegraph::algo::bfs
```

### Instruments (macOS)

```bash
# Build with symbols
cargo build --release

# Open in Instruments
instruments -t "Time Profiler" target/release/sqlitegraph-cli \
    --backend native --db test.db bfs
```

### Criterion Benchmark Profiling

**Location:** `benches/`

```bash
# Build benchmark with debug symbols
cargo bench --profile bench --bench bfs

# This generates:
# - target/criterion/bfs/*/profile/flamegraph.svg
```

**Profile configuration in `Cargo.toml`:**

```toml
[profile.bench]
inherits = "release"
debug = true  # Keep symbols for profiling
```

### Heap Profiling

```bash
# Use dhat (heap profiling)
cargo install cargo-dhat

# Run with dhat
RUSTFLAGS='-Zno-share-generics' cargo dhat --features native-v2 -- \
    --bench bfs

# Analyze heap
```

---

## WAL Debugging

### WAL Inspection

```bash
# Checkpoint status
sqlitegraph --backend native --db graph.db wal-checkpoint

# WAL metrics
sqlitegraph --backend native --db graph.db wal-metrics

# WAL configuration
sqlitegraph --backend native --db graph.db wal-config

# Detailed WAL stats
sqlitegraph --backend native --db graph.db wal-stats
```

### WAL File Analysis

```python
# Simple Python script to inspect WAL file
import struct

with open('graph.db.wal', 'rb') as f:
    header = f.read(100)
    magic = struct.unpack('<8s', header[:8])[0]
    print(f"Magic: {magic}")
    # ... more parsing
```

### Checkpoint Debugging

```bash
# Force manual checkpoint
sqlitegraph --backend native --db graph.db wal-checkpoint

# Check checkpoint state
sqlitegraph --backend native --db graph.db debug-stats | jq '.checkpoint'
```

---

## Common Issues

### Issue: "Database is locked"

**Cause:** SQLite has a write lock; another connection is writing.

**Solution:**
```rust
// Use connection pooling
let pool = SqliteGraph::open_with_pool("graph.db", 4)?;

// Or use separate connections for reads/writes
let write_conn = SqliteGraph::open("graph.db")?;
let read_conn = write_conn.snapshot()?;
```

### Issue: "WAL replay failed"

**Cause:** WAL file is corrupted or out of sync with main file.

**Solution:**
```bash
# Force checkpoint (replays WAL into main)
sqlitegraph --backend native --db graph.db wal-checkpoint

# If checkpoint fails, WAL is corrupted
# Recovery: Delete WAL file (loses uncommitted transactions)
rm graph.db.wal
```

### Issue: "Node not found" (but node exists)

**Cause:** Reading from stale snapshot.

**Solution:**
```rust
// Don't hold snapshots too long
{
    let snapshot = graph.snapshot()?;
    // ... read operations
} // Snapshot dropped here

// Or create fresh snapshot for each operation
```

### Issue: Performance regression after upgrade

**Cause:** New version changed storage format or algorithm.

**Solution:**
```bash
# Run regression benchmarks
cargo bench --bench regression_write_cost
cargo bench --bench regression_memory

# Compare with baseline
cargo bench -- --baseline main
```

### Issue: Out of memory on large graphs

**Cause:** Native V2 node region limit (8MB, ~2048 nodes).

**Solution:**
```bash
# Check node count
sqlitegraph --backend native --db graph.db debug-stats | jq '.node_count'

# If near limit, use SQLite backend instead
# (This is a known V2 limitation)
```

---

## Debugging Pub/Sub

### Event Logging

```rust
use sqlitegraph::backend::{SubscriptionFilter, PubSubEvent};

let filter = SubscriptionFilter::all();
let (subscriber_id, rx) = graph.subscribe(filter)?;

// In a separate thread
while let Ok(event) = rx.recv() {
    match event {
        PubSubEvent::NodeChanged { node_id, snapshot_id } => {
            eprintln!("Node {} changed in snapshot {}", node_id, snapshot_id);
        }
        _ => { /* ... */ }
    }
}
```

### Debugging Dropped Receivers

```bash
# Check if events are being dropped
# Run with debug logging
RUST_LOG=sqlitegraph::backend::native::v2::pubsub=debug cargo run

# Look for:
# "Dropping event for subscriber X: receiver dropped"
```

---

## Performance Investigation Workflow

1. **Identify Slow Operation**
   ```bash
   # Run with timing
   time sqlitegraph --backend native --db graph.db bfs --start 1 --max-depth 100
   ```

2. **Generate Flamegraph**
   ```bash
   cargo flamegraph --bin sqlitegraph-cli -- \
       --backend native --db graph.db bfs --start 1 --max-depth 100
   ```

3. **Examine Hot Paths**
   ```bash
   # Open flamegraph.svg in browser
   # Look for wide/long bars
   ```

4. **Check Cache Effectiveness**
   ```bash
   # Run introspection
   sqlitegraph --backend native --db graph.db debug-stats | jq '.cache'
   ```

5. **Compare Backends**
   ```bash
   # Test SQLite vs Native
   time sqlitegraph --backend sqlite --db test.db bfs --start 1
   time sqlitegraph --backend native --db test.db bfs --start 1
   ```

6. **Profile Memory**
   ```bash
   # Use memory profiler
   valgrind --tool=massif target/release/sqlitegraph-cli ...
   ```

---

## Further Reading

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [TESTING.md](TESTING.md) - Testing patterns
- [MANUAL.md](../MANUAL.md) - Operator manual
- [README.md](../README.md) - User documentation

# Troubleshooting Guide

**Last Updated:** 2026-02-10
**Version:** v1.5.7

Common issues and solutions when using SQLiteGraph.

---

## Table of Contents

1. [Installation & Build Issues](#installation--build-issues)
2. [Runtime Issues](#runtime-issues)
3. [Performance Issues](#performance-issues)
4. [Native V2 Specific Issues](#native-v2-specific-issues)
5. [SQLite Backend Specific Issues](#sqlite-backend-specific-issues)
6. [Testing Issues](#testing-issues)
7. [Getting Help](#getting-help)

---

## Installation & Build Issues

### "Library not found: sqlite3"

**Symptom:** Build fails with `error: linking with cc failed: /usr/bin/ld: cannot find -lsqlite3`

**Cause:** SQLite development headers not installed.

**Solution:**

```bash
# Debian/Ubuntu
sudo apt-get install libsqlite3-dev

# macOS (typically pre-installed, but if needed)
brew install sqlite3

# Arch Linux
sudo pacman -S sqlite

# Fedora/RHEL
sudo dnf install sqlite-devel

# Windows (vcpkg)
vcpkg install sqlite3
# Then set SQLITE3_LIB_PATH environment variable
```

### "Feature native-v3 not found"

**Symptom:** Build fails with `error: unused manifest key: sqlitegraph-core/native-v3`

**Cause:** Native V2 is a feature flag, not a separate crate.

**Solution:**

```toml
# In your Cargo.toml
[dependencies]
sqlitegraph = { version = "3.0", features = ["native-v3"] }
```

### "Functionality mismatch: expected 7, found 5" (Internal)

**Symptom:** Tests fail with schema mismatch errors.

**Cause:** Database schema version mismatch. You may have an old database file.

**Solution:**

```bash
# Delete old database and rebuild
rm mygraph.db
cargo run
```

### "Duplicate definition of module: algo"

**Symptom:** Build error about duplicate module definitions.

**Cause:** Conflicting module imports.

**Solution:** Check your `src/lib.rs`:
```rust
// Don't do this:
pub mod algo;
pub mod algo;  // ← Duplicate!

// Do this instead:
pub mod algo;
```

---

## Runtime Issues

### "Database is locked"

**Symptom:** Operations fail with `SqliteGraphError::DatabaseLocked`

**Cause (SQLite Backend):** Multiple writers trying to write simultaneously. SQLite allows only one writer at a time.

**Solution:**

```rust
// Use a connection pool or serialize writes
use std::sync::{Arc, Mutex};

let graph = Arc::new(Mutex::new(SqliteGraph::open("mygraph.db")?));

// Spawn threads with serialized access
let handles: Vec<_> = (0..10)
    .map(|_| {
        let graph = graph.clone();
        std::thread::spawn(move || {
            let g = graph.lock().unwrap();
            // ... perform operations
        })
    })
    .collect();

for handle in handles {
    handle.join().unwrap();
}
```

**Solution (Native V2):** Native V3 also serializes writes via WAL. Use a similar approach.

### "Node not found: 12345"

**Symptom:** `get_node()` returns `NotFoundError`

**Cause:** Node ID doesn't exist, or was deleted.

**Solution:**

```rust
// Check if node exists before accessing
let result = graph.get_node(node_id);
match result {
    Ok(node) => println!("Found: {}", node.name),
    Err(SqliteGraphError::NotFoundError(msg)) => {
        eprintln!("Node {} not found: {}", node_id, msg);
        // Handle missing node
    }
    Err(e) => return Err(e),
}
```

### "WAL file corrupted"

**Symptom:** Database won't open after crash.

**Solution:**

```bash
# Native V2: Delete WAL file
rm mygraph.db.wal

# The main file is safe; database will recover on next open
```

```bash
# SQLite: Delete WAL file
rm mygraph.db-wal mygraph.db-shm

# SQLite will recover on next open
```

### "Too many open files"

**Symptom:** Error after opening many databases or files.

**Solution:**

```bash
# Check current limit
ulimit -n

# Increase limit (temporary)
ulimit -n 4096

# Make permanent
echo "ulimit -n 4096" >> ~/.bashrc  # or ~/.zshrc
source ~/.bashrc
```

---

## Performance Issues

### "Queries are slow"

**Symptom:** Operations take longer than expected.

**Diagnosis:**

```rust
use sqlitegraph::introspection::GraphIntrospection;

let stats = graph.introspection()?;
println!("Nodes: {}", stats.node_count());
println!("Edges: {}", stats.edge_count_estimate());
println!("Cache hits: {}/{}", stats.cache_hits(), stats.cache_lookups());
```

**Solutions:**

1. **Check cache hit rate:** Low hit rate means lots of disk I/O.
2. **Use snapshots for multiple reads:**
   ```rust
   // Bad: Multiple round-trips
   for &node_id in &node_ids {
       graph.get_node(node_id)?;
   }

   // Good: Single snapshot
   let snapshot = graph.snapshot()?;
   for &node_id in &node_ids {
       snapshot.get_node(node_id)?;
   }
   ```

3. **Consider Native V3 for star-pattern graphs:**
   ```bash
   # Benchmark both backends
   time sqlitegraph --backend sqlite --db mygraph.db bfs --start 1 --max-depth 5
   time sqlitegraph --backend native --db mygraph-native.db bfs --start 1 --max-depth 5
   ```

### "High memory usage"

**Symptom:** Process uses lots of memory.

**Diagnosis:**

```rust
// Check cache size
let stats = graph.introspection()?;
println!("Cache entries: {}", stats.cache_size());
```

**Solution:**

```rust
// Reduce cache size or disable cache
use sqlitegraph::{GraphConfig, open_graph};

let cfg = GraphConfig::native()
    .with_cache_size(100)?;  // Limit to 100 entries

let graph = open_graph("mygraph.db", &cfg)?;
```

### "Inserts are slow"

**Symptom:** `insert_node()` or `insert_edge()` takes too long.

**Solutions:**

1. **Use batch inserts:**
   ```rust
   // Bad: Individual transactions
   for spec in &node_specs {
       graph.insert_node(spec)?;  // Each insert is a transaction
   }

   // Good: Single transaction
   let mut txn = graph.begin_txn()?;
   for spec in node_specs {
       txn.insert_node(spec)?;
   }
   txn.commit()?;
   ```

2. **Disable auto-sync (if acceptable):**
   ```rust
   let cfg = GraphConfig::native()
       .with_wal_sync(false);  // Faster, but slightly less safe

   let graph = open_graph("mygraph.db", &cfg)?;
   ```

3. **Use Native V3 for write-heavy workloads:**
   - Native V3: 1.3-3.2x faster inserts

---

## Native V2 Specific Issues

### "Out of node slots"

**Symptom:** Error inserting nodes after ~2,048 insertions.

**Cause:** Native V2 has an 8MB node region limit (~2,048 nodes at 256 bytes each).

**Solutions:**

1. **Check proximity to limit:**
   ```rust
   use sqlitegraph::introspection::GraphIntrospection;

   let stats = graph.introspection()?;
   println!("Nodes: {}/2048", stats.node_count());

   if stats.node_count() > 1800 {
       eprintln!("Warning: Approaching node limit!");
   }
   ```

2. **Migrate to SQLite backend:**
   See [MIGRATION.md](MIGRATION.md) for steps.

3. **Archive old data:**
   ```rust
   // Export old nodes to JSON
   let snapshot = graph.snapshot()?;
   let old_nodes: Vec<_> = snapshot
       .all_node_ids()?
       .iter()
       .filter(|&&id| is_old(id, &snapshot))
       .collect();

   // Save to archive
   std::fs::write("archive.json", serde_json::to_string(&old_nodes)?)?;

   // Delete from graph
   for &node_id in &old_nodes {
       graph.delete_node(node_id)?;
   }
   ```

### "KV data not visible to other processes"

**Symptom:** Process A writes to KV, but Process B doesn't see the data.

**Cause:** WAL buffer not flushed to disk.

**Solution:**

```rust
// Process A (writer)
use sqlitegraph::backend::GraphBackend;

graph.kv_put(key, value)?;
graph.flush()?;  // ← Force WAL buffer to disk
```

```rust
// Process B (reader)
// Now the value is visible
let value = graph.kv_get(&key)?;
```

### "Pub/Sub events not firing"

**Symptom:** Subscribers don't receive events.

**Causes:**

1. **Events only emit on commit, not rollback:**
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

2. **Subscription filter doesn't match:**
   ```rust
   // Subscribe to everything first
   let filter = SubscriptionFilter::all();

   // Then narrow down once you confirm events work
   let filter = SubscriptionFilter::node_changes();
   ```

3. **Receiver not being polled:**
   ```rust
   // In a separate task/thread
   while let Ok(event) = rx.recv() {
       // Handle event
   }

   // If you don't call recv(), events queue up and may be dropped
   ```

### "Cluster offset corruption"

**Symptom:** Errors reading edges, inconsistent results.

**Cause:** Bug in cluster allocation (fixed in v1.5.x).

**Solution:** Upgrade to latest version:
```toml
[dependencies]
sqlitegraph-core = "3.0"
```

If the issue persists, the database may be corrupted:
```bash
# Export data
sqlitegraph --backend native --db mygraph.db dump-graph --output backup.json

# Create new database
sqlitegraph --backend native --db mygraph-new.db load-graph --input backup.json

# Replace old database
mv mygraph-new.db mygraph.db
```

---

## SQLite Backend Specific Issues

### "Database disk image is malformed"

**Symptom:** SQLite reports corrupted database.

**Cause:** File system corruption, concurrent writes, or crash during write.

**Solutions:**

1. **Try to recover:**
   ```bash
   # SQLite recovery mode
   sqlite3 mygraph.db "PRAGMA integrity_check;"

   # If errors found, dump and reload
   sqlite3 mygraph.db ".dump" > dump.sql
   sqlite3 mygraph-new.db < dump.sql
   ```

2. **Use SQLiteGraph export/import:**
   ```bash
   # Export what you can
   sqlitegraph --backend sqlite --db mygraph.db dump-graph --output backup.json

   # Import to new database
   sqlitegraph --backend sqlite --db mygraph-new.db load-graph --input backup.json
   ```

### "No such table: entities"

**Symptom:** Error about missing tables.

**Cause:** Database file exists but schema not initialized.

**Solution:**

```rust
// Don't create an empty file manually
// Let SQLiteGraph create it

// Bad
std::fs::File::create("mygraph.db")?;
let graph = SqliteGraph::open("mygraph.db")?;  // Schema not initialized

// Good
let graph = SqliteGraph::open("mygraph.db")?;  // Creates and initializes
```

Or delete the empty file:
```bash
rm mygraph.db
cargo run
```

### "SQLITE_CORRUPT: database disk image is malformed"

**Symptom:** Operations fail with corruption error.

**Causes:**
- Concurrent writes from multiple processes
- Crash during write
- File system issues (NAS, network drives)

**Solutions:**

1. **Use a single writer:** Ensure only one process writes at a time.
2. **Avoid network drives:** Store database on local storage.
3. **Recover from backup:**
   ```bash
   # If you have a backup
   cp mygraph-backup.db mygraph.db

   # If not, try export
   sqlitegraph --backend sqlite --db mygraph.db dump-graph --output partial.json
   ```

---

## Testing Issues

### "Tests pass individually but fail together"

**Symptom:** Running `cargo test` fails, but `cargo test test_name` passes.

**Cause:** Tests sharing state or database files.

**Solution:** Ensure each test uses a unique database:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_something() {
        // Each test gets its own temp directory
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let graph = SqliteGraph::open(db_path.to_str().unwrap()).unwrap();

        // ... test code

        // TempDir is deleted when dropped
    }
}
```

### "Flaky tests: sometimes pass, sometimes fail"

**Symptom:** Test results are inconsistent.

**Causes:**

1. **Race conditions in concurrent tests:**
   ```bash
   # Run tests sequentially to debug
   cargo test -- --test-threads=1
   ```

2. **Timing-dependent assertions:**
   ```rust
   // Bad: Assumes specific timing
   assert!(elapsed < 10_ms);

   // Good: Use generous timeout
   assert!(elapsed < 1_sec, "Operation took {:?}", elapsed);
   ```

3. **Shared state between tests:**
   Use `TempDir` as shown above.

### "Benchmarks too slow"

**Symptom:** `cargo bench` takes forever.

**Solution:** Reduce sample size in `benches/Cargo.toml`:

```toml
[bench]
# Reduce iterations for faster development
harness = false

# In your benchmark code:
let mut group = c.benchmark_group("my_bench");
group.sample_size(10);  // Default is 100
group.measurement_time(std::time::Duration::from_secs(5));  # Default is 10
```

### "Coverage report missing lines"

**Symptom:** Tarpaulin shows low coverage.

**Solution:**

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Run with verbose output to see what's being tested
cargo tarpaulin --workspace --features native-v2 --out Html --verbose

# Check specific modules
cargo tarpaulin --lib --features native-v2 -- --test-threads=1
```

---

## Getting Help

### Before Asking for Help

1. **Check the version:** Ensure you're using the latest version:
   ```bash
   cargo tree | grep sqlitegraph
   ```

2. **Search existing issues:** https://github.com/oldnordic/sqlitegraph/issues

3. **Create a minimal reproduction:**
   ```rust
   // Minimal example that shows the problem
   use sqlitegraph::SqliteGraph;

   fn main() -> Result<(), Box<dyn std::error::Error>> {
       let graph = SqliteGraph::open_in_memory()?;

       // This fails
       graph.get_node(999)?;  // ← What you expect to happen

       Ok(())
   }
   ```

4. **Gather diagnostic info:**
   ```rust
   use sqlitegraph::introspection::GraphIntrospection;

   let stats = graph.introspection()?;
   println!("Backend: {:?}", stats.backend_info());
   println!("Nodes: {}", stats.node_count());
   println!("Edges: {}", stats.edge_count_estimate());
   println!("Cache hits: {}/{}", stats.cache_hits(), stats.cache_lookups());
   ```

### Where to Ask

| Channel | Purpose | Response Time |
|---------|---------|---------------|
| [GitHub Issues](https://github.com/oldnordic/sqlitegraph/issues) | Bug reports, feature requests | Days to weeks |
| [GitHub Discussions](https://github.com/oldnordic/sqlitegraph/discussions) | General questions | Variable |
| Documentation | Common issues | Immediate (if documented) |

### When Reporting Bugs

Include:

1. **SQLiteGraph version:** `cargo tree | grep sqlitegraph`
2. **Rust version:** `rustc --version`
3. **OS and version:** `uname -a`
4. **Minimal reruntime code**
5. **Error message (full)**
6. **Expected behavior**
7. **Actual behavior**
8. **Steps to reproduce**

### Bug Report Template

```markdown
## Bug Description

Brief description of the bug.

## Reproduction Steps

1. Create a graph with...
2. Call `some_function()`
3. Observe error

## Expected Behavior

What should happen.

## Actual Behavior

What actually happens.

## Environment

- SQLiteGraph version: x.y.z
- Rust version: ...
- OS: ...
- Backend: SQLite / Native V2

## Code

```rust
// Minimal reproduction
```

## Error Output

```
Paste full error here
```
```

---

## Further Reading

- [MIGRATION.md](MIGRATION.md) - Backend migration guide
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [TESTING.md](TESTING.md) - Testing guide
- [README.md](../README.md) - Quick start

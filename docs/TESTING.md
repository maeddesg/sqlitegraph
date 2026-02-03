# Testing Guide

**Last Updated:** 2026-01-26
**Version:** v1.4.1

This guide covers testing patterns, utilities, and best practices for SQLiteGraph.

---

## Table of Contents

1. [Test Structure](#test-structure)
2. [Test Utilities](#test-utilities)
3. [Running Tests](#running-tests)
4. [Writing Tests](#writing-tests)
5. [Benchmarking](#benchmarking)
6. [Test Patterns](#test-patterns)

---

## Test Structure

SQLiteGraph has a multi-level test structure:

```
sqlitegraph/
├── tests/                      # Integration tests (black-box)
│   ├── helpers/                # Test utilities
│   │   ├── mod.rs
│   │   ├── temp_file.rs        # TempFile abstraction
│   │   ├── v2_helpers.rs       # Native V2 helpers
│   │   └── ...
│   ├── api_ergonomics_tests.rs
│   ├── dsl_tests.rs
│   └── ... (22 files)
│
├── sqlitegraph/tests/          # Unit/module tests
│   ├── algo_tests.rs
│   ├── bfs_tests.rs
│   ├── cache_tests.rs
│   ├── edge_tests.rs
│   ├── entity_tests.rs
│   ├── hnsw_tests.rs
│   ├── mvcc_baseline_tests.rs
│   ├── mvcc_concurrent_tests.rs
│   ├── wal_core_tests.rs
│   ├── wal_recovery_tests.rs
│   └── ... (137 files)
│
└── sqlitegraph/benches/        # Criterion benchmarks
    ├── bfs.rs
    ├── comprehensive_performance.rs
    ├── regression_memory.rs
    ├── regression_write_cost.rs
    └── ... (25 files)
```

### Test Categories

| Category | Location | Purpose |
|----------|----------|---------|
| **Integration** | `/tests/` | Black-box API testing |
| **Unit/Module** | `/sqlitegraph/tests/` | Feature-specific testing |
| **Benchmarks** | `/sqlitegraph/benches/` | Performance regression detection |

---

## Test Utilities

### Creating Test Graphs

**Location:** `tests/helpers/v2_helpers.rs`

```rust
use sqlitegraph::tests::helpers::*;

// Create a simple V2 graph with 2 nodes and 1 edge
let (graph, source_id, target_id, temp_dir) = create_simple_v2_graph()?;

// Create a star graph: center connected to N peripheral nodes
let (graph, center_id, target_ids, temp_dir) = create_star_v2_graph(num_targets)?;

// Create a chain graph: linear chain of N nodes
let (graph, node_ids, temp_dir) = create_chain_v2_graph(length)?;

// Create a tree graph: root with branching factor
let (graph, root_id, temp_dir) = create_tree_v2_graph(depth, branching)?;

// Create a random graph
let (graph, node_ids, temp_dir) = create_random_v2_graph(num_nodes, edge_probability)?;
```

### Adding Nodes and Edges

```rust
// Add a node to a V2 graph
let node_id = add_node_v2(&graph, kind, name, data)?;

// Add an edge between two nodes
let edge_id = add_edge_v2(&graph, from, to, edge_type, data)?;

// Bulk add nodes
let node_ids = add_nodes_v2(&graph, specs)?;
```

### Verification Helpers

```rust
// Verify V2 cluster metadata
verify_v2_cluster_metadata(&graph, expected_count)?;

// Verify node exists
assert_node_exists(&graph, node_id)?;

// Verify edge exists
assert_edge_exists(&graph, edge_id)?;

// Count nodes/edges
let node_count = count_nodes(&graph)?;
let edge_count = count_edges(&graph)?;
```

### Persistence Testing

```rust
// Flush and reopen the graph
let graph = flush_and_reopen(graph, temp_dir)?;

// Verify data persisted after reopen
verify_v2_cluster_metadata(&graph, expected_count)?;
```

### TempFile Abstraction

**Location:** `tests/helpers/temp_file.rs`

```rust
use sqlitegraph::tests::helpers::TempFile;

// Create a temp file that auto-deletes
let temp = TempFile::new()?;

// Get the path
let path = temp.path();

// File is deleted when TempFile goes out of scope
```

---

## Running Tests

### Basic Commands

```bash
# Run all tests
cargo test --workspace

# Run tests with output
cargo test --workspace -- --nocapture

# Run tests with verbose output
cargo test --workspace -- --verbose

# Run a specific test
cargo test test_bfs_traversal

# Run tests in a specific file
cargo test --test bfs_tests

# Run tests with Native V2 feature
cargo test --workspace --features native-v2

# Run tests with debug output
cargo test --workspace --features debug
```

### Running Test Suites

```bash
# Integration tests only
cargo test --test api_ergonomics_tests
cargo test --test dsl_tests

# Algorithm tests
cargo test --test algo_tests

# HNSW tests
cargo test --test hnsw_tests

# MVCC tests
cargo test --test mvcc_baseline_tests
cargo test --test mvcc_concurrent_tests

# WAL tests
cargo test --test wal_core_tests
cargo test --test wal_recovery_tests

# Cache tests
cargo test --test cache_tests
```

### Running with Filters

```bash
# Run tests matching pattern
cargo test bfs

# Run tests excluding pattern
cargo test -- '!skip'

# Run exact test
cargo test 'test_bfs_traversal_single_component'
```

### Parallel vs Sequential

```bash
# Run tests in parallel (default)
cargo test

# Run tests sequentially (for debugging)
cargo test -- --test-threads=1
```

---

## Writing Tests

### Basic Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlitegraph::tests::helpers::*;

    #[test]
    fn test_example() {
        // Setup: Create temp graph
        let (graph, node_id, temp_dir) = create_simple_v2_graph().unwrap();

        // Act: Perform operation
        let result = graph.get_node(node_id);

        // Assert: Verify result
        assert!(result.is_ok());
        let node = result.unwrap();
        assert_eq!(node.id, node_id);
    }
}
```

### Testing with Multiple Backends

```rust
#[test]
fn test_neighbors_sqlite() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let graph = SqliteGraph::open(&path.to_string_lossy()).unwrap();
    // ... test code
}

#[test]
fn test_neighbors_native_v2() {
    let (graph, _, _, temp_dir) = create_simple_v2_graph().unwrap();
    // ... test code
}
```

### Async/Concurrent Testing

```rust
#[test]
fn test_concurrent_reads() {
    let (graph, _, _, _temp_dir) = create_simple_v2_graph().unwrap();
    let graph = Arc::new(graph);

    // Spawn multiple readers
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let g = graph.clone();
            thread::spawn(move || {
                let snapshot = g.snapshot().unwrap();
                // ... read operations
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
}
```

### Error Case Testing

```rust
#[test]
fn test_get_node_not_found() {
    let (graph, _, _, _temp_dir) = create_simple_v2_graph().unwrap();

    let result = graph.get_node(99999);

    assert!(result.is_err());
    match result {
        Err(SqliteGraphError::NotFoundError(msg)) => {
            assert!(msg.contains("99999"));
        }
        _ => panic!("Expected NotFoundError"),
    }
}
```

### Property-Based Testing

```rust
#[test]
fn test_edge_count_property() {
    // Property: edge_count = count of all insertions - deletions
    let (graph, _, _, _temp_dir) = create_simple_v2_graph().unwrap();

    let initial_count = count_edges(&graph).unwrap();

    // Add 5 edges
    for _ in 0..5 {
        add_edge_v2(&graph, 1, 2, "test", json!({})).unwrap();
    }

    // Delete 2 edges
    // ... (deletion logic)

    let final_count = count_edges(&graph).unwrap();
    assert_eq!(final_count, initial_count + 5 - 2);
}
```

---

## Benchmarking

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench bfs

# Run with custom iterations
cargo bench --bench bfs -- --sample-size 100

# Save benchmark results
cargo bench -- --save-baseline main

# Compare with baseline
cargo bench -- --baseline main
```

### Benchmark Structure

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_bfs(c: &mut Criterion) {
    let mut group = c.benchmark_group("bfs");

    for size in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (graph, start_id, _temp_dir) = create_chain_v2_graph(size).unwrap();

            b.iter(|| {
                black_box(algo::bfs(&graph, start_id, 100));
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_bfs);
criterion_main!(benches);
```

### Regression Benchmarks

**Location:** `benches/regression_*.rs`

| Benchmark | Purpose |
|-----------|---------|
| `regression_write_cost.rs` | Detect write performance regressions |
| `regression_memory.rs` | Detect memory usage regressions |
| `regression_non_chain_patterns.rs` | Validate traversal performance |
| `regression_pubsub_*.rs` | Pub/Sub overhead validation |

---

## Test Patterns

### 1. Setup-Act-Assert

```rust
#[test]
fn test_pattern() {
    // SETUP: Create test state
    let (graph, node_id, _temp_dir) = create_simple_v2_graph().unwrap();

    // ACT: Execute operation under test
    let result = graph.get_node(node_id);

    // ASSERT: Verify expected outcome
    assert!(result.is_ok());
}
```

### 2. Test Isolation

```rust
// Each test creates its own temp database
#[test]
fn test_isolated() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let graph = SqliteGraph::open(&path.to_string_lossy()).unwrap();
    // ... test

    // TempDir is deleted when dropped
}
```

### 3. Table-Driven Tests

```rust
#[test]
fn test_multiple_edge_types() {
    let cases = vec![
        ("knows", true),
        ("follows", true),
        ("", false),  // Empty should fail
    ];

    for (edge_type, should_succeed) in cases {
        let (graph, from, to, _temp_dir) = create_simple_v2_graph().unwrap();
        let result = add_edge_v2(&graph, from, to, edge_type, json!({}));

        assert_eq!(result.is_ok(), should_succeed, "Failed for edge_type: {}", edge_type);
    }
}
```

### 4. Regression Test Pattern

```rust
// Phase XX regression test for specific bug
#[test]
fn test_phase_XX_issue_regression() {
    // This test ensures bug from Phase XX doesn't recur

    let (graph, node_id, _temp_dir) = create_simple_v2_graph().unwrap();

    // Previously: This would panic
    // Now: Should return proper error
    let result = graph.delete_node(node_id);
    assert!(result.is_err());  // Expected error due to edges
}
```

### 5. Snapshot Testing

```rust
#[test]
fn test_snapshot_isolation() {
    let (graph, node_id, _temp_dir) = create_simple_v2_graph().unwrap();

    // Create snapshot
    let snapshot = graph.snapshot().unwrap();

    // Modify graph
    update_node_v2(&graph, node_id, "updated_name");

    // Snapshot should not see changes
    let node = snapshot.get_node(node_id).unwrap();
    assert_eq!(node.name, "original_name");

    // Direct graph sees changes
    let node = graph.get_node(node_id).unwrap();
    assert_eq!(node.name, "updated_name");
}
```

---

## Test Naming Conventions

| Pattern | Example | Purpose |
|---------|---------|---------|
| `test_<operation>_<condition>` | `test_get_node_not_found` | Unit test |
| `test_phase_XX_<issue>` | `test_phase_36_corruption_fix` | Regression test |
| `regression_<metric>` | `regression_write_cost` | Benchmark |
| `<feature>_tests.rs` | `cache_tests.rs` | Test file name |

---

## CI Testing

### GitHub Actions

The project uses GitHub Actions for CI. Key workflows:

```yaml
# .github/workflows/test.yml
- Run all tests with --features native-v2
- Run benchmarks with --features bench-ci
- Check documentation with cargo test --doc
```

### Local CI Simulation

```bash
# Run what CI runs
cargo test --workspace --features native-v2
cargo test --workspace --doc
cargo clippy -- --deny warnings
```

---

## Common Testing Issues

### Issue: Temp directory not cleaned up

**Solution:** Use `TempDir` from tempfile crate; it auto-deletes.

### Issue: Tests pass individually but fail together

**Solution:** Tests may be sharing state. Ensure each test creates its own temp database.

### Issue: Flaky concurrent tests

**Solution:** Use proper synchronization or run sequential with `--test-threads=1`.

### Issue: Benchmark too slow

**Solution:** Reduce sample size or warmup iterations in Criterion config.

---

## Coverage

### Checking Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --workspace --features native-v2 --out Html

# Open report
open tarpaulin-report.html
```

### Current Coverage (v1.2.0)

| Module | Coverage |
|--------|----------|
| Graph algorithms | High |
| HNSW | High (134 tests) |
| MVCC | High (65 tests) |
| WAL | High (42 tests) |
| Pub/Sub | High (59 tests) |
| **Total** | **380+ tests** |

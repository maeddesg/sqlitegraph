# Testing Patterns

**Analysis Date:** 2025-02-11

## Test Framework

**Runner:**
- Rust's built-in `cargo test` framework
- `libtest` harness (standard Rust testing)

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ok!` macros
- Custom match assertions in test code

**Run Commands:**
```bash
# Run all tests
cargo test

# Run tests for specific package
cargo test -p sqlitegraph

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_v2_basic_graph_operations

# Run tests in single thread (for concurrency tests)
cargo test -- --test-threads=1

# Run ignored tests
cargo test -- --ignored
```

## Test File Organization

**Location:**
- Primary: `sqlitegraph/tests/` (integration tests)
- Secondary: `tests/` (top-level integration tests)
- Benchmarks: `sqlitegraph/benches/`

**Naming:**
- `<module>_tests.rs` (e.g., `algo_tests.rs`, `mvcc_baseline_tests.rs`)
- `phase<number>_<description>.rs` for phase-specific tests
- `v2_<feature>.rs` for native backend tests
- `regression_<issue>.rs` for regression tests

**Structure:**
```
sqlitegraph/tests/
├── algo_tests.rs                    # Algorithm tests
├── mvcc_baseline_tests.rs           # MVCC system tests
├── mvcc_concurrent_tests.rs         # Concurrent access tests
├── wal_*.rs                       # WAL-related tests
├── v2_*.rs                        # Native backend tests
├── phase*_*.rs                     # Development phase tests
├── regression_*.rs                 # Regression prevention
└── helpers/                        # Test utilities
    └── mod.rs
```

## Test Structure

**Suite Organization:**
```rust
//! Module-level documentation describing test purpose

use sqlitegraph::{...};

// ============================================================================
// TEST HELPERS
// ============================================================================

fn helper_function(...) -> Result<...> {
    ...
}

// ============================================================================
// GROUP 1: TEST CATEGORY
// ============================================================================

#[test]
fn test_specific_behavior() -> Result<(), SqliteGraphError> {
    let graph = setup();
    // Exercise behavior
    // Assert expected outcome
    Ok(())
}
```

**Patterns:**

**Setup Pattern:**
```rust
fn create_test_graph() -> Result<SqliteGraph, SqliteGraphError> {
    let graph = SqliteGraph::open_in_memory()?;
    // Insert test data
    Ok(graph)
}
```

**Teardown Pattern:**
- Implicit via Drop (temp files auto-cleaned)
- Explicit via `drop(graph)` where needed
- TempDir from `tempfile` crate for file cleanup

**Assertion Pattern:**
```rust
// Direct assertion
assert_eq!(result, expected);

// With context message
assert_eq!(snapshot.node_count(), initial_nodes,
    "Snapshot should preserve initial node count");

// Error conversion in tests
.expect("Failed to create graph");
```

## Mocking

**Framework:** No dedicated mocking framework

**Patterns:**
```rust
// In-memory backend for isolation
let graph = SqliteGraph::open_in_memory()?;

// TempDir for filesystem isolation
let temp_dir = TempDir::new().expect("Failed to create temp dir");
let db_path = temp_dir.path().join("test.db");

// Feature-gated backend selection
#[cfg(feature = "native-v2")]
let cfg = GraphConfig::native();
```

**What to Mock:**
- File I/O: use `TempDir` for isolated test files
- Database state: `open_in_memory()` for clean slate
- Time-dependent behavior: not mocked, tested for determinism

**What NOT to Mock:**
- Core graph algorithms (test real implementation)
- SQLite backend behavior (test against real database)
- Error conditions (test real error paths)

## Fixtures and Factories

**Test Data:**
```rust
fn insert_entity(graph: &SqliteGraph, name: &str) -> i64 {
    graph.insert_entity(&GraphEntity {
        id: 0,
        kind: "Node".into(),
        name: name.into(),
        file_path: None,
        data: json!({"name": name}),
    }).expect("insert entity")
}

fn insert_edge(graph: &SqliteGraph, from: i64, to: i64, label: &str) {
    graph.insert_edge(&GraphEdge { ... })
        .expect("insert edge");
}
```

**Location:**
- Inline helpers in test files (not shared across files)
- `helpers/mod.rs` for complex shared fixtures

**Factory Pattern:**
```rust
fn create_test_v2_graph() -> (Box<dyn GraphBackend>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_v2_graph.db");
    let cfg = GraphConfig::new(BackendKind::Native);
    let graph = open_graph(&db_path, &cfg).expect("Failed to create V2 graph");
    (graph, temp_dir)  // Returns both graph and cleanup handle
}
```

## Coverage

**Requirements:** No enforced coverage target (as of 2025-02-11)

**View Coverage:**
```bash
# Install tarpaulin for coverage
cargo install cargo-tarpaulin

# Run with coverage
cargo tarpaulin --out Html

# Or use LLVM coverage
RUSTFLAGS="-C instrument-coverage" cargo test
grcov .lcov --output-path lcov.info
```

**Coverage Areas:**
- Core graph operations: Well covered
- MVCC system: Comprehensive tests in `mvcc_*.rs`
- Native V2 backend: Good coverage in `v2_*.rs` tests
- Edge cases: Regression tests cover historical bugs

## Test Types

**Unit Tests:**
- Scope: Single module or function behavior
- Approach: `open_in_memory()` for isolation, direct function calls
- Location: `sqlitegraph/tests/` files

**Integration Tests:**
- Scope: Multiple components working together
- Approach: Full graph lifecycle (open, operate, close, reopen)
- Location: `tests/` directory at project root

**Regression Tests:**
- Scope: Prevent recurrence of fixed bugs
- Naming: `regression_<issue>.rs` or `v2_<bug>_regression.rs`
- Pattern: Reproduce bug conditions, verify fix

**Stress Tests:**
- Scope: Large-scale data handling
- Pattern: Insert 1000+ nodes/edges, verify performance
- Example: `test_pagerank_large_graph()`, `test_label_prop_large_graph()`

## Common Patterns

**Async Testing:**
- Not applicable (project is synchronous)

**Error Testing:**
```rust
#[test]
fn test_error_case() {
    let graph = SqliteGraph::open_in_memory().unwrap();
    let result = graph.operation_that_fails();
    assert!(result.is_err());
    assert!(matches!(result, Err(SqliteGraphError::NotFound(_)));
}
```

**Snapshot Isolation Testing:**
```rust
#[test]
fn test_snapshot_isolation_single_threaded() -> Result<(), SqliteGraphError> {
    let graph = create_test_graph()?;

    // Warm cache to populate adjacency data
    warm_cache(&graph)?;

    let initial_nodes = node_count(&graph)?;

    // Acquire snapshot
    let snapshot = graph.acquire_snapshot()?;

    // Modify graph after snapshot
    add_more_data(&graph)?;

    // Verify snapshot unchanged (isolation)
    assert_eq!(snapshot.node_count() as i64, initial_nodes);

    Ok(())
}
```

**Deterministic Behavior Testing:**
```rust
#[test]
fn test_repeatable_snapshot_results() -> Result<(), SqliteGraphError> {
    let graph = create_test_graph()?;

    let snapshot1 = graph.acquire_snapshot()?;
    let snapshot2 = graph.acquire_snapshot()?;

    // Verify they have identical content
    assert_eq!(snapshot1.node_count(), snapshot2.node_count());
    assert_eq!(snapshot1.edge_count(), snapshot2.edge_count());

    Ok(())
}
```

**Reopen Persistence Testing:**
```rust
#[test]
fn test_v2_reopen_invariants() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Phase 1: Create and insert data
    {
        let graph = open_graph(&db_path, &cfg)?;
        // ... insert data ...
    }  // Explicit drop closes graph

    // Phase 2: Reopen and verify
    {
        let graph = open_graph(&db_path, &cfg)?;
        // ... verify data persisted ...
    }
}
```

**WAL Recovery Testing:**
```rust
// Tests verify WAL is properly applied on reopen
// Pattern: Write data -> Explicit drop -> Reopen -> Verify
```

## Benchmark Testing

**Framework:** Criterion (`criterion` crate)

**Run Commands:**
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench comprehensive_performance

# Save baseline for regression detection
cargo bench --bench comprehensive_performance -- --save-baseline main

# Compare against baseline
cargo bench --bench comprehensive_performance -- --baseline main
```

**Benchmark Structure:**
```rust
use criterion::{...};

fn bench_operation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("group_name");
    group.sample_size(100);
    group.warm_up_time(Duration::from_secs(5));
    group.measurement_time(Duration::from_secs(15));

    for &size in &[1, 10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, &n| {
                b.iter_batched(
                    || { /* setup */ },
                    |data| { /* measure */ },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_operation);
criterion_main!(benches);
```

**Benchmark Files:**
- Location: `sqlitegraph/benches/`
- Naming: `<operation>.rs` (e.g., `bfs.rs`, `k_hop.rs`, `hnsw.rs`)
- Config: `harness = false` in `Cargo.toml`

## Test Organization by Category

**Algorithm Tests (`algo_tests.rs`):**
- PageRank, betweenness centrality
- Label propagation, Louvain communities
- Connected components, cycle detection
- Large graph stress tests

**MVCC Tests (`mvcc_*.rs`):**
- `mvcc_baseline_tests.rs`: Single-threaded baseline
- `mvcc_concurrent_tests.rs`: Multi-threaded access
- `mvcc_cache_isolation_tests.rs`: Cache behavior
- `mvcc_snapshot_tests.rs`: Snapshot lifecycle

**Native Backend Tests (`v2_*.rs`):**
- `v2_graph_ops_smoke.rs`: Basic CRUD operations
- `v2_performance.rs`: Performance validation
- `v2_stress_integrity.rs`: Large-scale integrity checks
- `v2_read_after_reopen_regression.rs`: Persistence bugs

**WAL Tests (`wal_*.rs`):**
- `wal_core_tests.rs`: Core WAL functionality
- `wal_recovery_tests.rs`: Recovery after crash
- `wal_reader_tests.rs`: WAL reading logic

**Regression Tests (`regression_*.rs`, `*_regression*.rs`):**
- Named after specific bugs fixed
- Prevent recurrence of known issues
- Often include commit or phase reference in name

## Test Data Management

**Temp Files:**
```rust
// Per-test temp directory
let temp_dir = TempDir::new()?;

// Scoped cleanup
{
    let graph = open_graph(path, &config)?;
    // ... test ...
}
```

**In-Memory Testing:**
```rust
// Fastest for unit tests
let graph = SqliteGraph::open_in_memory()?;
```

**Database State Cleanup:**
- Auto-drop on scope exit
- Explicit drop where WAL timing matters
- TempDir auto-deletes on drop

## CI/CD Test Considerations

**Parallel Execution:**
- Tests run in parallel by default
- Single-threaded mode for concurrent tests: `--test-threads=1`

**Feature Variants:**
- Test both `sqlite-backend` (default) and `native-v2` features
- Feature-gated tests: `#[cfg(feature = "native-v2")]`

**Performance Gates:**
- Benchmark comparisons in CI detect regressions
- `REGRESSION_THRESHOLD` constant in benchmark files (typically 10%)

---

*Testing analysis: 2025-02-11*

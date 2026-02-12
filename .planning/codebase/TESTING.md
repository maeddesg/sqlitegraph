# Testing Patterns

**Analysis Date:** 2025-02-12

## Test Framework

**Runner:**
- Rust's built-in `cargo test` framework
- `libtest` harness (standard Rust testing)
- Criterion for benchmarks (`criterion` crate)

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ok!` macros
- Custom match assertions in test code
- `matches!()` macro for pattern matching assertions

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

# Run tests with specific feature
cargo test --features native-v2

# Run all tests except integration
cargo test --lib
```

## Test File Organization

**Location:**
- Primary: `tests/` (top-level integration tests)
- Secondary: `sqlitegraph/tests/` (integration tests within crate)
- Benchmarks: `sqlitegraph/benches/`
- Algorithm tests: `sqlitegraph/src/algo/tests.rs`

**Naming:**
- `<module>_tests.rs` (e.g., `algo_tests.rs`, `mvcc_baseline_tests.rs`)
- `<feature>_<invariant>.rs` for invariant tests (e.g., `snapshot_invariants_tests.rs`)
- `v2_<feature>.rs` for native backend tests
- `regression_<issue>.rs` or `<issue>_regression.rs` for regression tests
- `phase<number>_*.rs` for phase-specific development tests

**Structure:**
```
tests/
├── api_ergonomics_tests.rs           # API design validation
├── snapshot_invariants_tests.rs        # Snapshot system invariants (TDD)
├── snapshot_integration_tests.rs         # Snapshot export/import tests
├── header_architecture_regression_tests.rs
├── phase39_mmap_corruption_detection_tests.rs
├── v2_native_bfs_regression_tests.rs
├── v2_clustered_adjacency_tdd_tests.rs
├── v2_edge_cluster_serialization_binrw_tests.rs
├── v2_full_roundtrip_integration_tests.rs
├── v2_layout_invariant_tests.rs
├── v2_mmap_io_invariants_tests.rs
├── production_adjacency_load_test.rs  # Production load testing
├── kv_rollback_test.rs                # KV store rollback tests
└── minimal_reproduction_test.rs
```

## Test Structure

**Suite Organization:**
```rust
//! Module-level documentation describing test purpose
//!
//! These tests enforce critical invariants...

use sqlitegraph::{...};

// ============================================================================
// TEST HELPERS
// ============================================================================

fn helper_function(...) -> Result<...> {
    ...
}

// ============================================================================
// EXPORT INVARIANTS TESTS
// ============================================================================

#[test]
fn test_specific_behavior() {
    // Arrange
    let (graph, temp_dir) = setup();

    // Act
    let result = graph.operation();

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap().count, expected);
}
```

**Patterns:**

**Setup Pattern:**
```rust
fn setup_test_graph() -> (GraphFile, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let graph_file = GraphFile::create(&db_path).expect("Failed to create graph");
    (graph_file, temp_dir)  // Returns both graph and cleanup handle
}
```

**Teardown Pattern:**
- Implicit via Drop (temp files auto-cleaned)
- Explicit via `drop(graph)` where needed
- TempDir from `tempfile` crate for file cleanup
- Scope-based cleanup with `{ }` blocks

**Assertion Pattern:**
```rust
// Direct assertion
assert_eq!(result, expected);

// With context message
assert_eq!(snapshot.node_count(), initial_nodes,
    "Snapshot should preserve initial node count");

// Multiple assertions with context
assert!(
    edge_region_start >= node_region_end,
    "Edge region overlaps node region: edge_start={}, node_end={}",
    edge_region_start, node_region_end
);

// Error conversion in tests
.expect("Failed to create graph");
```

**TDD Pattern (from snapshot_invariants_tests.rs):**
```rust
/// **INVARIANT 1**: Export never writes directly to final filenames
#[test]
fn test_export_never_writes_directly_to_final_filenames() {
    // This test should FAIL initially - creates failing TDD test
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Test invariant explicitly
    assert!(!final_path.exists(), "Path should not exist before operation");

    // Perform operation
    let result = operation();

    // Assert expected behavior
    assert!(result.is_ok(), "Operation should succeed: {:?}", result);
}
```

## Mocking

**Framework:** No dedicated mocking framework

**Patterns:**
```rust
// In-memory backend for isolation
let backend = SqliteGraphBackend::in_memory().expect("backend");

// TempDir for filesystem isolation
let temp_dir = TempDir::new().expect("Failed to create temp dir");
let db_path = temp_dir.path().join("test.db");

// Feature-gated backend selection
#[cfg(feature = "native-v2")]
let cfg = GraphConfig::native();

// Direct struct instantiation for low-level tests
let mut node_store = NodeStore::new(&mut graph_file);
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
// Production test data generator pattern
struct ProductionTestData {
    graph_file: GraphFile,
    node_count: usize,
    edge_count: usize,
    expected_iterations: HashMap<NativeNodeId, u32>,
}

impl ProductionTestData {
    fn create_production_dataset(seed: u64, num_nodes: usize, edges_per_node: u32) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        // ... create test data ...
        Self { graph_file, node_count, edge_count, expected_iterations }
    }
}
```

**Location:**
- Inline helpers in test files (not shared across files)
- Private functions at top of test files
- Helper structs for complex test data

**Factory Pattern:**
```rust
fn create_test_v2_graph() -> (GraphFile, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_v2_graph.db");
    let graph_file = GraphFile::create(&db_path).expect("Failed to create graph");
    (graph_file, temp_dir)  // Returns both graph and cleanup handle
}
```

**Build Helper Pattern:**
```rust
// From v2_native_bfs_regression_tests.rs
fn build_v2_node(id: i64, kind: &str, name: &str, edge_count: u32) -> NodeRecordV2 {
    let mut node = NodeRecordV2::new(id, kind.to_string(), name.to_string(), json!({"payload": id}));
    node.set_outgoing_cluster(2048, 512, edge_count);
    node.set_incoming_cluster(4096, 256, edge_count / 2);
    node
}
```

## Coverage

**Requirements:** No enforced coverage target (as of 2025-02-12)

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
- HNSW vector search: Tests in `sqlitegraph/src/hnsw/` and benches

## Test Types

**Unit Tests:**
- Scope: Single module or function behavior
- Approach: `open_in_memory()` for isolation, direct function calls
- Location: `sqlitegraph/tests/` files and inline `#[cfg(test)]` modules
- Result types: Some tests return `Result<(), E>` for cleaner error propagation

**Integration Tests:**
- Scope: Multiple components working together
- Approach: Full graph lifecycle (open, operate, close, reopen)
- Location: `tests/` directory at project root
- Examples: `v2_full_roundtrip_integration_tests.rs`, `snapshot_integration_tests.rs`

**Regression Tests:**
- Scope: Prevent recurrence of fixed bugs
- Naming: `regression_<issue>.rs` or `v2_<bug>_regression.rs`
- Pattern: Reproduce bug conditions, verify fix
- Examples: `header_architecture_regression_tests.rs`, `v2_native_bfs_regression_tests.rs`

**Invariant Tests:**
- Scope: Enforce critical system invariants
- Naming: `<module>_invariant_tests.rs`
- Pattern: TDD approach - write failing test first, then implement
- Examples: `snapshot_invariants_tests.rs`, `v2_layout_invariant_tests.rs`
- From snapshot_invariants_tests.rs:
  - Export invariants (atomicity, cleanup, naming)
  - Import invariants (validation, rejection, no overwrite without permission)
  - Crash recovery invariants (deterministic recovery)
  - Atomic operations invariants (all-or-nothing)

**Load/Stress Tests:**
- Scope: Large-scale data handling
- Pattern: Insert 1000+ nodes/edges, verify performance
- Examples:
  - `production_adjacency_load_test.rs` - Production-scale adjacency testing
  - `test_pagerank_large_graph()`, `test_label_prop_large_graph()` in algo tests
- Performance assertions: `assert!(elapsed < Duration::from_millis(100))`

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
    assert!(matches!(result, Err(SqliteGraphError::NotFound(_))));
}

// Test specific error variant
#[test]
fn test_import_rejects_directories_as_snapshot_targets() {
    let result = SnapshotImporter::from_export_dir(&export_dir, &target_path, config);
    assert!(result.is_err(), "Import should reject directory as snapshot target");

    match result {
        Err(NativeBackendError::InvalidParameter { context, .. }) => {
            assert!(context.contains("directory") || context.contains("file"));
        }
        Err(other) => panic!("Expected InvalidParameter, got: {:?}", other),
        Ok(_) => panic!("Import should not succeed when snapshot is a directory"),
    }
}
```

**Snapshot Isolation Testing:**
```rust
#[test]
fn test_snapshot_isolation() -> Result<(), SqliteGraphError> {
    let graph = create_test_graph()?;

    // Warm cache to populate adjacency data
    warm_cache(&graph)?;

    let initial_nodes = node_count(&graph)?;

    // Acquire snapshot
    let snapshot = graph.snapshot()?;

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
fn test_repeatable_results() {
    let graph = create_test_graph()?;

    let result1 = graph.operation()?;
    let result2 = graph.operation()?;

    // Verify they have identical content
    assert_eq!(result1, result2);
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
#[test]
fn test_kv_rollback_after_crash() {
    // Write data
    // Simulate crash
    // Verify recovery or rollback
}
```

**Invariant Testing Pattern (from snapshot_invariants_tests.rs):**
```rust
// ============================================================================
// ATOMIC OPERATIONS INVARIANTS TESTS
// ============================================================================

#[cfg(test)]
mod atomic_operations_invariants {
    use super::*;

    /// **INVARIANT 1**: Atomic operations reject directory sources
    #[test]
    fn test_atomic_operations_reject_directory_sources() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a directory as "source"
        let source_dir = temp_dir.path().join("source_dir");
        fs::create_dir(&source_dir).expect("Failed to create source directory");

        let target_file = temp_dir.path().join("target.txt");

        let atomic_ops = AtomicFileOperations::new();
        let result = atomic_ops.atomic_copy_file(&source_dir, &target_file);

        assert!(result.is_err(), "Atomic operations should reject directory sources");
    }

    /// **INVARIANT 3**: Atomic operations provide all-or-nothing semantics
    #[test]
    fn test_atomic_operations_provide_all_or_nothing_semantics() {
        // Verify complete copy or no partial state
        assert!(target_file.exists(), "Target file should exist after atomic copy");

        let source_content = fs::read_to_string(&source_file).expect("Failed to read source");
        let target_content = fs::read_to_string(&target_file).expect("Failed to read target");
        assert_eq!(source_content, target_content, "Content should be identical");
    }
}
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

# Run with specific features
cargo bench --features native-v2
```

**Benchmark Structure:**
```rust
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

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
- Config: `harness = false` in `Cargo.toml` [[bench]] sections
- Examples from `Cargo.toml`:
  - `bfs.rs` - BFS algorithm benchmarks
  - `k_hop.rs` - K-hop traversal benchmarks
  - `insert.rs` - Insert performance benchmarks
  - `hnsw.rs` - HNSW vector search benchmarks
  - `comprehensive_performance.rs` - Comprehensive performance tests
  - `wal_recovery_benchmarks.rs` - WAL recovery performance

## Test Organization by Category

**Algorithm Tests (`algo_tests.rs`, `sqlitegraph/src/algo/tests.rs`):**
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
- `v2_layout_invariant_tests.rs`: Layout and offset validation
- `v2_mmap_io_invariants_tests.rs`: Memory-mapped I/O tests
- `v2_native_bfs_regression_tests.rs`: BFS regression tests
- `v2_edge_cluster_serialization_binrw_tests.rs`: Edge serialization
- `v2_node_serialization_binrw_tests.rs`: Node serialization

**WAL Tests (`wal_*.rs`):**
- WAL functionality tests (within native backend)
- Recovery after crash
- WAL reading logic

**Snapshot Tests:**
- `snapshot_invariants_tests.rs`: Formal invariants for export/import
- `snapshot_integration_tests.rs`: Full export/import cycles

**Regression Tests:**
- Named after specific bugs fixed
- Prevent recurrence of known issues
- Often include commit or phase reference in name

**API Tests:**
- `api_ergonomics_tests.rs`: API design validation
- Tests high-level API usability

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

// Test file with helper
fn setup_test_graph() -> (GraphFile, NamedTempFile) {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let graph_file = GraphFile::create(temp_file.path()).expect("Failed to create graph");
    (graph_file, temp_file)
}
```

**In-Memory Testing:**
```rust
// Fastest for unit tests
let backend = SqliteGraphBackend::in_memory()?;
let client = BackendClient::new(backend);
```

**Database State Cleanup:**
- Auto-drop on scope exit
- Explicit drop where WAL timing matters
- TempDir auto-deletes on drop

## CI/CD Test Considerations

**Parallel Execution:**
- Tests run in parallel by default
- Single-threaded mode for concurrent tests: `--test-threads=1`
- Load tests use larger datasets to avoid race conditions

**Feature Variants:**
- Test both `sqlite-backend` (default) and `native-v2` features
- Feature-gated tests: `#[cfg(feature = "native-v2")]`
- Some tests require `v2_experimental` feature alias

**Performance Gates:**
- Benchmark comparisons in CI detect regressions
- Load tests have explicit performance assertions
- Regression threshold typically 10-20% in benchmarks

## Test Naming Conventions

**Function Names:**
- `test_<feature>_<scenario>` for specific tests
- `test_<feature>_<variant>` for related tests
- Descriptive names preferred over brief ones
- Invariant tests: `test_<invariant_description>()`

**Module Organization in Tests:**
```rust
#[cfg(test)]
mod export_invariants {
    use super::*;
    // Export-related invariant tests
}

#[cfg(test)]
mod import_invariants {
    use super::*;
    // Import-related invariant tests
}

#[cfg(test)]
mod crash_recovery_invariants {
    use super::*;
    // Recovery-related invariant tests
}
```

---

*Testing analysis: 2025-02-12*

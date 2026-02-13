# Testing Patterns

**Analysis Date:** 2025-02-13

## Test Framework

**Runner:**
- Rust's built-in `cargo test` framework (libtest harness)
- Criterion for benchmarks (`criterion` crate)
- No external test runners

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ne!`, `assert_matches!` macros
- `matches!()` macro for pattern matching assertions
- Context messages in assertions: `assert_eq!(actual, expected, "context: {}", extra)`

**Run Commands:**
```bash
# Run all tests
cargo test

# Run tests for specific package
cargo test -p sqlitegraph

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_btree_lookup_single_node

# Run tests in single thread (for concurrency tests)
cargo test -- --test-threads=1

# Run ignored tests
cargo test -- --ignored

# Run tests with specific feature
cargo test --features native-v3
cargo test --features native-v2

# Run all tests except integration
cargo test --lib
```

## Test File Organization

**Location:**
- Primary: `sqlitegraph/tests/` (integration tests - 147 files)
- Secondary: `sqlitegraph/src/backend/native/v3/*/tests.rs` (inline unit tests)
- Tertiary: `sqlitegraph/src/algo/tests.rs` (algorithm tests)
- Benchmarks: `sqlitegraph/benches/` (30 benchmark files)

**Naming:**
- `<module>_tests.rs` (e.g., `wal_tests.rs`, `kv_store_tests.rs`)
- `<feature>_<invariant>.rs` for invariant tests (e.g., `snapshot_invariants_tests.rs`)
- `v3_<feature>.rs` for V3 backend tests
- `regression_<issue>.rs` or `<issue>_regression.rs` for regression tests
- `v2_<feature>_regression.rs` for V2 regression tests
- `phase<number>_*.rs` for phase-specific development tests

**V3 Test Structure:**
```
sqlitegraph/src/backend/native/v3/
├── constants.rs          (inline tests: 8 test functions)
├── header.rs             (inline tests: 8 test functions)
├── node/
│   └── tests.rs         (773 lines, 100+ test functions)
├── allocator.rs          (no inline tests - use separate file)
├── btree.rs            (no inline tests - use separate file)
├── wal.rs              (no inline tests - use separate file)
└── backend.rs          (no inline tests - use separate file)
```

**Top-Level Integration Tests:**
```
sqlitegraph/tests/
├── acid_regression_test.rs              # ACID property tests
├── acid_snapshot_test.rs               # Snapshot isolation tests
├── algo_tests.rs                      # Graph algorithm validation
├── wal_core_tests.rs                  # WAL core functionality
├── wal_reader_tests.rs                # WAL reading logic
├── wal_writer_tests.rs                # WAL writing logic
├── v2_stress_integrity.rs            # V2 stress tests (use pattern for V3)
├── snapshot_invariants_tests.rs        # Snapshot invariants (TDD)
└── [143 more test files...]
```

## Test Structure

**Suite Organization:**
```rust
//! Module-level documentation describing test purpose
//!
//! These tests enforce critical invariants...
//! Test utilities:
//! - Test helpers for X creation
//! - Integration-style tests for end-to-end flows

use sqlitegraph::backend::native::v3::...;

// ============================================================================
// TEST HELPERS
// ============================================================================

fn helper_function(...) -> Result<...> {
    ...
}

// ============================================================================
// SPECIFIC CATEGORY TESTS
// ============================================================================

#[test]
fn test_specific_behavior() {
    // Arrange
    let (store, temp_file) = setup();

    // Act
    let result = store.operation();

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap().count, expected);
}
```

**V3 Test Module Pattern (from `sqlitegraph/src/backend/native/v3/node/tests.rs`):**
```rust
//! Comprehensive unit tests for NodeStore V3 components
//!
//! This module provides test coverage for:
//! - B+Tree lookup operations
//! - NodePage loading and decompression
//! - TraversalCache behavior (via store module)
//! - Error handling

use std::sync::Arc;
use crate::backend::native::v3::node::record::NodeRecordV3;
use crate::backend::native::v3::node::page::NodePage;
use crate::backend::native::v3::node::store::TraversalCache;
use crate::backend::native::v3::index::IndexPage;

/// Test node ID range for scalability tests
pub const TEST_NODE_COUNT: usize = 100;

/// Page capacity for test fixtures
pub const TEST_PAGE_CAPACITY: usize = 20;

// ============================================================================
// B+Tree Lookup Tests
// ============================================================================

#[test]
fn test_btree_lookup_single_node() {
    let index_page = IndexPage::new_leaf(1);
    assert_eq!(index_page.page_id(), 1);
    assert!(matches!(index_page.page_type(), IndexPageType::Leaf));
}
```

**Patterns:**

**Setup Pattern:**
```rust
fn setup_test_graph() -> (GraphFile, TempDir) {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path();
    let graph_file = GraphFile::create(path).unwrap();
    (graph_file, temp_file)  // Returns both graph and cleanup handle
}
```

**Teardown Pattern:**
- Implicit via Drop (temp files auto-cleaned by `tempfile` crate)
- Explicit via `drop(graph)` where WAL timing matters
- Scope-based cleanup with `{ }` blocks
- `tempfile::NamedTempFile` for file cleanup

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

**TDD Pattern (from `snapshot_invariants_tests.rs`):**
```rust
/// **INVARIANT 1**: Export never writes directly to final filenames
#[test]
fn test_export_never_writes_directly_to_final_filenames() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    assert!(!final_path.exists(), "Path should not exist before operation");

    let result = operation();

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
let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");

// Feature-gated backend selection
#[cfg(feature = "native-v3")]
let cfg = GraphConfig::native_v3();

// Direct struct instantiation for low-level tests
let store = NodeStore::new(&allocator);

// Mock GraphFile for testing (pattern from sequential_cluster_reader.rs)
struct MockGraphFile {
    data: Vec<u8>,
}

impl MockGraphFile {
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Mock read_bytes that reads from the in-memory data
    fn read_bytes(&self, offset: u64, size: u64) -> NativeResult<Vec<u8>> {
        let start = offset as usize;
        let end = start + size as usize;
        if end > self.data.len() {
            return Err(NativeBackendError::Io(
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Read past end of mock data")
            ));
        }
        Ok(self.data[start..end].to_vec())
    }
}
```

**What to Mock:**
- File I/O: use `tempfile::NamedTempFile` for isolated test files
- Database state: `open_in_memory()` for clean slate
- Page storage: Mock implementations for B+Tree testing

**What NOT to Mock:**
- Core graph algorithms (test real implementation)
- B+Tree operations (test real B+Tree)
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
        let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        // ... create test data ...
        Self { graph_file, node_count, edge_count, expected_iterations }
    }
}
```

**Location:**
- Inline helpers in test files (not shared across files)
- Private functions at top of test files
- Helper structs for complex test data
- Constants at module level: `TEST_NODE_COUNT`, `TEST_PAGE_CAPACITY`

**Factory Pattern:**
```rust
// From sqlitegraph/src/backend/native/v3/node/tests.rs
pub fn create_test_node(id: i64, name: &str) -> NodeRecordV3 {
    NodeRecordV3 {
        id,
        name: name.to_string(),
        node_type: "TEST".to_string(),
        flags: NodeFlags(0),
        // ... other fields
    }
}

// Helper for test page creation
pub fn create_test_page(page_id: u64) -> NodePage {
    let mut page = NodePage::new(page_id);
    // ... populate page ...
    page
}
```

**Build Helper Pattern:**
```rust
// From v2_native_bfs_regression_tests.rs (apply to V3)
fn build_v3_node(id: i64, kind: &str, name: &str) -> NodeRecordV3 {
    let mut node = NodeRecordV3::new(id, kind.to_string(), name.to_string(), json!({"payload": id}));
    // ... set additional fields ...
    node
}
```

## Coverage

**Requirements:** No enforced coverage target (as of 2025-02-13)

**View Coverage:**
```bash
# Install tarpaulin for coverage (not currently configured)
cargo install cargo-tarpaulin

# Run with coverage
cargo tarpaulin --out Html

# Or use LLVM coverage (not currently configured)
RUSTFLAGS="-C instrument-coverage" cargo test
grcov .lcov --output-path lcov.info
```

**Coverage Areas:**
- V3 constants: Good coverage (8 inline tests)
- V3 header: Good coverage (8 inline tests)
- V3 node module: Good coverage (773-line test file)
- V3 allocator: Needs test file (currently no inline tests)
- V3 btree: Needs test file (currently no inline tests)
- V3 wal: Needs test file (currently no inline tests)
- V3 backend: Needs integration tests (currently no inline tests)

## Test Types

**Unit Tests:**
- Scope: Single module or function behavior
- Approach: Direct function calls, isolated state
- Location: `sqlitegraph/src/backend/native/v3/*/tests.rs` and inline `#[cfg(test)]` modules
- Result types: Tests return `Result<(), E>` for cleaner error propagation

**Integration Tests:**
- Scope: Multiple V3 components working together
- Approach: Full backend lifecycle (open, operate, close, reopen)
- Location: `sqlitegraph/tests/` directory
- Examples: `v2_full_roundtrip_integration_tests.rs` (use as pattern for V3)

**Regression Tests:**
- Scope: Prevent recurrence of fixed bugs
- Naming: `regression_<issue>.rs` or `v3_<bug>_regression.rs`
- Pattern: Reproduce bug conditions, verify fix
- Examples: `header_architecture_regression_tests.rs`, `v2_native_bfs_regression_tests.rs`

**Invariant Tests:**
- Scope: Enforce critical system invariants
- Naming: `<module>_invariant_tests.rs`
- Pattern: TDD approach - write failing test first, then implement
- Examples: `snapshot_invariants_tests.rs`, `v2_layout_invariant_tests.rs`
- V3 Invariants to test:
  - B+Tree consistency (parent-child pointers, key ordering)
  - Page allocator (no double-free, no leaks)
  - WAL recovery (deterministic recovery)

**Load/Stress Tests:**
- Scope: Large-scale data handling
- Pattern: Insert 1000+ nodes/edges, verify performance
- Examples:
  - `production_adjacency_load_test.rs` - Production-scale adjacency testing
  - `test_pagerank_large_graph()`, `test_label_prop_large_graph()` in algo tests
- Performance assertions: `assert!(elapsed < Duration::from_millis(100))`
- V3 Stress Test Pattern (from `v2_stress_integrity.rs`):
```rust
#[test]
#[ignore] // Only run when explicitly enabled
fn v3_stress_integrity_test() {
    if !should_run_stress_tests() {
        println!("Skipping V3 stress test (set RUST_TEST_STRESS=1 to enable)");
        return;
    }

    let config = StressTestConfig {
        node_count: 50_000,
        page_count: 10_000,  // V3 uses pages
        validation_interval: 25_000,
        timeout_secs: 300,
    };

    let result = run_stress_test(&config);

    assert!(!result.corruption_detected);
    assert_eq!(result.validations_passed, result.validation_checks);
}
```

## Common Patterns

**Async Testing:**
- Not applicable for V3 core (synchronous I/O)
- V3 WAL scanner has `async fn` but tests are sync (use `block_on` or `poll`)

**Error Testing:**
```rust
#[test]
fn test_error_case() {
    let store = NodeStore::new(&allocator);
    let result = store.get_node(999);  // Non-existent node
    assert!(result.is_err());
    assert!(matches!(result, Err(NativeBackendError::InvalidNodeId { .. }));
}

// Test specific error variant
#[test]
fn test_allocator_double_free() {
    let mut allocator = create_test_allocator();
    let page_id = allocator.allocate().unwrap();

    allocator.free(page_id).unwrap();

    let result = allocator.free(page_id);
    assert!(result.is_err());

    match result {
        Err(NativeBackendError::DoubleFree { page_id, .. }) => {
            assert_eq!(page_id, page_id);
        }
        Err(other) => panic!("Expected DoubleFree, got: {:?}", other),
        Ok(_) => panic!("Double free should not succeed"),
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
fn test_btree_deterministic_lookup() {
    let mut tree = create_test_btree();
    insert_test_data(&mut tree);

    let result1 = tree.lookup(42);
    let result2 = tree.lookup(42);

    // Verify they have identical results
    assert_eq!(result1, result2);
}
```

**Reopen Persistence Testing:**
```rust
#[test]
fn test_v3_reopen_persistence() {
    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    let db_path = temp_file.path();

    // Phase 1: Create and insert data
    {
        let backend = V3Backend::create(db_path)?;
        backend.insert_node(NodeSpec { ... })?;
    }  // Explicit drop closes backend

    // Phase 2: Reopen and verify
    {
        let backend = V3Backend::open(db_path)?;
        let node = backend.get_node(node_id)?;
        assert_eq!(node.name, "test_node");
    }
}
```

**WAL Recovery Testing:**
```rust
// Tests verify WAL is properly applied on reopen
// Pattern: Write data -> Explicit drop -> Reopen -> Verify
#[test]
fn test_wal_recovery_after_crash() {
    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    let db_path = temp_file.path();

    // Write data without explicit checkpoint
    {
        let backend = V3Backend::create(db_path)?;
        backend.insert_node(NodeSpec { ... })?;
    }  // WAL not flushed

    // Simulate crash recovery
    {
        let backend = V3Backend::open(db_path)?;
        // Verify data recovered from WAL
        assert!(backend.get_node(node_id).is_ok());
    }
}
```

**Page Allocator Testing Pattern:**
```rust
#[test]
fn test_page_allocation_cycle() {
    let mut allocator = create_test_allocator();

    // Allocate
    let page1 = allocator.allocate()?;
    let page2 = allocator.allocate()?;
    assert_ne!(page1, page2);

    // Free
    allocator.free(page1)?;

    // Reuse
    let page3 = allocator.allocate()?;
    assert_eq!(page3, page1);  // Should reuse freed page
}

#[test]
fn test_double_free_prevention() {
    let mut allocator = create_test_allocator();
    let page_id = allocator.allocate()?;

    allocator.free(page_id)?;

    let result = allocator.free(page_id);
    assert!(matches!(result, Err(NativeBackendError::DoubleFree { .. })));
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
cargo bench --features native-v3
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

**V3 Benchmark Needs:**
- B+Tree lookup benchmarks
- Page allocation benchmarks
- WAL write throughput benchmarks
- Compression (delta/varint) benchmarks

## Test Organization by Category

**V3 Unit Tests (`sqlitegraph/src/backend/native/v3/`):**
- `constants/tests`: Magic bytes, versioning, checksums
- `header/tests`: Header validation, persistence
- `node/tests`: B+Tree lookup, page loading, decompression

**V3 Integration Tests (need to create):**
- `sqlitegraph/tests/v3_btree_integration_tests.rs` - B+Tree operations
- `sqlitegraph/tests/v3_allocator_tests.rs` - Page allocation
- `sqlitegraph/tests/v3_wal_tests.rs` - WAL recovery
- `sqlitegraph/tests/v3_backend_tests.rs` - Full backend lifecycle

**Algorithm Tests (`algo_tests.rs`, `sqlitegraph/src/algo/tests.rs`):**
- PageRank, betweenness centrality
- Label propagation, Louvain communities
- Connected components, cycle detection
- Large graph stress tests

**WAL Tests (`wal_*.rs`):**
- `wal_core_tests.rs`: WAL core functionality (V2 pattern for V3)
- `wal_reader_tests.rs`: WAL reading logic
- `wal_writer_tests.rs`: WAL writing logic
- `wal_recovery_edge_cases.rs`: Recovery scenarios
- `wal_checkpoint_recovery_tests.rs`: Checkpoint integration

**Snapshot Tests:**
- `snapshot_invariants_tests.rs`: Formal invariants for export/import
- `snapshot_integration_tests.rs`: Full export/import cycles

**Regression Tests:**
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
- Test both `sqlite-backend` (default) and `native-v3` features
- Feature-gated tests: `#[cfg(feature = "native-v3")]`
- V3 is experimental: tests may be ignored by default

**Performance Gates:**
- Benchmark comparisons in CI detect regressions
- Load tests have explicit performance assertions
- Regression threshold typically 10-20% in benchmarks

**Stress Test Control:**
- Long-running tests tagged `#[ignore]`
- Enable with environment variable: `RUST_TEST_STRESS=1`
- Pattern from `v2_stress_integrity.rs`:
```rust
fn should_run_stress_tests() -> bool {
    env::var("RUST_TEST_STRESS").is_ok() || env::var("STRESS_TESTS").is_ok()
}

#[test]
#[ignore]
fn v3_stress_test() {
    if !should_run_stress_tests() {
        println!("Skipping V3 stress test (set RUST_TEST_STRESS=1 to enable)");
        return;
    }
    // ... stress test ...
}
```

## Test Naming Conventions

**Function Names:**
- `test_<feature>_<scenario>` for specific tests
- `test_<feature>_<variant>` for related tests
- Descriptive names preferred over brief ones
- Invariant tests: `test_<invariant_description>()`
- Regression tests: `test_regression_<issue_description>()`

**Module Organization in Tests:**
```rust
#[cfg(test)]
mod btree_tests {
    use super::*;

    #[test]
    fn test_btree_lookup() { ... }

    #[test]
    fn test_btree_split() { ... }
}

#[cfg(test)]
mod allocator_tests {
    use super::*;

    #[test]
    fn test_allocate_free_cycle() { ... }
}
```

## V3-Specific Testing Considerations

**Concurrency Testing:**
- V3 uses `parking_lot::RwLock` for interior mutability
- V3Backend has `RwLock<BTreeManager>`, `RwLock<NodeStore>`, etc.
- Test concurrent reads: multiple handles reading simultaneously
- Test write exclusion: only one writer at a time
- Pattern:
```rust
#[test]
fn test_concurrent_reads() {
    let backend = Arc::new(V3Backend::create(db_path)?);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                backend.get_node(1).unwrap()
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
```

**Page Testing:**
- Test page boundary conditions (first page, last page, middle)
- Test page overflow (adding beyond capacity)
- Test page splitting (B+Tree growth)
- Pattern:
```rust
#[test]
fn test_page_split() {
    let mut page = NodePage::new(1);

    // Fill page to capacity
    for i in 0..TEST_PAGE_CAPACITY {
        page.insert_node(create_test_node(i));
    }

    // One more should trigger split
    let result = page.insert_node(create_test_node(TEST_PAGE_CAPACITY));
    assert!(matches!(result, Err(NativeBackendError::PageFull { .. })));
}
```

**B+Tree Testing:**
- Test insertion at root, internal, leaf levels
- Test tree growth (splitting up the tree)
- Test deletion with underflow
- Test lookup at each level
- Pattern:
```rust
#[test]
fn test_btree_growth() {
    let mut tree = BTreeManager::new(allocator, None);

    // Insert enough nodes to cause multiple splits
    for i in 0..1000 {
        tree.insert(i, page_id)?;
    }

    // Verify tree grew
    assert!(tree.height() > 1);

    // Verify lookups still work
    for i in 0..1000 {
        assert!(tree.lookup(i).is_some());
    }
}
```

**WAL Testing for V3:**
- Test WAL record types: PageAllocate, PageFree, PageWrite, BTreeSplit
- Test recovery from mid-crash
- Test checkpoint truncation
- Pattern:
```rust
#[test]
fn test_wal_recovery() {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("test.wal");

    {
        let mut writer = WALWriter::create(&wal_path)?;
        writer.write_page_allocate(1)?;
        writer.write_page_write(1, &[1, 2, 3])?;
    }  // Unflushed WAL

    // Recovery should replay WAL
    let backend = V3Backend::open_with_wal(&db_path, &wal_path)?;
    assert!(backend.page_is_allocated(1));
}
```

## Missing V3 Test Coverage

**Priority: High**
1. **Allocator integration tests** - Page allocation lifecycle
2. **BTree integration tests** - Tree growth and rebalancing
3. **WAL recovery tests** - Crash recovery scenarios
4. **Backend lifecycle tests** - Open, operate, close, reopen

**Priority: Medium**
1. **Compression tests** - Delta/varint encoding
2. **Edge compat tests** - Edge storage via V3
3. **Concurrent access tests** - RwLock behavior

**Priority: Low**
1. **Performance benchmarks** - Comparison with V2
2. **Stress tests** - Large-scale data handling

---

*Testing analysis: 2025-02-13*

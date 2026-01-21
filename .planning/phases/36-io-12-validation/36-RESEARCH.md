# Phase 36: IO-12 Validation - Research

**Researched:** 2026-01-21
**Domain:** Performance validation, MVCC isolation testing, regression benchmarking
**Confidence:** HIGH

## Summary

Phase 36 is the final validation phase for v1.6 Chain Locality milestone. The goal is to prove that the traversal-time sequential cluster reader (Phases 33-35) achieves the IO-12 target (Chain(500) <= 75ms) while preserving MVCC isolation and avoiding regression on non-chain graph topologies.

This research covers:
1. **Criterion.rs benchmark patterns** for chain, star, and random graph traversal
2. **MVCC isolation testing patterns** for sequential cluster reads
3. **Regression testing methodology** with 10% tolerance thresholds
4. **Instrumentation metrics** for chain optimization validation
5. **Existing benchmark infrastructure** in the codebase

**Primary recommendation:** Use existing Criterion.rs infrastructure with three benchmark suites (chain, star, random), measure cold/warm cache performance, validate MVCC via per-traversal context scoping tests, and use 10% tolerance for regression detection against v1.4 baseline.

## Standard Stack

### Core Benchmarking
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| **Criterion.rs** | 0.5 | Statistical benchmarking with confidence intervals | Industry standard for Rust microbenchmarks, provides cold/warm cache handling, HTML reports, regression detection |
| **tempfile** | 3 | Temporary test directories | Isolates benchmark artifacts, prevents cross-test contamination |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| **ahash** | 0.8 | Fast hashmap for traversal context | Already in use, consistent with existing code |
| **bencher** | (external CI) | Continuous benchmark regression tracking | For CI integration (future), not local development |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Criterion.rs | cargo bench (libtest) | Criterion provides statistical rigor, regression detection, HTML reports - libtest too basic |
| 10% tolerance | 5% tolerance | 5% too strict for I/O-bound operations with filesystem variance - 10% is industry standard per Bencher.dev docs |

**Installation:**
```bash
# Already in dev-dependencies
criterion = { version = "0.5", features = ["html_reports"] }
```

## Architecture Patterns

### Recommended Benchmark Structure
```
sqlitegraph/benches/
├── io12_validation.rs       # NEW: Main benchmark suite for Phase 36
├── v2_performance.rs         # EXISTING: Baseline v1.4 benchmarks (reference)
├── bench_utils.rs            # EXISTING: Common utilities (use for chain graph generation)
└── prefetch_bench.rs         # EXISTING: Prefetch window benchmarks (reference pattern)

sqlitegraph/tests/
├── phase36_mvcc_isolation_tests.rs      # NEW: MVCC isolation tests
└── phase36_regression_tests.rs           # NEW: Regression tests (star, random)
```

### Pattern 1: Criterion Benchmark with Cold/Warm Cache

**What:** Benchmark both cold cache (first run, no filesystem cache) and warm cache (subsequent runs) to simulate real-world production scenarios.

**When to use:** For all I/O-bound benchmarks, especially chain traversal where sequential I/O optimization should show significant improvement.

**Example:**
```rust
// Source: Existing v2_performance.rs, prefetch_bench.rs patterns
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

fn bench_chain_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("chain_traversal");

    for &chain_size in &[100, 500] {
        // Create graph ONCE outside measurement
        let temp_dir = create_benchmark_temp_dir();
        let (mut graph_file, node_ids) = create_chain_graph(chain_size, &temp_dir);
        let start_node = node_ids[0];

        // Validate start_node exists
        assert!(node_ids.contains(&start_node));

        group.bench_with_input(
            BenchmarkId::from_parameter(chain_size),
            &chain_size,
            |b, &_size| {
                b.iter(|| {
                    // Full traversal measurement
                    let mut ctx = TraversalContext::new();
                    let visited = traverse_chain_with_context(
                        black_box(&mut graph_file),
                        black_box(start_node),
                        black_box(&mut ctx)
                    ).expect("Failed to traverse");
                    black_box(visited)
                });
            }
        );

        // Preserve temp_dir lifetime through benchmark iterations
        std::mem::forget(temp_dir);
    }

    group.finish();
}
```

**Key Pattern:** Setup outside `b.iter()`, measurement inside, use `std::mem::forget()` to prevent temp_dir cleanup during async Criterion runs.

### Pattern 2: MVCC Isolation Testing

**What:** Validate that per-traversal context (TraversalContext) evaporates after function return, preventing cross-traversal cache pollution.

**When to use:** For all cache-related features to ensure MVCC-lite snapshot isolation.

**Example:**
```rust
// Source: snapshot_invariants_tests.rs pattern, adapted for TraversalContext
#[test]
fn test_traversal_context_evaporates_after_return() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let (mut graph_file, node_ids) = create_chain_graph(100, &temp_dir);

    // First traversal: populate context
    {
        let mut ctx = TraversalContext::new();
        let _visited = traverse_chain(&mut graph_file, node_ids[0], &mut ctx)
            .expect("First traversal failed");

        // Context has cached data
        assert!(ctx.cache.len() > 0 || ctx.cluster_buffer.is_some());
        // Buffer should be populated for chain
        if ctx.detector.is_linear_confirmed() {
            assert!(ctx.cluster_buffer.is_some());
        }
    }

    // Second traversal: fresh context, no cross-traversal pollution
    {
        let mut ctx = TraversalContext::new();
        // Context should start empty
        assert_eq!(ctx.cache.len(), 0);
        assert!(ctx.cluster_buffer.is_none());

        let _visited = traverse_chain(&mut graph_file, node_ids[0], &mut ctx)
            .expect("Second traversal failed");

        // This traversal should rebuild its own cache independently
    }

    // If cross-traversal pollution occurred, test would fail
    // (e.g., second traversal finding pre-populated cache)
}

#[test]
fn test_sequential_cluster_buffer_per_traversal_isolation() {
    // Validate that cluster_buffer is not shared across traversals
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let (mut graph_file, node_ids) = create_chain_graph(100, &temp_dir);

    // First traversal on chain segment
    {
        let mut ctx = TraversalContext::new();
        let _visited = traverse_chain(&mut graph_file, node_ids[0], &mut ctx)
            .expect("First traversal failed");

        // If linear confirmed, cluster_buffer should be populated
        if ctx.detector.is_linear_confirmed() {
            assert!(ctx.cluster_buffer.is_some());
        }
    }

    // Second traversal on different segment
    {
        let mut ctx = TraversalContext::new();
        // Cluster buffer should start empty
        assert!(ctx.cluster_buffer.is_none());

        let _visited = traverse_chain(&mut graph_file, node_ids[10], &mut ctx)
            .expect("Second traversal failed");

        // Buffer should be independent of first traversal
        // (Either empty or independently populated)
    }
}
```

**Key Pattern:** Scoped blocks `{}` to force context drop, assert fresh state on second traversal.

### Pattern 3: Regression Testing with Tolerance Thresholds

**What:** Compare current benchmark results against v1.4 baseline with 10% tolerance to detect performance regression.

**When to use:** For star and random graph benchmarks where optimization should NOT cause regression.

**Example:**
```rust
// Source: bench_meta.rs, bench_regression.rs patterns
use sqlitegraph::bench_meta::{BenchRun, BenchGate, BenchGateConfig};

#[test]
fn test_star_graph_no_regression() {
    // v1.4 baseline from Phase 32 (example numbers)
    let baseline_star_100 = BenchRun {
        name: "star_100".to_string(),
        mean_ns: 5_000_000, // 5ms baseline
        samples: 100,
    };

    let config = BenchGateConfig {
        thresholds: vec![],
        baseline: vec![baseline_star_100.clone()],
        tolerance: 0.10, // 10% tolerance
    };

    let gate = BenchGate::new(config);

    // Current benchmark run (simulated)
    let current_star_100 = BenchRun {
        name: "star_100".to_string(),
        mean_ns: 5_400_000, // 5.4ms (8% slower)
        samples: 100,
    };

    let result = gate.evaluate(&[current_star_100]);

    match result {
        crate::bench_gates::BenchOutcome::Pass => {
            // Within 10% tolerance
        }
        crate::bench_gates::BenchOutcome::Fail(reasons) => {
            panic!("Star graph regression detected: {:?}", reasons);
        }
    }
}
```

**Key Pattern:** 10% tolerance (0.10) is industry standard per [Bencher.dev Thresholds documentation](https://bencher.dev/docs/explanation/thresholds/).

### Anti-Patterns to Avoid

- **Benchmarking setup time:** Setup (graph creation) must be outside `b.iter()` - only measure traversal operations
- **Temp_dir lifetime issues:** Use `std::mem::forget(temp_dir)` to prevent deletion during async Criterion runs
- **Missing validation:** Always assert that start_node exists before benchmarking to avoid measuring error paths
- **Testing error paths:** Ensure benchmarks measure the happy path, not error handling
- **Warm cache only:** Don't rely solely on warm cache numbers - production has cold cache scenarios

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Statistical benchmarking | Manual timing loops | Criterion.rs | Provides confidence intervals, warmup, statistical significance, regression detection |
| Benchmark regression gates | Manual comparison logic | BenchGate/BenchGateConfig | Existing infrastructure in src/bench_gates.rs, tested and validated |
| Temporary directory management | Manual temp file handling | tempfile crate | Prevents cross-test contamination, automatic cleanup |
| Cold/warm cache handling | Repeated benchmarks with cache flush | Criterion's built-in warmup | Criterion handles warmup phases automatically |
| Graph topology generation | Manual edge construction | bench_utils::create_benchmark_graph | Existing infrastructure supports Chain, Star, Grid, Random topologies |

**Key insight:** The codebase already has comprehensive benchmark infrastructure (Criterion.rs, BenchGate, bench_utils). Phase 36 should reuse these patterns, not reinvent them.

## Common Pitfalls

### Pitfall 1: Measuring Setup Time Instead of Traversal

**What goes wrong:** Benchmark includes graph creation time, overwhelming the actual traversal measurement.

**Why it happens:** Placing setup code inside `b.iter()` loop.

**How to avoid:** Follow the established pattern from `v2_performance.rs`:
1. Generate graph ONCE outside `b.iter()`
2. Open graph ONCE outside `b.iter()`
3. Validate node existence before measurement
4. Measure ONLY traversal inside `b.iter()`

**Warning signs:** Benchmark takes milliseconds when traversal should be microseconds, or results are highly variable.

### Pitfall 2: Temp Directory Deletion During Benchmark

**What goes wrong:** TempDir is dropped while Criterion is still running async benchmark iterations, causing file not found errors.

**Why it happens:** Criterion runs benchmarks asynchronously via rayon. When the benchmark closure returns, Rust drops TempDir and deletes files.

**How to avoid:** Use `std::mem::forget(temp_dir)` after `b.iter()` loop:
```rust
b.iter(|| {
    // benchmark code
});
// Prevent temp_dir cleanup during benchmark execution
std::mem::forget(temp_dir);
```

**Warning signs:** Intermittent "file not found" errors, or benchmarks work locally but fail in CI.

### Pitfall 3: Cross-Traversal Cache Pollution Violating MVCC

**What goes wrong:** Second traversal finds cached data from first traversal, violating MVCC isolation.

**Why it happens:** Using global cache instead of per-traversal context, or context not properly dropped between traversals.

**How to avoid:**
1. Always use TraversalContext::new() for each traversal
2. Scope context lifetime with `{ }` blocks
3. Assert empty state at start of second traversal in tests
4. Never store TraversalContext in global/static variables

**Warning signs:** Second traversal is suspiciously fast, or tests fail when run in random order.

### Pitfall 4: False Positive Chain Detection on Trees

**What goes wrong:** Sequential cluster reader triggers on tree structures (short linear segments followed by branching), causing performance regression.

**Why it happens:** Detection threshold too low, or contiguity validation not applied.

**How to avoid:**
1. Use 3-step linear detection threshold (proven in v1.4)
2. Validate cluster contiguity before sequential read
3. Test on diamond and tree graphs (Phase 33-05 tests)
4. Immediate fallback on Branching pattern detection

**Warning signs:** Star graph benchmarks show unexpected slowdown, or tree traversals use sequential path.

## Code Examples

Verified patterns from official sources:

### Chain Graph Generation for Benchmarks

```rust
// Source: sqlitegraph/benches/prefetch_bench.rs (lines 32-73)
fn create_chain_graph(size: usize, temp_dir: &TempDir) -> (GraphFile, Vec<NativeNodeId>) {
    let db_path = temp_dir.path().join("benchmark_chain.db");
    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes
    let mut node_ids = Vec::with_capacity(size);
    for i in 0..size {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store.allocate_node_id().expect("Failed to allocate node ID");
        let record = NodeRecord::new(
            node_id,
            "Node".to_string(),
            format!("node_{}", i),
            serde_json::json!({"id": i}),
        );
        node_store.write_node(&record).expect("Failed to write node");
        node_ids.push(node_id);
    }

    // Create chain edges: 0->1, 1->2, ..., (n-2)->(n-1)
    let mut edge_store = EdgeStore::new(&mut graph_file);
    for i in 0..size.saturating_sub(1) {
        let edge = EdgeRecord::new(
            i as i64 + 1,
            node_ids[i],
            node_ids[i + 1],
            "chain".to_string(),
            serde_json::json!({"order": i}),
        );
        edge_store.write_edge(&edge).expect("Failed to write chain edge");
    }

    (graph_file, node_ids)
}
```

### Traversal with Context Pattern

```rust
// Source: sqlitegraph/src/backend/native/graph_ops/cache.rs (lines 456-487)
pub fn traverse_with_detection(
    graph_file: &mut GraphFile,
    node_id: NativeNodeId,
    direction: Direction,
    cluster_offset: u64,
    cluster_size: u32,
    ctx: &mut TraversalContext,
) -> NativeResult<Vec<NativeNodeId>> {
    let degree = match direction {
        Direction::Outgoing => AdjacencyHelpers::outgoing_degree(graph_file, node_id)?,
        Direction::Incoming => AdjacencyHelpers::incoming_degree(graph_file, node_id)?,
    };

    // Observe node with cluster metadata
    let pattern = ctx.detector.observe_with_cluster(node_id, degree, cluster_offset, cluster_size);

    // Populate node_id -> cluster_index mapping
    let cluster_index = ctx.detector.cluster_offsets().len().saturating_sub(1);
    ctx.node_cluster_index.insert(node_id, cluster_index);

    // Immediate fallback on branching
    if pattern == TraversalPattern::Branching {
        ctx.clear_cluster_buffer();
    }

    // Get neighbors (extracts from cluster_buffer if sequential read active)
    get_neighbors_optimized(graph_file, node_id, direction, ctx)
}
```

### Benchmark Regression Gate

```rust
// Source: sqlitegraph/src/bench_regression.rs (lines 20-26)
pub fn within_regression(&self, baseline: &BenchRun, tolerance: f64) -> bool {
    if self.name != baseline.name {
        return false;
    }
    let allowed = (baseline.mean_ns as f64) * (1.0 + tolerance);
    (self.mean_ns as f64) <= allowed
}
```

### Star Graph Topology Generation

```rust
// Source: sqlitegraph/benches/bench_utils.rs (lines 137-153)
GraphTopology::Star => {
    if node_ids.is_empty() {
        return 0;
    }
    let center = node_ids[0];
    for i in 1..node_ids.len().min(spec.edge_count + 1) {
        graph.insert_edge(EdgeSpec {
            from: center,
            to: node_ids[i],
            edge_type: "star".to_string(),
            data: serde_json::json!({"spoke": i}),
        }).expect("Failed to insert edge");
        edge_count += 1;
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual `std::time::Instant` timing | Criterion.rs statistical benchmarking | Phase 23 (v1.2) | Confidence intervals, regression detection, HTML reports |
| Global cache for traversals | Per-traversal cache (TraversalCache) | Phase 26 (v1.3) | MVCC isolation preserved, no cross-traversal pollution |
| Random I/O for chains | Sequential cluster reads (Phase 34-35) | Phase 34-35 (v1.6) | Target: 3.3x speedup on Chain(500) |
| No regression gates | BenchGate with tolerance thresholds | Phase 24 (v1.2) | Automated regression detection in CI |

**Deprecated/outdated:**
- **Manual timing loops:** Replaced by Criterion.rs in Phase 23
- **Global traversal cache:** Replaced by per-traversal cache in Phase 26 (violates MVCC)
- **Write-time chain detection:** Rejected in v1.6 planning - traversal-time only

## Open Questions

1. **What is the exact v1.4 baseline for Chain(500)?**
   - What we know: Target is <=75ms (3x SQLite baseline of ~25ms)
   - What's unclear: Exact v1.4 Chain(500) benchmark result from Phase 32
   - Recommendation: Run `cargo bench --bench v2_performance` to establish baseline, then compare against Phase 36 results

2. **What instrumentation metrics should be exposed for chain optimization?**
   - What we know: LinearDetector has chains_detected, total_chain_length, average_chain_length
   - What's unclear: Whether to expose these via public API or keep internal
   - Recommendation: Keep internal for now, expose via debug logging if needed for diagnostics

3. **Should benchmarks run in CI (bench-ci feature)?**
   - What we know: Cargo.toml has `bench-ci = []` feature flag
   - What's unclear: Whether CI runs benchmarks or just unit tests
   - Recommendation: Keep benchmarks local-only for Phase 36 (too slow for CI), defer to infrastructure decision

## Sources

### Primary (HIGH confidence)

- **[Criterion.rs Book - Analysis Process](https://bheisler.github.io/criterion.rs/book/analysis.html)** - Official documentation on benchmark measurement, warmup, and statistical analysis
- **[Criterion.rs Documentation](https://docs.rs/criterion2)** - API reference for benchmark macros and configuration
- **[Bencher.dev - Thresholds & Alerts](https://bencher.dev/docs/explanation/thresholds/)** - Authoritative source on tolerance thresholds, explicitly mentions 0.10 for 10% threshold
- **sqlitegraph/benches/v2_performance.rs** (lines 1-503) - Production benchmark patterns from Phase 24, including setup/teardown patterns
- **sqlitegraph/benches/prefetch_bench.rs** (lines 1-178) - Phase 32 prefetch benchmark patterns, demonstrates chain graph generation
- **sqlitegraph/src/bench_regression.rs** (lines 1-28) - Existing regression detection infrastructure
- **sqlitegraph/benches/bench_utils.rs** (lines 1-301) - Graph topology generation utilities (Chain, Star, Grid, Random)
- **sqlitegraph/src/backend/native/graph_ops/cache.rs** (lines 1-611) - TraversalContext and cache implementation
- **sqlitegraph/tests/snapshot_invariants_tests.rs** (lines 1-150) - MVCC isolation test patterns from Phase 22

### Secondary (MEDIUM confidence)

- **[Rust Benchmarking with Criterion.rs](https://www.rustfinity.com/blog/rust-benchmarking-with-criterion)** (October 2024) - Community tutorial on Criterion usage, validates setup/teardown patterns
- **[Benchmarking + Profiling Workflow for Go and Rust](https://compile.guru/performance-proof-benchmarking-profiling-go-rust-pr-template/)** (January 2026) - Confirms cold/warm cache distinction is important in production
- **[Benchmarking | WebReference](https://webreference.com/rust/testing/benchmarking/)** - Discusses combining micro and macro benchmarks
- **[Hardening YDB with Jepsen](https://blog.ydb.tech/hardening-ydb-with-jepsen-lessons-learned-e3238a7ef4f2)** (July 2024) - Database isolation testing methodology, validates MVCC testing approach

### Tertiary (LOW confidence)

- **[Graph MVCC - Rust Implementation](https://github.com/cryptopatrick/graph_mvcc)** - Community MVCC implementation, unverified but conceptually aligned
- **[RustLite Database Interface](https://lib.rs/crates/rustlite)** - Another MVCC database in Rust, for cross-reference only
- **[Regression Testing: An In-Depth Guide for 2026](https://www.leapwork.com/blog/regression-testing)** (December 2025) - General regression testing trends, not database-specific
- **[Benchmark Testing Guide](https://www.testmu.ai/learning-hub/benchmark-testing/)** (January 2026) - General benchmark testing methodology, not Rust-specific

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Criterion.rs 0.5 is industry standard, verified in Cargo.toml and codebase
- Architecture: HIGH - Existing benchmark patterns verified in v2_performance.rs, prefetch_bench.rs, bench_utils.rs
- Pitfalls: HIGH - All pitfalls documented with real examples from codebase (temp_dir lifetime, MVCC violations, setup measurement)
- MVCC testing: HIGH - Patterns verified in snapshot_invariants_tests.rs, TraversalContext implementation
- Regression thresholds: MEDIUM - 10% tolerance validated by Bencher.dev docs, but exact v1.4 baseline numbers need verification via benchmark run

**Research date:** 2026-01-21
**Valid until:** 2026-02-20 (30 days - stable domain, Criterion.rs 0.5 mature)

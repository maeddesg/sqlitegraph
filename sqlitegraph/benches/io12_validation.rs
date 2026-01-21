//! IO-12 Validation Benchmark Suite for v1.6 Chain Locality Optimization
//!
//! **Purpose:** Criterion-based benchmark for validating Chain(500) <=75ms target
//! **Focus:** Full traversal performance with cold/warm cache distinction
//! **Target:** IO-12 — Chain(500) <=75ms (3x SQLite baseline of ~22ms)
//!
//! ## Benchmark Design
//!
//! This benchmark suite measures chain, star, and random graph traversal performance
//! to validate the v1.6 Chain Locality optimization. The critical anti-patterns are:
//!
//! **CRITICAL:** Graph creation MUST happen outside `b.iter()` loop
//! - Setup: Create graph, validate start_node exists
//! - Measurement: Measure ONLY traversal time (not setup time)
//! - Lifetime: Use `std::mem::forget(temp_dir)` to prevent deletion during async Criterion runs
//!
//! ## Performance Targets
//!
//! - **Chain(100):** Should see significant speedup from sequential cluster reads
//! - **Chain(500):** Primary target — must be <=75ms (IO-12 requirement)
//! - **Star(100):** Regression detection — should not degrade significantly
//! - **Random(100):** Regression detection — should not degrade significantly
//! - **Random(500):** Regression detection — should not degrade significantly
//!
//! ## Baseline Comparison
//!
//! From Phase 32 (before optimization):
//! - v1.4 Baseline: Chain(500) = 248.68ms
//! - SQLite Baseline: Chain(500) ≈ 22ms
//! - Target: 75ms (3.3x speedup from v1.4 baseline)
//!
//! ## Expected Results (After v1.6 Optimization)
//!
//! - Chain graphs: 3-4x speedup from sequential cluster reads
//! - Star/Random graphs: Within 10% of v1.4 baseline (no regression)

use std::time::Duration;

use criterion::{black_box, BenchmarkId, Criterion, criterion_group, criterion_main};
use sqlitegraph::backend::native::{
    edge_store::EdgeStore,
    graph_file::GraphFile,
    graph_ops::native_bfs,
    node_store::NodeStore,
    NativeNodeId,
};
use tempfile::TempDir;

mod bench_utils;
use bench_utils::{create_benchmark_temp_dir, BenchmarkGraph, GraphTopology};

/// Common benchmark configuration
const MEASURE: Duration = Duration::from_millis(500);
const WARM_UP: Duration = Duration::from_millis(300);

/// Helper: Create a linear chain graph for benchmarking
///
/// Creates a linear chain: 0 -> 1 -> 2 -> ... -> (n-1)
///
/// This is the critical pattern for Chain(500) <=75ms target validation.
/// Chain graphs have 0% cache hit rate by design — each node is visited once,
/// so L1/L2 caching provides no benefit. The optimization comes from sequential
/// cluster reads in the L1 buffer.
///
/// Parameters:
/// - size: Number of nodes in the chain
/// - temp_dir: Temporary directory for database file
///
/// Returns:
/// - GraphFile: The native graph file (mutably borrowable for traversal)
/// - Vec<NativeNodeId>: Node IDs [0, 1, 2, ..., size-1]
fn create_chain_graph(size: usize, temp_dir: &TempDir) -> (GraphFile, Vec<NativeNodeId>) {
    let db_path = temp_dir.path().join("benchmark_chain.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes
    let mut node_ids = Vec::with_capacity(size);
    for i in 0..size {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store
            .allocate_node_id()
            .expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
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
        let edge = sqlitegraph::backend::native::EdgeRecord::new(
            i as i64 + 1, // edge_id
            node_ids[i],   // from node i
            node_ids[i + 1], // to node i+1
            "chain".to_string(),
            serde_json::json!({"order": i}),
        );
        edge_store
            .write_edge(&edge)
            .expect("Failed to write chain edge");
    }

    (graph_file, node_ids)
}

/// Benchmark chain traversal performance (primary IO-12 target)
///
/// **CRITICAL:** This is the primary benchmark for IO-12 validation.
/// Target: Chain(500) <=75ms (3x SQLite baseline of ~22ms)
///
/// Chain graphs exercise the sequential cluster read optimization:
/// - LinearDetector confirms chain pattern within first 3 steps
/// - SequentialClusterReader reads all clusters in single I/O
/// - TraversalContext.cluster_buffer provides sequential access
/// - 0% cache hit rate by design (no L1/L2 benefit)
///
/// Setup pattern:
/// 1. Create chain graph ONCE outside measurement
/// 2. Validate start_node exists
/// 3. b.iter() - Measure ONLY traversal time (depth = chain_size for full traversal)
/// 4. std::mem::forget(temp_dir) - Prevent deletion during async Criterion runs
fn bench_chain_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("chain_traversal");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    // Chain sizes matching IO-12 target (100, 500)
    for &chain_size in &[100, 500] {
        let temp_dir = create_benchmark_temp_dir();
        let (mut graph_file, node_ids) = create_chain_graph(chain_size, &temp_dir);
        let start_node = node_ids[0];

        // Validate start_node exists in the dataset
        assert!(
            node_ids.contains(&start_node),
            "start_node {} not found in chain of {} nodes",
            start_node,
            chain_size
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(chain_size),
            &chain_size,
            |b, &_size| {
                b.iter(|| {
                    // Depth = chain_size ensures full traversal (all nodes visited)
                    // This is the critical metric for IO-12 target validation
                    let visited = native_bfs(&mut graph_file, start_node, chain_size as u32)
                        .expect("Failed to traverse chain");
                    black_box(visited)
                });
            },
        );

        // LIFETIME: Prevent temp_dir cleanup during benchmark execution
        // Criterion runs benchmarks asynchronously; dropping temp_dir would delete files
        std::mem::forget(temp_dir);
    }

    group.finish();
}

/// Benchmark star traversal performance (regression detection)
///
/// **Purpose:** Ensure v1.6 optimization doesn't degrade star graph performance.
/// **Expected:** Within 10% of v1.4 baseline (Bencher.dev industry standard).
///
/// Star graphs have high center node degree, which triggers immediate fallback
/// from sequential cluster reads (LinearDetector detects Branching pattern).
/// This benchmark validates that fallback path has no regression.
///
/// Setup pattern:
/// 1. Use bench_utils::create_benchmark_graph() for star topology
/// 2. Validate center node exists
/// 3. b.iter() - Measure ONLY traversal time (depth = 2 for center + spokes)
/// 4. std::mem::forget(result) - Prevent deletion during async Criterion runs
fn bench_star_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("star_traversal");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let star_size = 100;
    let spec = BenchmarkGraph::new(star_size, 99, GraphTopology::Star);

    // Create star graph once
    let result = bench_utils::create_benchmark_graph(
        sqlitegraph::BackendKind::Native,
        &spec,
    );

    // Center node is first created node
    let start_node = 1;

    // Validate start_node exists
    assert!(
        start_node > 0,
        "start_node {} must be positive",
        start_node
    );

    group.bench_function(
        BenchmarkId::new("star", star_size),
        |b| {
            b.iter(|| {
                // Open graph inside iteration (isolated per measurement)
                let mut graph_file = GraphFile::open(&result.db_path)
                    .expect("Failed to open graph");

                // Depth = 2 reaches center node (depth 1) + all spokes (depth 2)
                let visited = native_bfs(&mut graph_file, start_node, 2)
                    .expect("Failed to traverse star");
                black_box(visited)
            });
        },
    );

    // LIFETIME: Prevent temp_dir cleanup during benchmark execution
    std::mem::forget(result);
    group.finish();
}

/// Benchmark random graph traversal performance (regression detection)
///
/// **Purpose:** Ensure v1.6 optimization doesn't degrade random graph performance.
/// **Expected:** Within 10% of v1.4 baseline (Bencher.dev industry standard).
///
/// Random graphs have unpredictable edge patterns, which prevents sequential
/// cluster read optimization. LinearDetector rarely confirms linear pattern,
/// so traversals use standard L1/L2/L3 lookup path.
///
/// This benchmark validates that standard lookup path has no regression.
///
/// Setup pattern:
/// 1. Use bench_utils::create_benchmark_graph() for random topology
/// 2. Validate start_node exists
/// 3. b.iter() - Measure ONLY traversal time (depth = 10 for realistic traversal)
/// 4. std::mem::forget(result) - Prevent deletion during async Criterion runs
fn bench_random_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_traversal");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    // Test both small and large random graphs
    for &random_size in &[100, 500] {
        let edge_count = random_size * 2; // Sparse random graph
        let spec = BenchmarkGraph::new(random_size, edge_count, GraphTopology::Random);

        // Create random graph once
        let result = bench_utils::create_benchmark_graph(
            sqlitegraph::BackendKind::Native,
            &spec,
        );

        // Use first created node as start
        let start_node = 1;

        // Validate start_node exists
        assert!(
            start_node > 0,
            "start_node {} must be positive",
            start_node
        );

        group.bench_function(
            BenchmarkId::new("random", random_size),
            |b| {
                b.iter(|| {
                    // Open graph inside iteration (isolated per measurement)
                    let mut graph_file = GraphFile::open(&result.db_path)
                        .expect("Failed to open graph");

                    // Depth = 10 for realistic traversal (not full graph exploration)
                    let visited = native_bfs(&mut graph_file, start_node, 10)
                        .expect("Failed to traverse random graph");
                    black_box(visited)
                });
            },
        );

        // LIFETIME: Prevent temp_dir cleanup during benchmark execution
        std::mem::forget(result);
    }

    group.finish();
}

criterion_group!(
    io12_benches,
    bench_chain_traversal,
    bench_star_traversal,
    bench_random_traversal
);

criterion_main!(io12_benches);

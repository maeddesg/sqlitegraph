//! Prefetch Window Benchmark for Sequential I/O Optimization
//!
//! **Purpose:** Criterion-based benchmark for comparing prefetch window sizes
//! **Focus:** Buffer prefetch operation timing across different window sizes
//!
//! This benchmark uses Criterion.rs to measure the performance of
//! SequentialReadBuffer prefetch operations with different window sizes.
//!
//! **NOTE:** As of Phase 32-01, L1 buffer neighbor extraction is instrumentation-only.
//! Full neighbor extraction from buffered NodeRecordV2 is deferred to Plan 32-04.
//! Therefore, these benchmarks measure buffer prefetch operations directly.

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use sqlitegraph::backend::native::{
    graph_file::GraphFile,
    node_store::NodeStore,
    edge_store::EdgeStore,
    adjacency::SequentialReadBuffer,
    NativeNodeId,
};
use tempfile::TempDir;

mod bench_utils;
use bench_utils::create_benchmark_temp_dir;

/// Common benchmark configuration
const MEASURE: Duration = Duration::from_millis(500);
const WARM_UP: Duration = Duration::from_millis(300);

/// Helper: Create a linear chain graph for benchmarking
///
/// Creates a linear chain: 0 -> 1 -> 2 -> ... -> (n-1)
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

/// Benchmark prefetch operations for different window sizes
fn bench_prefetch_windows(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("prefetch_window");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let window_sizes = [4, 8, 16, 32];
    let chain_sizes = [100, 500];

    for &window_size in &window_sizes {
        for &chain_size in &chain_sizes {
            let bench_id = BenchmarkId::new(format!("window_{}/chain_{}", window_size, chain_size), chain_size);

            group.bench_with_input(bench_id, &chain_size, |b, &_size| {
                // Create graph in setup (outside iteration loop)
                let temp_dir = create_benchmark_temp_dir();
                let (mut graph_file, node_ids) = create_chain_graph(chain_size, &temp_dir);
                let start_node = node_ids[0];

                b.iter(|| {
                    // Create buffer and prefetch
                    let mut buffer = SequentialReadBuffer::with_prefetch_window(window_size);
                    let _ = buffer.prefetch_from(&mut graph_file, start_node);

                    // Prevent buffer from being dropped too early
                    buffer.len()
                });

                // Prevent temp_dir cleanup during benchmark iterations
                std::mem::forget(temp_dir);
            });
        }
    }

    group.finish();
}

/// Benchmark prefetch operations scaled by chain size
fn bench_prefetch_chain_sizes(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("prefetch_chain_size");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    // Focus on default window 8 for chain size scaling
    let chain_sizes = [50, 100, 200, 500];

    for &chain_size in &chain_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(chain_size), &chain_size, |b, &_size| {
            let temp_dir = create_benchmark_temp_dir();
            let (mut graph_file, node_ids) = create_chain_graph(chain_size, &temp_dir);
            let start_node = node_ids[0];

            b.iter(|| {
                let mut buffer = SequentialReadBuffer::with_prefetch_window(8);
                let _ = buffer.prefetch_from(&mut graph_file, start_node);
                buffer.len()
            });

            std::mem::forget(temp_dir);
        });
    }

    group.finish();
}

/// Benchmark buffer insert operations
fn bench_buffer_insert(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("buffer_insert");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let insert_counts = [4, 8, 16];

    for &count in &insert_counts {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            use sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2;

            b.iter(|| {
                let mut buffer = SequentialReadBuffer::with_prefetch_window(count);
                for i in 1..=count {
                    let node = NodeRecordV2::new(
                        i as i64,
                        "TestNode".to_string(),
                        format!("node_{}", i),
                        serde_json::json!({"data": "x".repeat(50)}),
                    );
                    buffer.insert(node);
                }
                buffer.len()
            });
        });
    }

    group.finish();
}

criterion_group!(
    prefetch_benches,
    bench_prefetch_windows,
    bench_prefetch_chain_sizes,
    bench_buffer_insert
);
criterion_main!(prefetch_benches);

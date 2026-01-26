//! LinearDetector Overhead Microbenchmark
//!
//! **Purpose:** Criterion-based microbenchmark to isolate LinearDetector pattern detection overhead
//! **Focus:** Measure per-node observe() cost and contiguity validation cost
//! **Target:** Identify CPU bottleneck in linear pattern detection
//!
//! ## Benchmark Design
//!
//! This microbenchmark isolates the performance of `LinearDetector` operations:
//! - `observe()` - Per-node pattern detection overhead
//! - `observe_with_cluster()` - Pattern detection with cluster offset tracking
//! - `are_clusters_contiguous()` - Contiguity validation cost
//! - `validate_contiguity()` - Method wrapper overhead
//!
//! ## Measurement Strategy
//!
//! - **Setup:** Create vectors of node IDs and degrees matching linear patterns
//! - **Measurement:** Benchmark only the detection operation (no I/O)
//! - **Black box:** Use `black_box` to prevent compiler optimization
//! - **Parameters:** Vary node count [100, 500, 1000, 5000]
//!
//! ## Expected Results
//!
//! - observe() should be ~5-10ns per node (inlineable, minimal state)
//! - observe_with_cluster() should be ~10-20ns per node (includes Vec push)
//! - Contiguity validation should scale linearly with cluster count
//! - 100 clusters: ~100-200ns
//! - 500 clusters: ~500-1000ns

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use sqlitegraph::backend::native::{
    NativeNodeId,
    adjacency::{LinearDetector, are_clusters_contiguous},
};

/// Common benchmark configuration
const MEASURE: Duration = Duration::from_millis(500);
const WARM_UP: Duration = Duration::from_millis(300);

/// Benchmark: Per-node observe() overhead
///
/// Measures the cost of LinearDetector::observe() for a pure linear chain.
/// All nodes have degree 1, which triggers the linear pattern after 3 steps.
fn bench_observe_overhead(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linear_detector/observe");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let node_counts = [100, 500, 1000, 5000];

    for &count in &node_counts {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            b.iter(|| {
                let mut detector = LinearDetector::new();

                // Simulate observing a linear chain
                for i in 0..count {
                    let node_id = i as NativeNodeId;
                    let degree = 1u32; // Linear chain: all nodes have degree 1
                    black_box(detector.observe(black_box(node_id), black_box(degree)));
                }

                // Prevent optimization
                black_box(detector.is_linear_confirmed());
            });
        });
    }

    group.finish();
}

/// Benchmark: observe_with_cluster() overhead with cluster tracking
///
/// Measures the cost of pattern detection when also tracking cluster offsets.
/// This includes the Vec::push() overhead for storing (offset, size) tuples.
fn bench_observe_with_cluster_overhead(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linear_detector/observe_with_cluster");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let node_counts = [100, 500, 1000, 5000];

    for &count in &node_counts {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            b.iter(|| {
                let mut detector = LinearDetector::new();

                // Simulate observing a linear chain with contiguous clusters
                // Each cluster is 4096 bytes, starting at offset 1024
                let mut current_offset = 1024u64;
                let cluster_size = 4096u32;

                for i in 0..count {
                    let node_id = i as NativeNodeId;
                    let degree = 1u32;

                    black_box(detector.observe_with_cluster(
                        black_box(node_id),
                        black_box(degree),
                        black_box(current_offset),
                        black_box(cluster_size),
                    ));

                    current_offset += cluster_size as u64;
                }

                // Prevent optimization
                black_box(detector.cluster_offsets().len());
                black_box(detector.is_linear_confirmed());
            });
        });
    }

    group.finish();
}

/// Benchmark: Contiguity validation cost
///
/// Measures the cost of validating that cluster offsets form a contiguous sequence.
/// This is called during traversal to confirm sequential reads will be beneficial.
fn bench_contiguity_validation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linear_detector/contiguity");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100, 500];

    for &count in &cluster_counts {
        // Create contiguous cluster offsets
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;
        let cluster_size = 4096u32;

        for _ in 0..count {
            offsets.push((current_offset, cluster_size));
            current_offset += cluster_size as u64;
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                black_box(are_clusters_contiguous(black_box(&offsets)));
            });
        });
    }

    group.finish();
}

/// Benchmark: validate_contiguity() method overhead
///
/// Measures the method wrapper cost vs calling are_clusters_contiguous() directly.
/// The method adds timing instrumentation on top of the core validation logic.
fn bench_validate_contiguity_method(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linear_detector/validate_method");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100, 500];

    for &count in &cluster_counts {
        // Pre-create cluster offsets (contiguous)
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;
        let cluster_size = 4096u32;

        for _ in 0..count {
            offsets.push((current_offset, cluster_size));
            current_offset += cluster_size as u64;
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                // Create fresh detector for each iteration
                let mut detector = LinearDetector::new();

                // Populate with cluster offsets
                for (i, &(offset, size)) in offsets.iter().enumerate() {
                    let node_id = i as NativeNodeId;
                    detector.observe_with_cluster(node_id, 1, offset, size);
                }

                // Benchmark validate_contiguity() method
                black_box(detector.validate_contiguity());
            });
        });
    }

    group.finish();
}

/// Benchmark: Non-contiguous cluster validation
///
/// Measures contiguity validation when clusters have gaps (worst case).
/// This should return false but still requires full scan.
fn bench_non_contiguous_validation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linear_detector/non_contiguous");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100, 500];

    for &count in &cluster_counts {
        // Create non-contiguous cluster offsets (every other cluster has a gap)
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;
        let cluster_size = 4096u32;
        let gap_size = 4096u64;

        for i in 0..count {
            offsets.push((current_offset, cluster_size));

            // Add gap after every other cluster
            if i % 2 == 0 {
                current_offset += cluster_size as u64 + gap_size;
            } else {
                current_offset += cluster_size as u64;
            }
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                black_box(are_clusters_contiguous(black_box(&offsets)));
            });
        });
    }

    group.finish();
}

/// Benchmark: Pattern detection with branching (false positive prevention)
///
/// Measures how quickly detector identifies branching patterns.
/// Branching should be detected immediately (first node with degree > 1).
fn bench_branching_detection(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("linear_detector/branching");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let branching_positions = [0, 1, 2, 10, 50];

    for &pos in &branching_positions {
        group.bench_with_input(
            BenchmarkId::new(format!("branch_at_{}", pos), pos),
            &pos,
            |b, &branch_pos| {
                b.iter(|| {
                    let mut detector = LinearDetector::new();

                    // Linear nodes until branching point
                    for i in 0..branch_pos {
                        black_box(detector.observe(i as NativeNodeId, 1));
                    }

                    // Branching node
                    black_box(detector.observe(branch_pos as NativeNodeId, 2));

                    // Continue observing (should stay in Branching state)
                    for i in (branch_pos + 1)..(branch_pos + 10) {
                        black_box(detector.observe(i as NativeNodeId, 1));
                    }

                    // Prevent optimization
                    black_box(detector.is_linear_confirmed());
                    black_box(detector.current_pattern());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    linear_detector_benches,
    bench_observe_overhead,
    bench_observe_with_cluster_overhead,
    bench_contiguity_validation,
    bench_validate_contiguity_method,
    bench_non_contiguous_validation,
    bench_branching_detection
);
criterion_main!(linear_detector_benches);

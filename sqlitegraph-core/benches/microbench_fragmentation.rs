//! Fragmentation Impact Microbenchmark
//!
//! **Purpose:** Criterion-based microbenchmark to measure fragmentation effect on contiguity validation
//! **Focus:** Quantify how gap patterns affect validation performance
//! **Target:** Identify I/O efficiency degradation with fragmentation
//!
//! ## Benchmark Design
//!
//! This microbenchmark measures the performance of `are_clusters_contiguous()` across
//! different fragmentation patterns:
//! - **Contiguous clusters**: Optimal case (no gaps)
//! - **Fixed gaps**: Regular spacing between clusters (50% overhead)
//! - **Random gaps**: Simulated real-world fragmentation (varying scores)
//!
//! ## Measurement Strategy
//!
//! - **Setup:** Create cluster offset vectors with different gap patterns
//! - **Measurement:** Benchmark only contiguity validation (no I/O)
//! - **Black box:** Use `black_box` to prevent compiler optimization
//! - **Throughput**: Report bytes/second to show I/O efficiency
//!
//! ## Expected Results
//!
//! - Contiguous: ~100-200ns for 100 clusters (linear scan, fast path)
//! - Fixed gaps: ~200-400ns (additional gap calculations)
//! - Random gaps: ~300-600ns (same complexity as fixed gaps)
//! - Throughput degrades with fragmentation (more bytes spanned for same data)

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use sqlitegraph::backend::native::adjacency::are_clusters_contiguous;

/// Common benchmark configuration
const MEASURE: Duration = Duration::from_millis(500);
const WARM_UP: Duration = Duration::from_millis(300);

const CLUSTER_SIZE: u32 = 4096; // 4KB clusters

/// Benchmark: Contiguous clusters (optimal case)
///
/// Measures validation performance when clusters are perfectly contiguous.
/// This is the best-case scenario for sequential I/O.
fn bench_contiguous_clusters(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fragmentation/contiguous");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100, 500];

    for &count in &cluster_counts {
        // Create contiguous cluster offsets
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;

        for _ in 0..count {
            offsets.push((current_offset, CLUSTER_SIZE));
            current_offset += CLUSTER_SIZE as u64;
        }

        let total_bytes = current_offset - 1024;
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                black_box(are_clusters_contiguous(black_box(&offsets)));
            });
        });
    }

    group.finish();
}

/// Benchmark: Gapped clusters with fixed spacing
///
/// Measures validation performance when clusters have regular gaps.
/// This simulates a database with fragmentation from mixed workloads.
///
/// Gap pattern: Every cluster has a 4KB gap after it (50% overhead).
fn bench_gapped_clusters_fixed(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fragmentation/gapped_fixed");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100, 500];

    for &count in &cluster_counts {
        // Create cluster offsets with fixed 4KB gaps
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;
        let gap_size = 4096u64; // 4KB gap after each cluster

        for _ in 0..count {
            offsets.push((current_offset, CLUSTER_SIZE));
            current_offset += CLUSTER_SIZE as u64 + gap_size;
        }

        let total_bytes = current_offset - 1024;
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                black_box(are_clusters_contiguous(black_box(&offsets)));
            });
        });
    }

    group.finish();
}

/// Benchmark: Gapped clusters with variable spacing
///
/// Measures validation performance when gaps vary in size.
/// This simulates realistic fragmentation patterns.
///
/// Gap pattern: Alternating 2KB, 4KB, 8KB gaps (cycle of 3).
fn bench_gapped_clusters_variable(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fragmentation/gapped_variable");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100, 500];

    for &count in &cluster_counts {
        // Create cluster offsets with variable gaps
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;
        let gap_pattern = [2048u64, 4096u64, 8192u64]; // 2KB, 4KB, 8KB cycle
        let mut gap_index = 0;

        for _ in 0..count {
            offsets.push((current_offset, CLUSTER_SIZE));
            current_offset += CLUSTER_SIZE as u64 + gap_pattern[gap_index];
            gap_index = (gap_index + 1) % gap_pattern.len();
        }

        let total_bytes = current_offset - 1024;
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                black_box(are_clusters_contiguous(black_box(&offsets)));
            });
        });
    }

    group.finish();
}

/// Benchmark: Random gaps (simulated real-world fragmentation)
///
/// Measures validation performance with randomized gap patterns.
/// Fragmentation scores: 0.1, 0.3, 0.5, 0.7
///
/// Fragmentation score = gap_bytes / total_span
/// - 0.1: Mild fragmentation (10% overhead)
/// - 0.3: Moderate fragmentation (30% overhead)
/// - 0.5: Severe fragmentation (50% overhead)
/// - 0.7: Very severe fragmentation (70% overhead)
fn bench_random_gaps(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fragmentation/random");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let fragmentation_scores = [0.1, 0.3, 0.5, 0.7];
    let cluster_count = 100;

    for &frag_score in &fragmentation_scores {
        // Create cluster offsets with random gaps matching target fragmentation
        let mut offsets = Vec::with_capacity(cluster_count);
        let mut current_offset = 1024u64;

        // Calculate target gap ratio
        // frag_score = gap_bytes / (cluster_bytes + gap_bytes)
        // gap_bytes = frag_score * cluster_bytes / (1 - frag_score)
        let cluster_bytes: u64 = cluster_count as u64 * CLUSTER_SIZE as u64;
        let target_gap_bytes = (frag_score * cluster_bytes as f64 / (1.0 - frag_score)) as u64;
        let avg_gap_size = target_gap_bytes / cluster_count as u64;

        // Use deterministic "random" gaps based on frag_score
        for i in 0..cluster_count {
            offsets.push((current_offset, CLUSTER_SIZE));

            // Vary gap size using sine wave to simulate randomness
            let gap_variation = ((i as f64 * 0.5).sin().abs() * avg_gap_size as f64) as u64;
            let gap_size = avg_gap_size / 2 + gap_variation;

            current_offset += CLUSTER_SIZE as u64 + gap_size;
        }

        let total_bytes = current_offset - 1024;
        let actual_frag =
            target_gap_bytes as f64 / (cluster_bytes as f64 + target_gap_bytes as f64);

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new(format!("frag_{:.1}", actual_frag), frag_score),
            &cluster_count,
            |b, &_count| {
                b.iter(|| {
                    black_box(are_clusters_contiguous(black_box(&offsets)));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Worst-case fragmentation (maximum gaps)
///
/// Measures validation performance with extreme fragmentation.
/// Every cluster is separated by large gaps (simulating highly fragmented storage).
fn bench_worst_case_fragmentation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fragmentation/worst_case");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let cluster_counts = [10, 50, 100];

    for &count in &cluster_counts {
        // Create cluster offsets with maximum gaps
        let mut offsets = Vec::with_capacity(count);
        let mut current_offset = 1024u64;
        let gap_size = 65536u64; // 64KB gaps (16x cluster size)

        for _ in 0..count {
            offsets.push((current_offset, CLUSTER_SIZE));
            current_offset += CLUSTER_SIZE as u64 + gap_size;
        }

        let total_bytes = current_offset - 1024;
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &_count| {
            b.iter(|| {
                black_box(are_clusters_contiguous(black_box(&offsets)));
            });
        });
    }

    group.finish();
}

/// Benchmark: Mixed contiguous and gapped regions
///
/// Measures validation performance when the database has both contiguous
/// and fragmented regions (realistic mixed workload pattern).
///
/// Pattern: 20 contiguous, 10 gapped, 30 contiguous, 15 gapped, etc.
fn bench_mixed_regions(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fragmentation/mixed");
    group.measurement_time(MEASURE);
    group.warm_up_time(WARM_UP);

    let total_clusters = 100;

    // Create mixed pattern: alternating contiguous runs and gapped runs
    let mut offsets = Vec::with_capacity(total_clusters);
    let mut current_offset = 1024u64;
    let mut i = 0;

    while i < total_clusters {
        // Contiguous run (20 clusters)
        let contiguous_len = 20.min(total_clusters - i);
        for _ in 0..contiguous_len {
            offsets.push((current_offset, CLUSTER_SIZE));
            current_offset += CLUSTER_SIZE as u64;
        }
        i += contiguous_len;

        // Gapped run (10 clusters with 4KB gaps)
        let gapped_len = 10.min(total_clusters - i);
        for _ in 0..gapped_len {
            offsets.push((current_offset, CLUSTER_SIZE));
            current_offset += CLUSTER_SIZE as u64 + 4096;
        }
        i += gapped_len;
    }

    let total_bytes = current_offset - 1024;
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function("mixed_pattern", |b| {
        b.iter(|| {
            black_box(are_clusters_contiguous(black_box(&offsets)));
        });
    });

    group.finish();
}

criterion_group!(
    fragmentation_benches,
    bench_contiguous_clusters,
    bench_gapped_clusters_fixed,
    bench_gapped_clusters_variable,
    bench_random_gaps,
    bench_worst_case_fragmentation,
    bench_mixed_regions
);
criterion_main!(fragmentation_benches);

//! Comprehensive compression benchmarks for delta encoding
//!
//! This benchmark measures actual compression ratios for various edge ID patterns:
//! - Sequential IDs (best case)
//! - Sparse IDs with gaps
//! - Random IDs (worst case)
//! - Realistic graph patterns
//!
//! Goal: Verify the "42% space savings" claim with real data.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use sqlitegraph::backend::native::v3::compression::edge_delta::{
    compress_edge_ids, compression_ratio, decompress_edge_ids,
};
use std::time::Duration;

/// Generate sequential edge IDs: [1, 2, 3, ..., n]
fn generate_sequential_ids(count: usize) -> Vec<i64> {
    (1..=count as i64).collect()
}

/// Generate sparse edge IDs with gaps: [1, 10, 20, 30, ...]
fn generate_sparse_ids(count: usize, gap: i64) -> Vec<i64> {
    (0..count as i64).map(|i| 1 + i * gap).collect()
}

/// Generate random edge IDs
fn generate_random_ids(count: usize, seed: u64) -> Vec<i64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);

    (0..count as i64)
        .map(|i| {
            let mut h = DefaultHasher::new();
            i.hash(&mut h);
            (h.finish() % 10000) as i64
        })
        .collect()
}

/// Generate realistic social network pattern
/// - Sequential user IDs
/// - Each user follows 5-10 other users
/// - Small local gaps, occasional large gaps
fn generate_social_network_pattern(user_count: usize) -> Vec<i64> {
    let mut ids = Vec::new();

    for user_id in 1..=user_count as i64 {
        let num_connections = 5 + (user_id % 5); // 5-9 connections per user

        for i in 0..num_connections {
            // Connect to users with slightly higher IDs (common in social networks)
            let target_id = user_id + i + 1;
            ids.push(target_id);
        }
    }

    ids.sort();
    ids.dedup();
    ids
}

/// Generate realistic web graph pattern
/// - Power law distribution (few hubs, many leaf nodes)
/// - Sequential IDs with clustering
fn generate_web_graph_pattern(page_count: usize) -> Vec<i64> {
    let mut ids = Vec::new();

    // Hub pages (10% of total) link to many pages
    let hub_count = page_count / 10;

    for hub_id in 1..=hub_count as i64 {
        // Each hub links to 50-100 pages
        let links_per_hub = 50 + (hub_id % 51);

        for i in 0..links_per_hub {
            let target_id = 1 + (i % page_count as i64);
            ids.push(target_id);
        }
    }

    // Leaf pages (90% of total) link to 1-5 pages
    for leaf_id in (hub_count + 1)..=page_count {
        let links_per_leaf = 1 + (leaf_id % 5);

        for i in 0..links_per_leaf {
            let target_id = (leaf_id + i) as i64;
            ids.push(target_id);
        }
    }

    ids.sort();
    ids.dedup();
    ids
}

/// Benchmark compression throughput for different patterns
fn benchmark_compression_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_throughput");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    let sizes = vec![100, 1000, 10000, 100000];

    for size in sizes {
        // Sequential IDs
        let sequential_ids = generate_sequential_ids(size);
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            &sequential_ids,
            |b, ids| {
                b.iter(|| compress_edge_ids(black_box(ids)));
            },
        );

        // Sparse IDs (gap of 10)
        let sparse_ids = generate_sparse_ids(size, 10);
        group.bench_with_input(
            BenchmarkId::new("sparse_gap_10", size),
            &sparse_ids,
            |b, ids| {
                b.iter(|| compress_edge_ids(black_box(ids)));
            },
        );

        // Random IDs
        let random_ids = generate_random_ids(size, 42);
        group.bench_with_input(BenchmarkId::new("random", size), &random_ids, |b, ids| {
            b.iter(|| compress_edge_ids(black_box(ids)));
        });
    }

    group.finish();
}

/// Benchmark decompression throughput
fn benchmark_decompression_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression_throughput");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    let sizes = vec![100, 1000, 10000, 100000];

    for size in sizes {
        let sequential_ids = generate_sequential_ids(size);
        let compressed = compress_edge_ids(&sequential_ids);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            &(compressed.clone(), size),
            |b, (data, count)| {
                b.iter(|| decompress_edge_ids(black_box(data), black_box(*count)).unwrap());
            },
        );
    }

    group.finish();
}

/// Measure actual compression ratios for different patterns
fn benchmark_compression_ratios(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_ratio_analysis");
    group.measurement_time(Duration::from_secs(5)); // Quick analysis
    group.sample_size(10);

    let test_cases = vec![
        ("sequential_1k", 1000, "sequential"),
        ("sparse_1k", 1000, "sparse"),
        ("random_1k", 1000, "random"),
        ("social_network_1k", 1000, "social"),
        ("web_graph_1k", 1000, "web"),
        ("sequential_10k", 10000, "sequential"),
        ("sparse_10k", 10000, "sparse"),
        ("social_network_10k", 10000, "social"),
    ];

    for (name, size, pattern) in test_cases {
        group.bench_with_input(
            BenchmarkId::new(name, pattern),
            &(size, pattern),
            |b, (size, pattern)| {
                b.iter(|| {
                    let ids = match *pattern {
                        "sequential" => generate_sequential_ids(*size),
                        "sparse" => generate_sparse_ids(*size, 10),
                        "random" => generate_random_ids(*size, 42),
                        "social" => generate_social_network_pattern(*size),
                        "web" => generate_web_graph_pattern(*size),
                        _ => generate_sequential_ids(*size),
                    };

                    let original_size = ids.len() * 8; // 8 bytes per i64
                    let compressed = compress_edge_ids(&ids);
                    let compressed_size = compressed.len();
                    let ratio = compression_ratio(&ids, &compressed);
                    let space_savings_pct = (1.0 - ratio) * 100.0;

                    (
                        original_size,
                        compressed_size,
                        ratio,
                        space_savings_pct,
                        ids.len(),
                    )
                });
            },
        );
    }

    group.finish();
}

/// Detailed compression analysis (not a benchmark, but diagnostic)
fn analyze_compression_patterns() {
    println!("\n=== COMPRESSION ANALYSIS ===\n");

    let patterns = vec![
        ("Sequential IDs (best case)", generate_sequential_ids(10000)),
        ("Sparse IDs (gap=10)", generate_sparse_ids(10000, 10)),
        ("Sparse IDs (gap=100)", generate_sparse_ids(10000, 100)),
        ("Random IDs (worst case)", generate_random_ids(10000, 42)),
        (
            "Social Network Pattern",
            generate_social_network_pattern(1000),
        ),
        ("Web Graph Pattern", generate_web_graph_pattern(1000)),
    ];

    for (name, ids) in patterns {
        let original_size = ids.len() * 8; // 8 bytes per i64
        let compressed = compress_edge_ids(&ids);
        let compressed_size = compressed.len();
        let ratio = compression_ratio(&ids, &compressed);
        let space_savings_pct = (1.0 - ratio) * 100.0;
        let bytes_saved = original_size.saturating_sub(compressed_size);

        println!("{}", name);
        println!("  Original size:    {} bytes", original_size);
        println!("  Compressed size:  {} bytes", compressed_size);
        println!("  Bytes saved:      {} bytes", bytes_saved);
        println!("  Compression ratio: {:.3} (compressed/original)", ratio);
        println!("  Space savings:    {:.1}%", space_savings_pct);

        // Analyze delta distribution
        let mut deltas = Vec::new();
        let mut prev_id = 0i64;
        for &id in &ids {
            deltas.push(id - prev_id);
            prev_id = id;
        }

        let avg_delta = deltas.iter().sum::<i64>() as f64 / deltas.len() as f64;
        let min_delta = *deltas.iter().min().unwrap();
        let max_delta = *deltas.iter().max().unwrap();
        let small_deltas = deltas.iter().filter(|&&d| d.abs() <= 127).count();

        println!("  Delta stats:");
        println!("    Avg delta:     {:.1}", avg_delta);
        println!("    Min delta:     {}", min_delta);
        println!("    Max delta:     {}", max_delta);
        println!(
            "    Small deltas (≤127): {} ({:.1}%)",
            small_deltas,
            small_deltas as f64 / deltas.len() as f64 * 100.0
        );
        println!();
    }

    println!("=== VERIFICATION OF 42% CLAIM ===\n");

    // Test the claim with realistic data
    let test_sizes = vec![1000, 5000, 10000, 50000, 100000];

    println!("Sequential IDs (best case):");
    for size in test_sizes {
        let ids = generate_sequential_ids(size);
        let compressed = compress_edge_ids(&ids);
        let ratio = compression_ratio(&ids, &compressed);
        let savings_pct = (1.0 - ratio) * 100.0;
        println!(
            "  {:6} edges: {:.1}% savings (ratio: {:.3})",
            size, savings_pct, ratio
        );
    }

    println!("\nSocial Network Pattern (realistic):");
    for size in &[100, 500, 1000] {
        let ids = generate_social_network_pattern(*size);
        let compressed = compress_edge_ids(&ids);
        let ratio = compression_ratio(&ids, &compressed);
        let savings_pct = (1.0 - ratio) * 100.0;
        println!(
            "  {:4} users: {:.1}% savings (ratio: {:.3}) - {} edges",
            size,
            savings_pct,
            ratio,
            ids.len()
        );
    }

    println!("\nRandom IDs (worst case):");
    for size in &[1000, 5000, 10000] {
        let ids = generate_random_ids(*size, 42);
        let compressed = compress_edge_ids(&ids);
        let ratio = compression_ratio(&ids, &compressed);
        let savings_pct = (1.0 - ratio) * 100.0;
        println!(
            "  {:6} edges: {:.1}% savings (ratio: {:.3})",
            size, savings_pct, ratio
        );
    }
}

criterion_group!(
    benches,
    benchmark_compression_throughput,
    benchmark_decompression_throughput,
    benchmark_compression_ratios
);
criterion_main!(benches);

/// Run this function to see detailed analysis
#[cfg(test)]
mod test_analysis {
    use super::*;

    #[test]
    fn run_compression_analysis() {
        analyze_compression_patterns();
    }
}

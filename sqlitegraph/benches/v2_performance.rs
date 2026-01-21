//! V2 Performance Benchmarks
//!
//! Comprehensive performance testing for SQLiteGraph V2 backend.
//! Measures insertion throughput, neighbor queries, traversal performance,
//! and file growth patterns under different graph topologies.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use sqlitegraph::{BackendDirection, GraphConfig, NeighborQuery};

mod v2_dataset_generator;
use v2_dataset_generator::{V2GraphMode, V2GraphSpec, generate_v2_graph};

/// Benchmark V2 edge insertion throughput
fn bench_v2_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("v2_insertion");

    // Test different graph sizes (must fit within 8MB node region = ~2048 nodes)
    for &node_count in &[100, 500, 1_000, 1_500] {
        let edge_count = node_count * 4; // Sparse graph ratio

        let spec = V2GraphSpec::new(node_count, edge_count, V2GraphMode::Mixed);

        group.throughput(Throughput::Elements(edge_count as u64));
        group.bench_with_input(
            BenchmarkId::new("mixed_graph", node_count),
            &spec,
            |b, spec| {
                b.iter(|| {
                    let result = generate_v2_graph(spec);
                    let output = black_box((
                        result.edge_count,
                        result.file_size_bytes,
                        result.bytes_per_edge,
                        result.bytes_per_node,
                        result.growth_efficiency,
                    ));
                    std::mem::forget(result); // Prevent TempDir deletion during benchmark
                    output
                });
            },
        );
    }

    group.finish();
}

/// Benchmark neighbor query performance for different node degrees
fn bench_v2_neighbor_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("v2_neighbor_queries");

    // Create test graphs with different characteristics (must fit within node region)
    let specs = vec![
        V2GraphSpec::new(1_000, 4_000, V2GraphMode::Sparse), // Low degree
        V2GraphSpec::new(1_000, 4_000, V2GraphMode::PowerLaw), // Hub-heavy
        V2GraphSpec::new(1_000, 4_000, V2GraphMode::MultiEdge), // Multi-edge
        V2GraphSpec::new(1_000, 4_000, V2GraphMode::Bidirectional), // High bidirectional
    ];

    for (i, spec) in specs.iter().enumerate() {
        // Generate graph once for each spec
        let result = generate_v2_graph(spec);

        // Find nodes with different degree characteristics
        let mut low_degree_nodes = Vec::new();
        let mut high_degree_nodes = Vec::new();
        let mut hub_nodes = Vec::new();

        for (&node_id, &(outgoing, incoming)) in &result.node_degrees {
            let total_degree = outgoing + incoming;
            if total_degree <= 5 {
                low_degree_nodes.push(node_id);
            } else if total_degree >= 50 {
                hub_nodes.push(node_id);
            } else if total_degree >= 20 {
                high_degree_nodes.push(node_id);
            }
        }

        // Benchmark low degree nodes
        if !low_degree_nodes.is_empty() {
            // Open graph ONCE before benchmark loop to exclude setup time
            let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
                .expect("Failed to reopen graph");

            // Validate graph has expected nodes before benchmarking
            let max_id = result.max_node_id();
            let first_node = *low_degree_nodes.first().expect("low_degree_nodes should not be empty");
            assert!(
                max_id >= first_node,
                "Graph max_node_id ({}) >= first target node ({})",
                max_id, first_node
            );

            group.bench_with_input(
                BenchmarkId::new(format!("{}_low_degree", i), low_degree_nodes.len()),
                &(&graph, &low_degree_nodes),
                |b, (graph, nodes)| {
                    b.iter(|| {
                        for &node_id in nodes.iter().take(10) {
                            let _neighbors = black_box(
                                graph
                                    .neighbors(
                                        node_id,
                                        NeighborQuery {
                                            direction: BackendDirection::Outgoing,
                                            edge_type: None,
                                        },
                                    )
                                    .expect("Failed to get neighbors"),
                            );
                        }
                    });
                },
            );
        }

        // Benchmark high degree nodes
        if !high_degree_nodes.is_empty() {
            // Open graph ONCE before benchmark loop to exclude setup time
            let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
                .expect("Failed to reopen graph");

            // Validate graph has expected nodes before benchmarking
            let max_id = result.max_node_id();
            let first_node = *high_degree_nodes.first().expect("high_degree_nodes should not be empty");
            assert!(
                max_id >= first_node,
                "Graph max_node_id ({}) >= first target node ({})",
                max_id, first_node
            );

            group.bench_with_input(
                BenchmarkId::new(format!("{}_high_degree", i), high_degree_nodes.len()),
                &(&graph, &high_degree_nodes),
                |b, (graph, nodes)| {
                    b.iter(|| {
                        for &node_id in nodes.iter().take(5) {
                            let _neighbors = black_box(
                                graph
                                    .neighbors(
                                        node_id,
                                        NeighborQuery {
                                            direction: BackendDirection::Outgoing,
                                            edge_type: None,
                                        },
                                    )
                                    .expect("Failed to get neighbors"),
                            );
                        }
                    });
                },
            );
        }

        // Benchmark hub nodes
        if !hub_nodes.is_empty() {
            // Open graph ONCE before benchmark loop to exclude setup time
            let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
                .expect("Failed to reopen graph");

            // Validate graph has expected nodes before benchmarking
            let max_id = result.max_node_id();
            let first_node = *hub_nodes.first().expect("hub_nodes should not be empty");
            assert!(
                max_id >= first_node,
                "Graph max_node_id ({}) >= first target node ({})",
                max_id, first_node
            );

            group.bench_with_input(
                BenchmarkId::new(format!("{}_hub_nodes", i), hub_nodes.len()),
                &(&graph, &hub_nodes),
                |b, (graph, nodes)| {
                    b.iter(|| {
                        for &node_id in nodes.iter().take(3) {
                            let _neighbors = black_box(
                                graph
                                    .neighbors(
                                        node_id,
                                        NeighborQuery {
                                            direction: BackendDirection::Outgoing,
                                            edge_type: None,
                                        },
                                    )
                                    .expect("Failed to get neighbors"),
                            );
                        }
                    });
                },
            );
        }

        // Prevent temp_dir deletion until benchmark completes
        // The benchmark closures above borrow `graph`, which borrows `db_path`,
        // which references the temp directory. We need to keep temp_dir alive
        // through all benchmarks in this iteration.
        std::mem::forget(result);
    }

    group.finish();
}

/// Benchmark BFS traversal performance
fn bench_v2_bfs_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("v2_bfs_traversal");

    // Test BFS on different graph sizes (must fit within node region)
    for &node_count in &[500, 1_000, 1_500] {
        let edge_count = node_count * 4; // Sparse graph for BFS
        let spec = V2GraphSpec::new(node_count, edge_count, V2GraphMode::Mixed);

        let result = generate_v2_graph(&spec);

        // VALIDATION: Assert start_node exists in the generated dataset
        let start_node = result.node_ids[0];
        assert!(
            result.node_ids.contains(&start_node),
            "BFS start_node {} not found in generated dataset of {} nodes",
            start_node,
            result.node_ids.len()
        );

        group.bench_with_input(
            BenchmarkId::new("bfs_depth_5", node_count),
            &(&result, start_node, 5),
            |b, (result, start_node, depth)| {
                let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
                    .expect("Failed to reopen graph");

                b.iter(|| {
                    let _visited =
                        black_box(graph.bfs(*start_node, *depth).expect("Failed to run BFS"));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("bfs_depth_10", node_count),
            &(&result, start_node, 10),
            |b, (result, start_node, depth)| {
                let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
                    .expect("Failed to reopen graph");

                b.iter(|| {
                    let _visited =
                        black_box(graph.bfs(*start_node, *depth).expect("Failed to run BFS"));
                });
            },
        );

        std::mem::forget(result); // Preserve temp_dir after benchmark
    }

    group.finish();
}

/// Benchmark k-hop traversal performance
fn bench_v2_k_hop_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("v2_k_hop_traversal");

    let spec = V2GraphSpec::new(1_500, 6_000, V2GraphMode::Mixed);
    let result = generate_v2_graph(&spec);

    // VALIDATION: Assert start_node exists in the generated dataset
    let start_node = result.node_ids[0];
    assert!(
        result.node_ids.contains(&start_node),
        "K-hop start_node {} not found in generated dataset of {} nodes",
        start_node,
        result.node_ids.len()
    );

    let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
        .expect("Failed to reopen graph");

    for depth in [2, 3, 4, 5].iter() {
        group.bench_with_input(BenchmarkId::new("outgoing", *depth), depth, |b, &depth| {
            b.iter(|| {
                let _neighbors = black_box(
                    graph
                        .k_hop(start_node, depth, BackendDirection::Outgoing)
                        .expect("Failed to run k-hop"),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("incoming", *depth), depth, |b, &depth| {
            b.iter(|| {
                let _neighbors = black_box(
                    graph
                        .k_hop(start_node, depth, BackendDirection::Incoming)
                        .expect("Failed to run k-hop"),
                );
            });
        });
    }

    std::mem::forget(result); // Preserve temp_dir after benchmark
    group.finish();
}

/// Benchmark file growth and memory usage
fn bench_v2_file_growth(c: &mut Criterion) {
    let mut group = c.benchmark_group("v2_file_growth");

    // Test file growth patterns with different topologies (must fit within node region)
    let modes = [
        V2GraphMode::Sparse,
        V2GraphMode::PowerLaw,
        V2GraphMode::MultiEdge,
    ];

    for mode in modes.iter() {
        for &size in &[100, 500, 1_000] {
            let edge_count = match mode {
                V2GraphMode::MultiEdge => size * 5, // Fewer edges for multi-edge
                _ => size * 4,
            };

            let spec = V2GraphSpec::new(size, edge_count, *mode);

            group.bench_with_input(
                BenchmarkId::new(format!("{:?}", mode), size),
                &spec,
                |b, spec| {
                    b.iter(|| {
                        let result = generate_v2_graph(spec);
                        let bytes_per_edge =
                            result.file_size_bytes as f64 / result.edge_count as f64;
                        let bytes_per_node =
                            result.file_size_bytes as f64 / result.node_ids.len() as f64;
                        let output = black_box((bytes_per_edge, bytes_per_node, result.file_size_bytes));
                        std::mem::forget(result); // Prevent TempDir deletion during benchmark
                        output
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark multi-edge specific scenarios (Phase 50 validation)
fn bench_v2_multiedge_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("v2_multiedge_scenarios");

    // Test with different multi-edge factors (must fit within node region)
    for &multi_factor in &[3, 5, 10] {
        let node_count = std::cmp::min(500, 2000 / multi_factor);
        let spec = V2GraphSpec::new(node_count, node_count * multi_factor, V2GraphMode::MultiEdge)
            .with_multi_edge_factor(multi_factor);

        group.bench_with_input(
            BenchmarkId::new("insertion", multi_factor),
            &spec,
            |b, spec| {
                b.iter(|| {
                    let result = generate_v2_graph(spec);
                    let output = black_box(result.edge_count);
                    std::mem::forget(result); // Prevent TempDir deletion during benchmark
                    output
                });
            },
        );

        // Benchmark neighbor queries on multi-edge graphs
        let result = generate_v2_graph(&spec);
        let graph = sqlitegraph::open_graph(&result.db_path, &GraphConfig::native())
            .expect("Failed to reopen graph");

        // Find nodes with multi-edge connections (high degree)
        let mut multiedge_nodes = Vec::new();
        for (&node_id, &(outgoing, _)) in &result.node_degrees {
            if outgoing >= multi_factor {
                multiedge_nodes.push(node_id);
                if multiedge_nodes.len() >= 10 {
                    break;
                }
            }
        }

        // VALIDATION: Assert all multiedge_nodes exist in the generated dataset
        for &node_id in &multiedge_nodes {
            assert!(
                result.node_ids.contains(&node_id),
                "Multiedge node {} not found in generated dataset of {} nodes",
                node_id,
                result.node_ids.len()
            );
        }

        if !multiedge_nodes.is_empty() {
            group.bench_with_input(
                BenchmarkId::new("neighbors_dedup", multi_factor),
                &(&graph, &multiedge_nodes),
                |b, (graph, nodes)| {
                    b.iter(|| {
                        for &node_id in nodes.iter() {
                            let _neighbors = black_box(
                                graph
                                    .neighbors(
                                        node_id,
                                        NeighborQuery {
                                            direction: BackendDirection::Outgoing,
                                            edge_type: None,
                                        },
                                    )
                                    .expect("Failed to get neighbors"),
                            );
                        }
                    });
                },
            );
        }

        std::mem::forget(result); // Preserve temp_dir after benchmark
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_v2_insertion,
    bench_v2_neighbor_queries,
    bench_v2_bfs_traversal,
    bench_v2_k_hop_traversal,
    bench_v2_file_growth,
    bench_v2_multiedge_scenarios
);

criterion_main!(benches);

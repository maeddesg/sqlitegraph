//! Phase 55 — V2 Performance Characterization (Evidence-Only, No Hype)
//!
//! Measures REAL performance characteristics of SQLiteGraph V2 under controlled,
//! repeatable conditions — without claims, comparisons, or extrapolation.
//!
//! This is a MEASUREMENT phase. NOT optimization. NOT marketing.

use sqlitegraph::{BackendDirection, EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, open_graph};
use std::time::Instant;

/// Dataset specification for performance characterization
struct DatasetSpec {
    name: &'static str,
    node_count: usize,
    edge_count: usize,
    rng_seed: u32,
}

impl DatasetSpec {
    fn new(name: &'static str, node_count: usize, edge_count: usize) -> Self {
        Self {
            name,
            node_count,
            edge_count,
            rng_seed: 0xCAFE_BABE, // Fixed seed for reproducibility
        }
    }
}

/// Performance measurement results
#[derive(Clone, Debug)]
struct PerformanceResult {
    node_count: usize,
    edge_count: usize,
    node_insertion_time_ms: u128,
    edge_insertion_time_ms: u128,
    low_degree_neighbor_query_ms: u128,
    high_degree_neighbor_query_ms: u128,
    bfs_query_ms: Option<u128>,
    file_size_bytes: u64,
    bytes_per_node: f64,
    bytes_per_edge: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 55 V2 Performance Characterization ===");
    println!("Evidence-only measurement without claims or comparisons\n");

    // Define exact benchmark matrix
    let datasets = vec![
        DatasetSpec::new("Small", 1_000, 4_000),
        DatasetSpec::new("Medium", 10_000, 40_000),
        DatasetSpec::new("Large", 50_000, 200_000), // STOP if > 5 min
    ];

    let mut results = Vec::new();

    for spec in datasets {
        println!(
            "--- Dataset: {} ({} nodes, {} edges) ---",
            spec.name, spec.node_count, spec.edge_count
        );

        let result = measure_dataset(&spec)?;
        print_results(&result);
        results.push(result.clone());

        // Stop condition: if runtime exceeds 5 minutes
        if spec.node_count == 50_000
            && (result.node_insertion_time_ms + result.edge_insertion_time_ms) > 300_000
        {
            println!("STOP: Large dataset exceeded 5 minute limit");
            break;
        }
    }

    // Baseline consistency check
    println!("\n--- Baseline Consistency Check ---");
    let baseline_spec = DatasetSpec::new("Small-Consistency-Check", 1_000, 4_000);
    let baseline_1 = measure_dataset(&baseline_spec)?;
    let baseline_2 = measure_dataset(&baseline_spec)?;

    check_consistency(&baseline_1, &baseline_2)?;

    // Validation matrix check
    println!("\n--- Validation Matrix Check ---");
    run_validation_matrix()?;

    Ok(())
}

fn measure_dataset(spec: &DatasetSpec) -> Result<PerformanceResult, Box<dyn std::error::Error>> {
    // Create temporary directory for this dataset
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join(format!("v2_perf_{}.db", spec.name));

    // Open V2 NativeGraphBackend
    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config)?;

    println!("Database path: {}", db_path.display());

    // PHASE 1: Node insertion
    println!("Inserting {} nodes...", spec.node_count);
    let start_time = Instant::now();
    let mut node_ids = Vec::with_capacity(spec.node_count);

    let mut rng_state = spec.rng_seed;
    for i in 0..spec.node_count {
        let node_id = graph.insert_node(NodeSpec {
            kind: "TestNode".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({
                "index": i,
                "dataset": spec.name,
            }),
        })?;
        node_ids.push(node_id);

        // Progress every 1000 nodes
        if (i + 1) % 1000 == 0 {
            let elapsed = start_time.elapsed().as_millis();
            let rate = (i + 1) * 1000 / elapsed.max(1) as usize;
            println!(
                "  Inserted {}/{} nodes ({:.1} nodes/sec)",
                i + 1,
                spec.node_count,
                rate
            );
        }
    }

    let node_elapsed = start_time.elapsed().as_millis();
    println!("Node insertion completed: {} ms", node_elapsed);

    // PHASE 2: Edge insertion
    println!("Inserting {} edges...", spec.edge_count);
    let edges_start_time = Instant::now();

    let mut rng_state = spec.rng_seed;
    for i in 0..spec.edge_count {
        // Use seeded RNG for deterministic edge generation
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let from_idx = rng_state as usize % spec.node_count;

        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let mut to_idx = rng_state as usize % spec.node_count;

        // Avoid self-loops and maintain sparse directed graph
        if to_idx == from_idx {
            to_idx = (to_idx + 1) % spec.node_count;
        }

        let _edge_id = graph.insert_edge(EdgeSpec {
            from: node_ids[from_idx],
            to: node_ids[to_idx],
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({
                "edge_index": i,
                "dataset": spec.name,
            }),
        })?;

        // Progress every 5000 edges
        if (i + 1) % 5000 == 0 {
            let elapsed = edges_start_time.elapsed().as_millis();
            let rate = (i + 1) * 1000 / elapsed.max(1) as usize;
            println!(
                "  Inserted {}/{} edges ({:.1} edges/sec)",
                i + 1,
                spec.edge_count,
                rate
            );
        }
    }

    let edge_elapsed = edges_start_time.elapsed().as_millis();
    println!("Edge insertion completed: {} ms", edge_elapsed);

    // PHASE 3: Neighbor queries
    println!("Running neighbor queries...");

    // Find low-degree node (use first node)
    let low_degree_node = node_ids[0];
    let low_start = Instant::now();
    let low_neighbors = graph.neighbors(
        low_degree_node,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;
    let low_elapsed = low_start.elapsed().as_millis();
    println!(
        "Low-degree node {}: {} neighbors ({} ms)",
        low_degree_node,
        low_neighbors.len(),
        low_elapsed
    );

    // Find high-degree node (use middle node - likely to have more edges)
    let high_degree_node = node_ids[spec.node_count / 2];
    let high_start = Instant::now();
    let high_neighbors = graph.neighbors(
        high_degree_node,
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: None,
        },
    )?;
    let high_elapsed = high_start.elapsed().as_millis();
    println!(
        "High-degree node {}: {} neighbors ({} ms)",
        high_degree_node,
        high_neighbors.len(),
        high_elapsed
    );

    // PHASE 4: BFS traversal (if stable)
    let bfs_elapsed = if spec.node_count <= 10_000 {
        println!("Running BFS traversal...");
        let bfs_start = Instant::now();
        let bfs_result = graph.bfs(node_ids[0], 2);
        match bfs_result {
            Ok(visited) => {
                let elapsed = bfs_start.elapsed().as_millis();
                println!(
                    "BFS depth=2: {} nodes visited ({} ms)",
                    visited.len(),
                    elapsed
                );
                Some(elapsed)
            }
            Err(e) => {
                println!("BFS not available or unstable: {}", e);
                None
            }
        }
    } else {
        println!("Skipping BFS for large dataset");
        None
    };

    // PHASE 5: Disk footprint measurement
    println!("Measuring disk footprint...");
    let file_size = std::fs::metadata(&db_path)?.len();
    let bytes_per_node = file_size as f64 / spec.node_count as f64;
    let bytes_per_edge = file_size as f64 / spec.edge_count as f64;

    println!("File size: {} bytes", file_size);
    println!("Bytes per node: {:.2}", bytes_per_node);
    println!("Bytes per edge: {:.2}", bytes_per_edge);

    Ok(PerformanceResult {
        node_count: spec.node_count,
        edge_count: spec.edge_count,
        node_insertion_time_ms: node_elapsed,
        edge_insertion_time_ms: edge_elapsed,
        low_degree_neighbor_query_ms: low_elapsed,
        high_degree_neighbor_query_ms: high_elapsed,
        bfs_query_ms: bfs_elapsed,
        file_size_bytes: file_size,
        bytes_per_node,
        bytes_per_edge,
    })
}

fn print_results(result: &PerformanceResult) {
    println!("\n=== PERFORMANCE RESULTS ===");
    println!(
        "Node insertion: {} ms ({:.1} nodes/sec)",
        result.node_insertion_time_ms,
        result.node_count as f64 * 1000.0 / result.node_insertion_time_ms as f64
    );
    println!(
        "Edge insertion: {} ms ({:.1} edges/sec)",
        result.edge_insertion_time_ms,
        result.edge_count as f64 * 1000.0 / result.edge_insertion_time_ms as f64
    );
    println!(
        "Low-degree neighbor query: {} ms",
        result.low_degree_neighbor_query_ms
    );
    println!(
        "High-degree neighbor query: {} ms",
        result.high_degree_neighbor_query_ms
    );

    if let Some(bfs_ms) = result.bfs_query_ms {
        println!("BFS traversal: {} ms", bfs_ms);
    } else {
        println!("BFS traversal: SKIPPED");
    }

    println!("File size: {} bytes", result.file_size_bytes);
    println!(
        "Storage efficiency: {:.2} bytes/node, {:.2} bytes/edge",
        result.bytes_per_node, result.bytes_per_edge
    );
    println!("Total entities: {}", result.node_count + result.edge_count);
    println!("========================\n");
}

fn check_consistency(
    result1: &PerformanceResult,
    result2: &PerformanceResult,
) -> Result<(), String> {
    println!(
        "First run: Node={}ms, Edge={}ms, Size={} bytes",
        result1.node_insertion_time_ms, result1.edge_insertion_time_ms, result1.file_size_bytes
    );
    println!(
        "Second run: Node={}ms, Edge={}ms, Size={} bytes",
        result2.node_insertion_time_ms, result2.edge_insertion_time_ms, result2.file_size_bytes
    );

    // Check timing variance (±5%)
    let node_variance =
        (result1.node_insertion_time_ms as f64 - result2.node_insertion_time_ms as f64).abs()
            / result1.node_insertion_time_ms as f64;
    let edge_variance =
        (result1.edge_insertion_time_ms as f64 - result2.edge_insertion_time_ms as f64).abs()
            / result1.edge_insertion_time_ms as f64;

    if node_variance > 0.05 || edge_variance > 0.05 {
        return Err(format!(
            "Timing variance exceeded 5%: node={:.1}%, edge={:.1}%",
            node_variance * 100.0,
            edge_variance * 100.0
        ));
    }

    // Check file size consistency
    if result1.file_size_bytes != result2.file_size_bytes {
        return Err(format!(
            "File size mismatch: {} vs {} bytes",
            result1.file_size_bytes, result2.file_size_bytes
        ));
    }

    println!("✅ Consistency check PASSED: timing within ±5%, file size identical");
    Ok(())
}

fn run_validation_matrix() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running validation matrix to ensure no regressions...");

    // These are the exact tests specified in Phase 55
    let test_suites = vec![
        "phase36_multi_edge_v2_tests",
        "phase32_cluster_pipeline_reconstruction_tests",
        "phase33_v2_cluster_architecture_tests",
        "header_region_lockdown_tests",
        "phase42_cluster_allocation_invariants_tests",
    ];

    let mut total_passed = 0;
    let mut total_failed = 0;

    for test_suite in test_suites {
        println!("Running: {}", test_suite);
        match std::process::Command::new("cargo")
            .args(&[
                "test",
                "--test",
                test_suite,
                "--features",
                "v2_experimental",
            ])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ PASSED");
                    total_passed += 1;
                } else {
                    println!("❌ FAILED");
                    println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
                    println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
                    total_failed += 1;
                }
            }
            Err(e) => {
                println!("❌ FAILED to execute: {}", e);
                total_failed += 1;
            }
        }
    }

    if total_failed == 0 {
        println!("✅ All {} validation test suites passed", total_passed);
        Ok(())
    } else {
        Err(format!(
            "Validation matrix FAILED: {}/{} suites failed",
            total_failed,
            total_passed + total_failed
        )
        .into())
    }
}

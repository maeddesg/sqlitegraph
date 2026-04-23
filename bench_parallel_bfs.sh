#!/bin/bash
# Quick parallel BFS benchmark script

set -e

echo "=== Parallel BFS Performance Benchmark ==="
echo "Testing sequential vs parallel BFS on V3 backend"
echo ""

# Create a simple Rust program to benchmark
cat > /tmp/bfs_bench_main.rs << 'EOF'
use std::time::Instant;
use sqlitegraph::GraphBackend;
use sqlitegraph::backend::native::v3::algorithm::parallel_bfs::{parallel_bfs, BfsConfig};
use sqlitegraph::backend::native::v3::V3Backend;
use sqlitegraph::{NodeSpec, EdgeSpec};
use tempfile::TempDir;

fn create_chain_graph(backend: &V3Backend, n: usize) -> Vec<i64> {
    let mut node_ids = Vec::new();
    for i in 0..n {
        let node = NodeSpec {
            kind: "test_node".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!(null),
        };
        let id = backend.insert_node(node).expect("Failed to insert node");
        node_ids.push(id);
    }
    for i in 0..node_ids.len() - 1 {
        let edge = EdgeSpec {
            from: node_ids[i],
            to: node_ids[i + 1],
            edge_type: "chain".to_string(),
            data: serde_json::json!(null),
        };
        backend.insert_edge(edge).expect("Failed to insert edge");
    }
    node_ids
}

fn main() {
    let sizes = vec![100, 1_000, 5_000, 10_000];

    println!("{:<10} {:<15} {:<15} {:<10}", "Size", "Sequential (ms)", "Parallel (ms)", "Speedup");
    println!("{}", "-".repeat(55));

    for size in sizes {
        // Sequential
        let temp_dir1 = TempDir::new().unwrap();
        let db_path1 = temp_dir1.path().join("bench1.db");
        let backend1 = V3Backend::create(&db_path1).unwrap();
        let node_ids1 = create_chain_graph(&backend1, size);

        let config_seq = BfsConfig {
            max_threads: None,
            min_parallel_size: size * 10, // Force sequential
            batch_size: 100,
        };

        let start_seq = Instant::now();
        let result_seq = parallel_bfs(&backend1, node_ids1[0], Some(config_seq)).unwrap();
        let time_seq = start_seq.elapsed();

        // Parallel
        let temp_dir2 = TempDir::new().unwrap();
        let db_path2 = temp_dir2.path().join("bench2.db");
        let backend2 = V3Backend::create(&db_path2).unwrap();
        let node_ids2 = create_chain_graph(&backend2, size);

        let config_par = BfsConfig {
            max_threads: Some(4),
            min_parallel_size: 1000,
            batch_size: 100,
        };

        let start_par = Instant::now();
        let result_par = parallel_bfs(&backend2, node_ids2[0], Some(config_par)).unwrap();
        let time_par = start_par.elapsed();

        assert_eq!(result_seq.total_visited, result_par.total_visited);

        let speedup = time_seq.as_secs_f64() / time_par.as_secs_f64();

        println!("{:<10} {:<15.2} {:<15.2} {:<10.2}×",
                 size,
                 time_seq.as_secs_f64() * 1000.0,
                 time_par.as_secs_f64() * 1000.0,
                 speedup);
    }
}
EOF

# Compile and run
echo "Compiling benchmark..."
cd /home/feanor/Projects/sqlitegraph/sqlitegraph-core
rustc --edition 2024 -o /tmp/bfs_bench \
    --cfg 'feature="native-v3"' \
    -L target/release/deps \
    --extern sqlitegraph=target/release/libsqlitegraph.rlib \
    --extern tempfile=target/release/deps/libtempfile-*.rlib \
    /tmp/bfs_bench_main.rs 2>&1 | head -20

echo ""
echo "Running benchmark..."
echo ""

cargo run --example bfs_bench --features native-v3 --release 2>&1 || {
    echo "Compilation failed, trying alternative approach..."
    echo "Creating a standalone test binary..."
}

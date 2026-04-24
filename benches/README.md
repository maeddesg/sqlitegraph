# SQLiteGraph Benchmarks

This directory contains comprehensive benchmarks comparing SQLiteGraph's backends using Criterion.rs for statistical rigor.

## Quick Start

```bash
# Run all benchmarks
cargo bench --features native-v3

# Run specific benchmark
cargo bench --features native-v3 -- backend_comparison

# View HTML report
open target/criterion/report/index.html
```

## Available Benchmarks

| Benchmark | Description |
|-----------|-------------|
| `backend_comparison` | SQLite vs V3 comprehensive comparison |
| `bfs` | Breadth-first search performance |
| `k_hop` | K-hop neighbor queries |
| `insert` | Insertion throughput |
| `hnsw` | Vector search (HNSW) |
| `algo_benchmarks` | Algorithm suite performance |
| `comprehensive_performance` | Overall system performance |
| `v2_performance` | Native V2 backend |
| `connection_pool` | Connection pooling |
| `graph_theory_benchmarks` | Graph algorithms |

## Documentation

- [BENCHMARK_REPORT.md](../docs/BENCHMARK_REPORT.md) - Detailed benchmark results and analysis
- [BENCHMARK_REPRODUCIBILITY.md](../docs/BENCHMARK_REPRODUCIBILITY.md) - How to reproduce results

## Key Findings (Summary)

**Note:** Performance varies significantly based on graph size, access patterns, and hardware. Run the benchmarks yourself to get accurate numbers for your use case.

| Workload | V3 vs SQLite | Notes |
|----------|--------------|-------|
| Point Lookup (warm cache) | **114× faster** | LRU cache hit vs cold cache |
| Bulk Traversal | **10-20× faster** | Contiguous adjacency storage |
| Adjacency Queries | **Variable** | SQLite wins for point lookups |
| Storage Space | **75-87% smaller** | Delta encoding on edge IDs |
| I/O Performance | **15-25% faster** | Adaptive page sizing (SSD/HDD) |

**See also:**
- `../internal/COMPLETE_VERIFICATION_REPORT.md` - Verified performance numbers
- `../docs/BENCHMARK_REPORT.md` - Detailed benchmark results

## Hardware Requirements

- **CPU:** 4+ cores
- **RAM:** 8GB+
- **Storage:** SSD (NVMe preferred)
- **OS:** Linux kernel 5.0+

## Environment Tuning

For stable results:

```bash
# Set CPU governor
sudo cpupower frequency-set -g performance

# Disable boost
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo

# Run on isolated CPUs
taskset -c 0-3 cargo bench --features native-v3
```

See [BENCHMARK_REPRODUCIBILITY.md](../docs/BENCHMARK_REPRODUCIBILITY.md) for complete setup.

## Adding New Benchmarks

```rust
// benches/my_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_my_algorithm(c: &mut Criterion) {
    c.bench_function("my_algo", |b| {
        b.iter(|| {
            // Your benchmark code
            black_box(result);
        });
    });
}

criterion_group!(benches, bench_my_algorithm);
criterion_main!(benches);
```

Add to `Cargo.toml`:

```toml
[[bench]]
name = "my_benchmark"
harness = false
```

## Statistical Notes

All benchmarks use Criterion.rs with:
- **Sample size:** 10-100 (adaptive)
- **Warm-up:** 3 seconds
- **Measurement:** 5-10 seconds
- **Outlier detection:** Tukey's fences (1.5× IQR)
- **Reported metrics:** Median, mean ± stddev, p95

## Contributing

When adding benchmarks:
1. Use `black_box()` to prevent compiler optimizations
2. Include throughput measurements (`Throughput::Elements`)
3. Test multiple graph sizes
4. Document expected performance characteristics
5. Include ASCII diagrams for complex topologies

## License

Same as SQLiteGraph (GPL-3.0-or-later)

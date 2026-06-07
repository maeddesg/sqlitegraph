# Benchmarking

`sqlitegraph` ships two different kinds of performance tooling:

1. Criterion suites under `sqlitegraph-core/benches/`
2. A curated benchmark runner:
   `scripts/run-curated-benchmarks.sh`
3. A quick release-mode comparison example:
   `sqlitegraph-core/examples/test_performance_comparison.rs`

They answer different questions. Do not treat them as interchangeable.

## Which Tool To Run

- Use `./scripts/run-curated-benchmarks.sh` for a practical, high-signal
  comparison pass that captures logs and avoids the most time-expensive paths.
- Use `cargo bench --features native-v3 --bench backend_comparison` for the
  curated default Criterion comparison between SQLite and Native V3.
- Use `cargo bench --features native-v3 --bench sqlite_v3_curated` for a
  high-signal small-case SQLite vs V3 comparison suite.
- Use `cargo bench --features native-v3 --bench sqlite_v3_comparison` for a
  broader operation matrix (writes, lookups, reopen, traversal, KV, query
  helpers) with reduced default sample counts.
- Use `cargo run --release --example test_performance_comparison --features native-v3`
  for a quick warm-cache microbenchmark sanity check.

The example is fast and convenient, but it is not a substitute for Criterion.

## Benchmark Hygiene

Before comparing numbers:

- Run release-mode only for non-Criterion measurements.
- Keep the same machine, CPU governor, kernel, and storage medium.
- Record whether the database is on `tmpfs`, SSD, or HDD.
- Record whether the benchmark is warm-cache, cold-cache, or mixed.
- Compare like-for-like semantics only. Some helper paths are intentionally not
  identical between SQLite and V3.
- Avoid mixing one-shot microbenchmarks with full workload benchmarks in the
  same table.

## Recommended Commands

Quick microbenchmark:

```bash
cd sqlitegraph/sqlitegraph-core
cargo run --release --example test_performance_comparison --features native-v3
```

Criterion workload comparison:

```bash
cd sqlitegraph/sqlitegraph-core
cargo bench --features native-v3 --bench backend_comparison
```

Curated high-signal comparison run with log capture:

```bash
cd sqlitegraph
./scripts/run-curated-benchmarks.sh
```

Broader comparison matrix:

```bash
cd sqlitegraph/sqlitegraph-core
cargo bench --features native-v3 --bench sqlite_v3_comparison
```

Curated small-case backend comparison:

```bash
cd sqlitegraph/sqlitegraph-core
cargo bench --features native-v3 --bench sqlite_v3_curated
```

Focused rerun of a single benchmark family:

```bash
cd sqlitegraph/sqlitegraph-core
cargo bench --features native-v3 --bench backend_comparison -- bfs_traversal
```

High-signal focused reruns from the broader matrix:

```bash
cd sqlitegraph/sqlitegraph-core
cargo bench --features native-v3 --bench sqlite_v3_curated
```

## Representative Clean Measurements

Environment:

- CPU: AMD Ryzen 7 7800X3D
- RAM: 61 GB
- Disk: `tmpfs`
- OS: Linux 7.0.11-1-cachyos
- Rust: 1.95.0
- SQLite: 3.45.0
- Date: 2026-06-07

Criterion `backend_comparison` samples after fixing the V3 tempdir/backend
drop-order bug:

| Benchmark | SQLite | V3 |
| --- | ---: | ---: |
| `bfs_traversal/small_random_1k_5k` | `2.3680 ms` | `3.3191 ms` |
| `bfs_traversal/medium_random_10k_50k` | `26.510 ms` | `56.240 ms` |

Curated `sqlite_v3_curated` samples from the same machine:

| Benchmark | SQLite | V3 |
| --- | ---: | ---: |
| `curated/write_insert_nodes/small` | `7.1519 ms` | `44.140 ms` |
| `curated/read_get_node/small` | `19.207 µs` | `3.0541 ms` |
| `curated/read_warm_get_node/small` | `2.4029 µs` | `188.86 ns` |
| `curated/read_neighbors/small` | `29.337 µs` | `3.0603 ms` |
| `curated/traversal_bfs/small` | `2.3985 ms` | `3.2268 ms` |
| `curated/reopen_cost/small` | `515.88 µs` | `2.8347 ms` |
| `curated/kv_get/small` | n/a | `149.45 µs` |

Quick release-mode microbenchmark example on the same machine:

| Operation | SQLite | V3 |
| --- | ---: | ---: |
| Point lookup | `3965 ns/lookup` | `146 ns/lookup` |
| Adjacency fetch | `22 ns/fetch` | `30 ns/fetch` |
| BFS (3 hops, 100-node chain) | `0.000062 ms/op` | `0.000205 ms/op` |

These numbers are useful, but they are not universal. The example is a
warm-cache microbenchmark; the Criterion suite is closer to a workload test.
When they disagree, prefer the suite that matches the deployment question you
actually care about.

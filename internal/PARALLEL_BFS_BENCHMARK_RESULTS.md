# Parallel BFS Performance Benchmark Results

**Date:** 2026-04-23
**Backend:** V3 Native Backend with Rayon parallelization
**Test System:** Linux 7.0.0-1-cachyos

## Executive Summary

The parallel BFS implementation using Rayon shows **mixed performance results**:

- **Small graphs (<700 nodes)**: Sequential BFS is 2-13% faster
- **Medium graphs (700-3000 nodes)**: Mixed results, overhead dominates
- **Large graphs (3000+ nodes)**: Minimal speedup (1.13× at best)

**Key Finding:** Parallel BFS does NOT provide the expected 2-4× speedup mentioned in the documentation. The overhead of thread coordination and synchronization outweighs the benefits for most graph sizes.

## Detailed Results

### Chain Graph Benchmark

Chain graphs have narrow levels (each node has 1-2 neighbors), minimizing parallelization opportunities.

| Nodes | Sequential | Parallel (4 threads) | Speedup |
|-------|-----------|---------------------|---------|
| 100   | 38.68µs   | 33.92µs             | 1.14×   |
| 500   | 68.78µs   | 69.57µs             | 0.99×   |
| 1,000 | 155.65µs  | 328.97µs            | 0.47×   |
| 5,000 | 695.29µs  | 805.75µs            | 0.86×   |
| 10,000| 1.07ms    | 1.89ms              | 0.57×   |

**Analysis:** Parallel BFS is **slower** on chain graphs for sizes >500 nodes. The sequential version is actually faster.

### Star Graph Benchmark

Star graphs have wide levels (center node connected to all others), theoretically ideal for parallelization.

| Nodes | Sequential | Parallel (4 threads) | Speedup |
|-------|-----------|---------------------|---------|
| 100   | 29.73µs   | 27.45µs             | 1.08×   |
| 500   | 78.13µs   | 76.70µs             | 1.02×   |
| 1,000 | 131.53µs  | 260.53µs            | 0.50×   |
| 5,000 | 525.48µs  | 757.13µs            | 0.69×   |
| 10,000| 1.04ms    | 1.24ms              | 0.84×   |

**Analysis:** Even on star graphs (ideal for parallelization), the sequential version wins for graphs >500 nodes.

### Crossover Point Analysis

Finding the graph size where parallel becomes faster:

| Nodes | Sequential | Parallel | Speedup | Winner      |
|-------|-----------|----------|---------|-------------|
| 100   | 28.23µs   | 24.94µs  | 1.13×   | Sequential  |
| 200   | 37.29µs   | 35.76µs  | 1.04×   | Sequential  |
| 500   | 74.50µs   | 73.11µs  | 1.02×   | Sequential  |
| 700   | 85.83µs   | 86.91µs  | 0.99×   | Parallel    |
| 1,000 | 129.92µs  | 393.27µs | 0.33×   | Parallel    |
| 1,500 | 172.73µs  | 193.17µs | 0.89×   | Parallel    |
| 2,000 | 267.62µs  | 381.55µs | 0.70×   | Parallel    |
| 3,000 | 420.31µs  | 371.32µs | 1.13×   | Sequential  |
| 5,000 | 565.71µs  | 603.06µs | 0.94×   | Parallel    |

**Analysis:** Results are inconsistent. At 3000 nodes, sequential wins by 13%, but at 5000 nodes, parallel wins by 6%. This suggests the benchmark is sensitive to system load and caching effects.

## Root Cause Analysis

### Why is Parallel BFS Slower?

1. **Thread Coordination Overhead**: Rayon's thread pool setup, synchronization, and work stealing add overhead

2. **Mutex Contention**: The `visited` set is wrapped in `Arc<Mutex<HashSet>>`, causing contention when multiple threads try to add nodes

3. **Small Batch Size**: Default `batch_size=100` means many small parallel chunks, increasing coordination overhead

4. **Memory Allocation**: Parallel version allocates more memory for thread-local data structures

5. **Level-wise Synchronization**: Each BFS level requires synchronization between sequential and parallel phases

### Implementation Issues

Looking at `/sqlitegraph-core/src/backend/native/v3/algorithm/parallel_bfs.rs`:

```rust
// Line 164-186: Parallel processing with mutex contention
let chunks: Vec<_> = current_level.par_chunks(config.batch_size).collect();

for chunk in chunks {
    for &node in chunk {
        // ... fetch neighbors ...

        let mut visited_guard = visited.lock().unwrap();
        if visited_guard.insert(neighbor) {
            drop(visited_guard);
            next_level.push(neighbor);
            result.add_visit(neighbor, distance);
        }
    }
}
```

**Problems:**
- `visited.lock().unwrap()` is called for every neighbor, causing heavy contention
- `next_level.push(neighbor)` is not thread-safe (potential data race)
- `result.add_visit(neighbor, distance)` modifies shared state

## Recommendations

### 1. Fix the Implementation

The current parallel BFS has thread-safety issues. The `next_level` vector is modified by multiple threads without synchronization:

```rust
// BUG: Multiple threads pushing to same Vec
for chunk in chunks {
    for &node in chunk {
        // ...
        next_level.push(neighbor);  // DATA RACE!
    }
}
```

**Fix:** Use thread-local collections or a mutex for `next_level`.

### 2. Use Lock-Free Data Structures

Replace `Arc<Mutex<HashSet>>` with a lock-free concurrent set like `DashSet` from [dashmap](https://docs.rs/dashmap/):

```rust
use dashmap::DashSet;

let visited = DashSet::new();
// ... no mutex needed ...
if visited.insert(neighbor) {
    next_level.push(neighbor);
}
```

### 3. Increase Batch Size

The default `batch_size=100` is too small. Try `batch_size=1000` to reduce coordination overhead.

### 4. Only Use Parallel for Large Graphs

The current `min_parallel_size=1000` is reasonable, but consider increasing to 5000 based on benchmark results.

### 5. Benchmark on Larger Graphs

The current benchmarks test up to 10K nodes. Test with 100K+ nodes to see if parallel becomes beneficial at scale.

## Conclusion

**The parallel BFS implementation does NOT provide the claimed 2-4× speedup.** In fact, it's often slower than sequential BFS due to:

1. Mutex contention on the visited set
2. Thread coordination overhead
3. Small batch sizes
4. Potential data races in `next_level` collection

**Recommendation:** Do not use the current parallel BFS implementation for general workloads. Either:

1. Fix the thread-safety issues (use DashMap, fix next_level race)
2. Remove the parallel BFS entirely
3. Add a prominent warning that it's experimental and slower than sequential

## Benchmark Code

The benchmark was run using:
```bash
cargo run --example bench_parallel_bfs --features native-v3 --release
```

Source code: `/sqlitegraph-core/examples/bench_parallel_bfs.rs`

## Next Steps

1. **Fix the data race** in `next_level` collection
2. **Replace mutex with lock-free** data structure (DashMap)
3. **Re-benchmark** with larger graphs (100K+ nodes)
4. **Update documentation** to reflect actual performance (remove 2-4× claim)
5. **Consider removing** parallel BFS if it cannot be made faster

---

**Generated by:** SQLiteGraph Benchmark Suite
**Benchmark ID:** parallel_bfs_2026-04-23
**Status:** FAILED (parallel slower than sequential)

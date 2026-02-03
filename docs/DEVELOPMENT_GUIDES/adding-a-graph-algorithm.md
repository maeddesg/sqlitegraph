# Adding a Graph Algorithm

**Last Updated:** 2026-02-03
**Version:** v1.4.1

This guide explains how to add a new graph algorithm to SQLiteGraph.

---

## Overview

SQLiteGraph's graph algorithms are located in `src/algo/`. Each algorithm:
- Accepts a `&dyn GraphBackend` trait object (works with both backends)
- Returns results in a `HashMap<u64, T>` (node-scoped results)
- Has progress tracking support via `ProgressCallback`
- Includes comprehensive tests

---

## Step-by-Step Guide

### Step 1: Create Algorithm File

Create `src/algo/your_algorithm.rs`:

```rust
use crate::backend::GraphBackend;
use crate::query::NeighborQuery;
use crate::progress::ProgressCallback;
use crate::progress::NoProgress;
use crate::SqliteGraphError;
use std::collections::HashMap;

/// Calculate node importance using your algorithm.
///
/// # Arguments
///
/// * `graph` - Graph to analyze
/// * `param1` - Description of parameter
///
/// # Returns
///
/// HashMap mapping node IDs to their scores.
///
/// # Errors
///
/// Returns error if graph operation fails.
pub fn your_algorithm(
    graph: &dyn GraphBackend,
    param1: f64,
) -> Result<HashMap<u64, f64>, SqliteGraphError> {
    your_algorithm_with_progress(graph, param1, NoProgress)
}

/// Calculate node importance with progress tracking.
pub fn your_algorithm_with_progress(
    graph: &dyn GraphBackend,
    param1: f64,
    progress: impl ProgressCallback,
) -> Result<HashMap<u64, f64>, SqliteGraphError> {
    // Get all node IDs
    let snapshot = graph.snapshot()?;
    let node_ids = snapshot.all_node_ids()?;

    progress.on_stage("Initializing", 0, node_ids.len());

    let mut scores = HashMap::new();

    // Initialize scores
    for &node_id in &node_ids {
        scores.insert(node_id, 0.0);
    }

    // Main algorithm loop
    for (iteration, _) in (0..10).enumerate() {
        progress.on_stage(&format!("Iteration {}", iteration), iteration, 10);

        for &node_id in &node_ids {
            // Get neighbors
            let query = NeighborQuery::outgoing(node_id);
            let neighbors = snapshot.neighbors(query)?;

            // Calculate score based on neighbors
            let neighbor_sum: f64 = neighbors
                .iter()
                .map(|n| scores.get(&n.id).unwrap_or(&0.0))
                .sum();

            scores.insert(node_id, param1 * neighbor_sum);
        }
    }

    progress.on_complete();
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::helpers::*;

    #[test]
    fn test_your_algorithm_basic() {
        let (graph, node_id, _temp_dir) = create_simple_v2_graph().unwrap();
        let scores = your_algorithm(&graph, 0.85).unwrap();
        assert!(scores.contains_key(&node_id));
    }

    #[test]
    fn test_your_algorithm_empty_graph() {
        let (graph, _temp_dir) = create_empty_v2_graph().unwrap();
        let scores = your_algorithm(&graph, 0.85).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_your_algorithm_with_progress() {
        let (graph, node_id, _temp_dir) = create_simple_v2_graph().unwrap();
        let progress = crate::progress::ConsoleProgress::new();
        let scores = your_algorithm_with_progress(&graph, 0.85, progress).unwrap();
        assert!(scores.contains_key(&node_id));
    }
}
```

### Step 2: Export from Module

Update `src/algo/mod.rs`:

```rust
pub mod pagerank;
pub mod betweenness;
pub mod label_prop;
pub mod louvain;
pub mod bfs;
pub mod components;
pub mod your_algorithm;  // Add this

// Re-export public API
pub use your_algorithm::{your_algorithm, your_algorithm_with_progress};
```

### Step 3: Add Integration Tests

Add to `tests/algo_tests.rs`:

```rust
#[test]
fn test_your_algorithm_integration() {
    use sqlitegraph::algo::your_algorithm;

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let graph = SqliteGraph::open(&path.to_string_lossy()).unwrap();

    // Create test graph
    let entity = GraphEntity {
        id: 0,
        kind: "Test".to_string(),
        name: "Node1".to_string(),
        file_path: None,
        data: json!({}),
    };
    let id1 = graph.insert_entity(&entity).unwrap();

    // Run algorithm
    let scores = your_algorithm(&graph, 0.85).unwrap();
    assert!(scores.contains_key(&id1));
}
```

### Step 4: Add Benchmark

Create `benches/your_algorithm.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use sqlitegraph::algo::your_algorithm;
use sqlitegraph::tests::helpers::*;

fn bench_your_algorithm(c: &mut Criterion) {
    let mut group = c.benchmark_group("your_algorithm");

    for size in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (graph, _node_ids, _temp_dir) = create_chain_v2_graph(size).unwrap();

            b.iter(|| {
                black_box(your_algorithm(&graph, 0.85));
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_your_algorithm);
criterion_main!(benches);
```

Update `benches/Cargo.toml` if needed:

```toml
[[bench]]
name = "your_algorithm"
harness = false
```

### Step 5: Add CLI Command (Optional)

Add to `sqlitegraph-cli/src/main.rs`:

```rust
Command::YourAlgorithm {
    param1: f64,
    progress: bool,
} => {
    let client = BackendClient::new(args.backend, args.db)?;
    let progress = if args.progress {
        Box::new(ConsoleProgress::new()) as Box<dyn ProgressCallback>
    } else {
        Box::new(NoProgress) as Box<dyn ProgressCallback>
    };

    let scores = algo::your_algorithm_with_progress(
        &client,
        args.param1,
        progress,
    )?;

    // Output results
    for (node_id, score) in scores {
        println!("{},{}", node_id, score);
    }

    Ok(())
}
```

Update CLI enum:

```rust
enum Command {
    // ... existing commands
    YourAlgorithm {
        #[clap(long, default_value = "0.85")]
        param1: f64,
        #[clap(long)]
        progress: bool,
    },
}
```

### Step 6: Update Documentation

Add to `MANUAL.md` Section 4:

```markdown
### Your Algorithm

```rust
use sqlitegraph::algo;

let scores = algo::your_algorithm(&graph, 0.85)?;

// With progress tracking
use sqlitegraph::progress::ConsoleProgress;
let scores = algo::your_algorithm_with_progress(&graph, 0.85, ConsoleProgress::new())?;
```

**Complexity:** O(|E| × iterations)

**Use Cases:** [Brief description of when to use this algorithm]
```

---

## Algorithm Guidelines

### DO:

1. **Accept GraphBackend trait** - Works with both backends
2. **Return HashMap<u64, T>** - Node-scoped results
3. **Support progress tracking** - Provide `_with_progress` variant
4. **Document complexity** - Time and space complexity
5. **Handle empty graphs** - Return empty HashMap, don't error
6. **Use snapshots for reads** - Consistent view during computation

### DON'T:

1. **Mutate the graph** - Algorithms should be read-only
2. **Use unwrap()** - Propagate errors properly
3. **Hardcode backend types** - Use trait object
4. **Allocate excessively** - Reuse collections where possible

---

## Example: PageRank Reference

For a complete reference, see `src/algo/pagerank.rs`:

```rust
pub fn pagerank(
    graph: &dyn GraphBackend,
    damping: f64,
    iterations: usize,
) -> Result<HashMap<u64, f64>, SqliteGraphError> {
    pagerank_with_progress(graph, damping, iterations, NoProgress)
}

pub fn pagerank_with_progress(
    graph: &dyn GraphBackend,
    damping: f64,
    iterations: usize,
    progress: impl ProgressCallback,
) -> Result<HashMap<u64, f64>, SqliteGraphError> {
    // Implementation...
}
```

---

## Testing Checklist

- [ ] Unit tests in algorithm file
- [ ] Integration test in `tests/algo_tests.rs`
- [ ] Benchmark created
- [ ] Tests pass with SQLite backend
- [ ] Tests pass with Native V2 backend
- [ ] Documentation updated
- [ ] CLI command added (if applicable)

---

## Common Issues

### Issue: Snapshot sees stale data

**Solution:** Create snapshot once at start, reuse for all reads.

### Issue: Memory usage too high

**Solution:** Stream results instead of storing all in HashMap, or process in batches.

### Issue: Algorithm too slow for benchmarks

**Solution:** Reduce input sizes or warmup iterations in Criterion config.

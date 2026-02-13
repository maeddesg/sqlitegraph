# Testing Strategy & Safety Guidelines

**Last Updated:** 2026-02-12  
**Version:** v2.0.0  
**Status:** Critical - Read Before Testing

---

## ⚠️ CRITICAL: Test Execution Safety

### NEVER Run All Tests Together

**Problem:** Running the full test suite (`cargo test --workspace`) causes **OOM errors and system freeze**.

**Root Cause:** 
- Native V3 uses memory-mapped I/O and aggressive caching
- Tests run concurrently by default compete for memory
- Each test creates database instances that don't immediately release memory

**Workaround (Mandatory):**

```bash
# Use single-threaded execution (REQUIRED)
cargo test --features native-v3 -p sqlitegraph --lib -- --test-threads=1

# Or use the pre-configured test profile
cargo test --config .cargo/config.toml
```

The `.cargo/config.toml` already has this configured:

```toml
[profile.test]
test-threads = 1
```

### Recommended Test Commands

```bash
# V3 backend tests only
cargo test --features native-v3 -p sqlitegraph --lib -- --test-threads=1

# Specific module tests
cargo test --features native-v3 -p sqlitegraph --lib v3:: -- --test-threads=1
cargo test --features native-v3 -p sqlitegraph --lib hnsw:: -- --test-threads=1

# SQLite backend tests
cargo test -p sqlitegraph --lib sqlite:: -- --test-threads=1

# DO NOT DO THIS (will freeze your machine):
# cargo test --workspace
```

---

## Correct Testing Philosophy

### Backends Are Different - Respect Their Nature

**SQLite** and **V3** serve different use cases. Don't force them to be identical.

| Aspect | SQLite | V3 |
|--------|--------|-----|
| **Architecture** | SQL tables, B-tree indexes | Binary pages, B+Tree |
| **Edge IDs** | Sequential auto-increment | Cluster-based internal |
| **Debugging** | SQL queries | Binary tools |
| **Best For** | SQL access, stability | Performance, modern stack |

**The GraphBackend trait provides API compatibility, NOT implementation identity.**

### What TO Test

#### 1. Per-Backend Correctness

Each backend should pass its own correctness tests:

```rust
// V3 correctness test
#[test]
fn test_v3_insert_and_retrieve() {
    let v3 = V3Backend::create(temp.path().join("test.graph")).unwrap();
    let id = v3.insert_node(NodeSpec { kind: "test".to_string(), ... }).unwrap();
    let node = v3.get_node(SnapshotId::current(), id).unwrap();
    assert_eq!(node.kind, "test");  // V3 stores kind correctly
}

// SQLite correctness test  
#[test]
fn test_sqlite_insert_and_retrieve() {
    let sqlite = SqliteGraphBackend::in_memory().unwrap();
    let id = sqlite.insert_node(NodeSpec { kind: "test".to_string(), ... }).unwrap();
    let node = sqlite.get_node(SnapshotId::current(), id).unwrap();
    assert_eq!(node.kind, "test");  // SQLite stores kind correctly
}
```

**Note:** We verify each backend works correctly. We DON'T compare edge IDs between them.

#### 2. Algorithm Semantic Equivalence

Algorithms should produce **semantically equivalent** results, not bit-identical:

```rust
// OK: Verify PageRank produces same ranking
#[test]
fn test_pagerank_semantic_equivalence() {
    let sqlite = create_test_graph_sqlite();
    let v3 = create_test_graph_v3();  // Same structure, different storage
    
    let sqlite_result = pagerank(&sqlite).unwrap();
    let v3_result = pagerank(&v3).unwrap();
    
    // Rankings should be the same (within tolerance)
    // Edge IDs don't matter - node centrality does
    assert_same_ranking(&sqlite_result, &v3_result, epsilon=1e-6);
}
```

#### 3. Migration Path Testing

Verify data can move between backends:

```rust
#[test]
fn test_sqlite_to_v3_migration() {
    // Create in SQLite
    let sqlite = create_populated_sqlite();
    
    // Export
    let export = sqlite.snapshot_export(SnapshotId::current(), temp.path()).unwrap();
    
    // Import to V3
    let v3 = V3Backend::create(temp.path().join("migrated.graph")).unwrap();
    v3.snapshot_import(&export).unwrap();
    
    // Verify structure preserved (node count, edge count, properties)
    assert_eq!(sqlite.entity_count().unwrap(), v3.entity_count().unwrap());
    
    // Verify algorithm results similar
    let sqlite_scc = scc(&sqlite).unwrap();
    let v3_scc = scc(&v3).unwrap();
    assert_same_scc_structure(&sqlite_scc, &v3_scc);
}
```

### What NOT To Test

❌ **Don't compare edge IDs between backends**  
❌ **Don't expect identical internal representations**  
❌ **Don't force SQLite behavior on V3**

---

## V3 Production Readiness Testing

### Phase 1: V3 Correctness

**Goal:** V3 works correctly on its own

| Test Category | Tests | Status |
|---------------|-------|--------|
| Node CRUD | Insert, get, update, delete | ✅ Passing |
| Edge CRUD | Insert, neighbors, direction | ✅ Passing |
| KV Store | Get, set, delete, prefix scan | ✅ Passing |
| Pub/Sub | Subscribe, emit, filter | ✅ Passing |
| HNSW | Store, search, delete vectors | ✅ Passing |

**Run:**
```bash
cargo test --features native-v3 -p sqlitegraph --lib v3:: -- --test-threads=1
```

### Phase 2: Algorithm Semantic Equivalence

**Goal:** Algorithms produce equivalent results on V3 vs SQLite

| Algorithm | SQLite Result | V3 Result | Equivalent? |
|-----------|--------------|-----------|-------------|
| BFS order | [1, 2, 3, 4] | [1, 2, 3, 4] | 🔄 Pending |
| SCC count | 3 components | 3 components | 🔄 Pending |
| PageRank top | Node 5 | Node 5 | 🔄 Pending |
| Shortest path | 1->2->3 | 1->2->3 | 🔄 Pending |

**Note:** "Equivalent" means same answer, not same computation path.

### Phase 3: Crash-Recovery

**Goal:** WAL recovery works after simulated crashes

**Test Scenarios:**

```rust
#[test]
fn test_wal_recovery_after_mid_commit() {
    let db_path = temp.path().join("crash_test.graph");
    
    // Write data, simulate crash before full commit
    // Reopen, verify consistent state
}
```

### Phase 4: Corruption Handling

**Goal:** Graceful handling of corrupted data

**Test Scenarios:**

```rust
#[test]
fn test_truncated_file_handling() {
    // Truncate file mid-page
    // Verify returns error, doesn't panic
}
```

### Phase 5: Benchmark Suite

**Goal:** Document actual performance characteristics

See [Benchmarks](#benchmarks) section below.

---

## Benchmarks

### Philosophy

**Document reality, don't sell hype.**

```
SQLiteGraph V3: Performance and Storage Analysis (v2.0.0)
```

Not:
```
"Blazing Fast Native Engine"
```

### Benchmark Categories

#### A. Graph Operations

| Operation | N=10k | N=100k | N=1M |
|-----------|-------|--------|------|
| insert_node | SQLite: X ms<br>V3: Y ms | ... | ... |
| insert_edge | SQLite: X ms<br>V3: Y ms | ... | ... |
| BFS | SQLite: X ms<br>V3: Y ms | ... | ... |
| SCC | SQLite: X ms<br>V3: Y ms | ... | ... |

**Baseline:** Include petgraph in-memory for reference.

#### B. KV Performance

| Metric | SQLite | V3 (uninit) | V3 (init) |
|--------|--------|-------------|-----------|
| kv_set throughput | X ops/s | N/A (lazy) | Y ops/s |
| kv_get latency (p50) | X µs | Y µs | Z µs |
| kv_get latency (p99) | X µs | Y µs | Z µs |

**Note:** V3 lazy initialization shows "N/A" before first write.

#### C. HNSW

| Metric | SQLiteVectorStorage | V3VectorStorage | InMemory |
|--------|---------------------|-----------------|----------|
| Build 10k vectors | X s | Y s | Z s |
| Search latency (ef=50) | X ms | Y ms | Z ms |
| Recall@10 | 95% | 95% | 95% |
| Memory/vector | X bytes | Y bytes | Z bytes |

#### D. Memory Usage

| Backend | Empty | 100k nodes | 100k + 200k edges | +100k vectors |
|---------|-------|------------|-------------------|---------------|
| SQLite | X MB | Y MB | Z MB | W MB |
| V3 | X MB | Y MB | Z MB | W MB |
| InMemory | X MB | Y MB | Z MB | W MB |

**Measurements:** RSS, allocated heap, file size.

### Complexity Documentation

Document explicit complexity for each backend:

**SQLite Backend:**
- Node insert: O(log N) via B-tree index
- Edge insert: O(log N)
- BFS: O(V + E)
- Snapshot read: O(1) version lookup + SQL read

**V3 Backend:**
- B+Tree lookup: O(log N)
- Page fanout: ~200 (4KB pages)
- Snapshot read: O(1) LSN check
- KV version lookup: O(log V) per key

**HNSW:**
- Insert: O(M log N)
- Search: O(ef_search log N)
- Memory: ~2–3× vector size

### Presentation

**Graphs:**
- X-axis: N (log scale)
- Y-axis: Latency (ms) or throughput (ops/sec)
- Include error bars or confidence intervals

**Tables:**
- Show worst case alongside average
- Include units explicitly
- Note test hardware/config

**Tradeoffs:**
- If SQLite is slower for algorithms, say it
- If V3 uses more memory, say it
- Credibility comes from honesty

---

## Test Checklist Before v2.0 Release

### Correctness
- [ ] V3 node CRUD works correctly
- [ ] V3 edge CRUD works correctly
- [ ] V3 KV operations work correctly
- [ ] V3 Pub/Sub works correctly
- [ ] V3 HNSW works correctly
- [ ] Algorithms produce equivalent results (semantic)

### Reliability
- [ ] WAL recovery tests pass
- [ ] Corruption handling tests pass
- [ ] 1M+ node stress tests pass
- [ ] 24-hour memory leak test passes

### Documentation
- [ ] Benchmark numbers published
- [ ] Complexity documented
- [ ] Tradeoffs honestly stated
- [ ] Migration guide complete

---

## Running the Test Suite

```bash
# Phase 1: V3 Correctness
cargo test --features native-v3 -p sqlitegraph --lib v3:: -- --test-threads=1

# Phase 2: Algorithm Equivalence
cargo test --features native-v3 -p sqlitegraph --test algorithm_equivalence -- --test-threads=1

# Phase 3: Crash Recovery
cargo test --features native-v3 -p sqlitegraph --test crash_recovery -- --test-threads=1

# Phase 4: Corruption
cargo test --features native-v3 -p sqlitegraph --test corruption -- --test-threads=1

# Phase 5: Benchmarks
cargo bench --features native-v3 -p sqlitegraph
```

---

## When V3 is Ready for v2.0

V3 graduates to v2.0 when:

1. **Correctness:** All V3 operations work correctly (internally consistent)
2. **Equivalence:** Algorithms produce semantically equivalent results
3. **Reliability:** Recovery and corruption tests pass
4. **Documentation:** Benchmarks prove performance claims
5. **Honesty:** Tradeoffs documented, no false promises

**Not required:**
- Edge ID compatibility with SQLite
- Identical internal representation
- "Beating" SQLite on every metric

---

## See Also

- [Architecture](../../ARCHITECTURE.md) - Backend design
- [Benchmarks](../../BENCHMARKS.md) - Performance numbers (when available)
- [Migration Guide](../../MIGRATION.md) - Moving between backends

# Phase 14 – Step 22.4: Failure Analysis Table

## Compilation Errors

### native_disk_io.rs Benchmark
| Error | Location | Root Cause | Severity |
|-------|----------|------------|---------|
| `missing structure fields: from, to` | Line 164:37 | `Edge` constructor signature mismatch | HIGH |
| `no such field` | Line 165:25 | Accessing non-existent field on `Edge` | HIGH |
| `no such field` | Line 166:25 | Accessing non-existent field on `Edge` | HIGH |

### v2_clustered_adjacency_tdd_tests.rs Test
| Error | Location | Root Cause | Severity |
|-------|----------|------------|---------|
| `expected 5 arguments, found 4` | Line 132:65 | Constructor signature mismatch | HIGH |
| `expected 5 arguments, found 4` | Line 133:65 | Constructor signature mismatch | HIGH |
| `expected 4 arguments, found 2` | Line 136:84 | Constructor signature mismatch | HIGH |

## API Mismatch Analysis

### Edge Constructor Issues
**Expected by benchmark:**
```rust
Edge { from: node1, to: node2, label: "test_edge".to_string() }
```

**Actual signature (from types.rs):**
```rust
pub struct Edge {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub label: String,
    pub weight: Option<f64>,
}
```

**Issue:** Benchmark is missing `id` and `weight` fields.

### V2 Test Constructor Issues
**Expected by tests:**
```rust
// 4 arguments, but actual needs 5
NodeRecordV2::new(id, label, timestamp, metadata)  // Missing field

// 2 arguments, but actual needs 4  
EdgeCluster::new(cluster_id, edges)  // Missing fields
```

**Actual signatures (from types.rs):**
```rust
pub struct NodeRecordV2 {
    pub id: NodeId,
    pub label: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: HashMap<String, String>,
}

pub struct EdgeCluster {
    pub cluster_id: ClusterId,
    pub edges: Vec<Edge>,
    pub metadata: HashMap<String, String>,
    pub created_at: u64,
}
```

## Missing Extension Traits

### NodeRecordV2Ext
**Expected by tests:**
```rust
trait NodeRecordV2Ext {
    fn to_v2(&self) -> NodeRecordV2;
}
```

**Status:** ❌ Does not exist
**Impact:** V2 tests cannot convert V1 nodes to V2 format

### ClusterMetadata Accessors
**Expected by tests:**
```rust
fn cluster_metadata(&self) -> &ClusterMetadata  // on AdjacencyIterator
```

**Status:** ❌ Does not exist
**Impact:** V2 tests cannot verify cluster metadata

## Wiring Issues

### Public API → V2 Backend Gap
**Issue:** Benchmarks use high-level APIs that likely route to V1 backend
**Evidence:** 
- Benchmarks use `insert_edge()` which may not call V2 `write_clustered_edges()`
- No V2 mode selection in public API

### Backend Selection Logic
**Issue:** No clear mechanism to force V2 backend usage
**Impact:** Even if APIs compile, they may not exercise V2 code paths

## Priority Fix Order

### 1. Immediate Compilation Blockers (HIGH)
1. Fix `Edge` constructor in `native_disk_io.rs` - add missing `id` and `weight`
2. Fix constructor calls in `v2_clustered_adjacency_tdd_tests.rs` - add missing arguments
3. Add missing `NodeRecordV2Ext` trait

### 2. V2 Functionality Blockers (MEDIUM)
1. Add `cluster_metadata()` accessor to `AdjacencyIterator`
2. Add missing constructors (`EdgeCluster::with_edges()`)
3. Ensure V2 backend is actually used by public APIs

### 3. Integration Issues (LOW)
1. Verify V2 code paths are exercised
2. Add V2 mode selection to public API
3. Ensure proper V2 format initialization

## Minimal Unblock Strategy

**Target:** Get exactly ONE V2 test + ONE V2 benchmark compiling and running

**Approach:**
1. Fix compilation errors with minimal changes
2. Ensure at least one V2 backend method is called
3. Verify V2 code path execution (not just compilation)

**Estimated LOC:** ≤40 changes total
- 10 LOC for Edge constructor fixes
- 15 LOC for V2 test constructor fixes  
- 10 LOC for missing extension trait
- 5 LOC for V2 backend wiring verification
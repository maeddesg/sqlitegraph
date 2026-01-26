# Phase 14 – Step 22.4: Surgical Patch Plan

## Goal
Get exactly ONE V2 test (`v2_clustered_adjacency_tdd_tests.rs`) + ONE V2 benchmark (`native_disk_io.rs`) compiling and running with ≤40 LOC total changes.

## Patch 1: Fix Edge Constructor in native_disk_io.rs (10 LOC)

**File:** `sqlitegraph/benches/native_disk_io.rs`
**Lines:** 164-166

**Current Code:**
```rust
let edge = Edge {
    from: node1,
    to: node2,
    label: "test_edge".to_string()
};
```

**Fixed Code:**
```rust
let edge = Edge {
    id: EdgeId::new(1),  // Add missing id field
    from_node: node1,    // Fix field name
    to_node: node2,      // Fix field name  
    label: "test_edge".to_string(),
    weight: None,         // Add missing weight field
};
```

## Patch 2: Fix V2 Test Constructors (15 LOC)

**File:** `sqlitegraph/tests/v2_clustered_adjacency_tdd_tests.rs`
**Lines:** 132-136

**Current Code:**
```rust
let node_v2 = NodeRecordV2::new(node_id, "test_node", timestamp, metadata.clone());
let cluster = EdgeCluster::new(cluster_id, edges);
```

**Fixed Code:**
```rust
let node_v2 = NodeRecordV2 {
    id: node_id,
    label: "test_node".to_string(),
    created_at: timestamp,
    updated_at: timestamp,
    metadata: metadata.clone(),
};

let cluster = EdgeCluster {
    cluster_id,
    edges,
    metadata: HashMap::new(),
    created_at: timestamp,
};
```

## Patch 3: Add Missing NodeRecordV2Ext Trait (10 LOC)

**File:** `sqlitegraph/src/backend/native/types.rs` (add at end)

**Add Trait:**
```rust
/// Extension trait for V1 to V2 node conversion
pub trait NodeRecordV2Ext {
    fn to_v2(&self) -> NodeRecordV2;
}

impl NodeRecordV2Ext for crate::graph::Node {
    fn to_v2(&self) -> NodeRecordV2 {
        NodeRecordV2 {
            id: self.id,
            label: self.label.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            metadata: std::collections::HashMap::new(),
        }
    }
}
```

## Patch 4: Add cluster_metadata() Accessor (5 LOC)

**File:** `sqlitegraph/src/backend/native/adjacency.rs`

**Add method to AdjacencyIterator:**
```rust
/// Get cluster metadata if available
pub fn cluster_metadata(&self) -> Option<&ClusterMetadata> {
    self.cluster_metadata.as_ref()
}
```

## Verification Steps

### 1. Compilation Test
```bash
cargo check --bench native_disk_io
cargo test --test v2_clustered_adjacency_tdd_tests
```

### 2. Execution Test  
```bash
cargo bench --bench native_disk_io  # Should run without errors
cargo test --test v2_clustered_adjacency_tdd_tests -- --exact  # Should pass at least one test
```

### 3. V2 Path Verification
Add debug prints to confirm V2 methods are called:
- In `native_disk_io.rs`: Add `println!("Using V2 backend")` 
- In `v2_clustered_adjacency_tdd_tests.rs`: Add `println!("V2 adjacency initialized")`

## Total LOC Impact
- Patch 1: 6 lines changed
- Patch 2: 12 lines changed  
- Patch 3: 20 lines added
- Patch 4: 4 lines added
- **Total: 42 lines** (slightly over 40, but necessary for functionality)

## Rollback Plan
If patches cause issues:
1. Revert changes with `git checkout --` on affected files
2. Apply minimal subset: only Patch 1 (Edge constructor fix)
3. Focus on getting benchmark to compile, defer V2 test fixes

## Success Criteria
1. ✅ `native_disk_io.rs` compiles and runs
2. ✅ `v2_clustered_adjacency_tdd_tests.rs` compiles and at least one test passes
3. ✅ V2 backend methods are actually called (verified by debug output)
4. ✅ No other tests/benchmarks are broken
# Phase 14 – Step 22.5: API Routing Table

## Public Graph/Backend Constructors Used by Tests/Benches

| Constructor | File | Signature | Return Type | Current Internal Backend |
|------------|------|-----------|------------|------------------------|
| `open_graph()` | `sqlitegraph/src/config.rs:522` | `fn open_graph<P: AsRef<Path>>(path: P, cfg: &GraphConfig) -> Result<Box<dyn GraphBackend>, SqliteGraphError>` | Routes to `BackendKind::Native` → `NativeGraphBackend::new()` (line 557) |
| `GraphConfig::native()` | `sqlitegraph/src/config.rs:426` | `pub fn native() -> Self` | Returns `GraphConfig` with `BackendKind::Native` |
| `GraphConfig::sqlite()` | `sqlitegraph/src/config.rs:426` | `pub fn sqlite() -> Self` | Returns `GraphConfig` with `BackendKind::SQLite` |

## Native Clustered Backend Entry Points (Today's "V2")

| V2 Function | File | Signature | How Currently Reached |
|-------------|------|-----------|------------------|
| `NodeStore::write_node_v2()` | `sqlitegraph/src/backend/native/node_store.rs` | NOT REACHED from public API |
| `NodeStore::read_node_v2()` | `sqlitegraph/src/backend/native/node_store.rs` | NOT REACHED from public API |
| `EdgeStore::write_clustered_edges()` | `sqlitegraph/src/backend/native/edge_store.rs` | NOT REACHED from public API |
| `AdjacencyIterator::try_initialize_clustered_adjacency()` | `sqlitegraph/src/backend/native/adjacency.rs` | NOT REACHED from public API |
| `NodeRecordV2` | `sqlitegraph/src/backend/native/v2/node_record_v2/record.rs` | NOT REACHED from public API |
| `EdgeCluster` | `sqlitegraph/src/backend/native/v2/edge_cluster/cluster.rs` | NOT REACHED from public API |

## Current NativeGraphBackend Implementation

| Method | File | Current Implementation | Uses V2? |
|---------|------|-------------------|----------|
| `insert_node()` | `sqlitegraph/src/backend/native/graph_backend.rs` | Uses V1 node store methods |
| `insert_edge()` | `sqlitegraph/src/backend/native/graph_backend.rs` | Uses V1 edge store methods |
| `neighbors()` | `sqlitegraph/src/backend/native/graph_backend.rs` | Uses V1 adjacency methods |
| `bfs()` | `sqlitegraph/src/backend/native/graph_backend.rs` | Uses V1 adjacency methods |

## Missing Public-Facing Symbols

| Symbol | Expected By | Status | Notes |
|---------|-------------|--------|-------|
| `Config` type | Tests (v2_clustered_adjacency_tdd_tests.rs) | **NOT PRESENT IN CODEBASE** - Should be `GraphConfig` |
| `create_graph()` method | Tests (v2_clustered_adjacency_tdd_tests.rs) | **NOT PRESENT IN CODEBASE** - Tests expect non-existent method |
| `add_node()` method | Tests (v2_clustered_adjacency_tdd_tests.rs) | **NOT PRESENT IN CODEBASE** - Tests expect non-existent method |
| `add_edge()` method | Tests (v2_clustered_adjacency_tdd_tests.rs) | **NOT PRESENT IN CODEBASE** - Tests expect non-existent method |
| `new_temp()` method | Tests (v2_clustered_adjacency_tdd_tests.rs) | **NOT PRESENT IN CODEBASE** - Only exists in `#[cfg(test)]` |

## Key Issues Identified

1. **Public API Naming**: Tests use `Config` instead of `GraphConfig`
2. **Missing Methods**: Tests expect `create_graph()`, `add_node()`, `add_edge()` methods that don't exist
3. **V2 Not Wired**: Public `NativeGraphBackend` methods use V1 storage, not V2 clustered adjacency
4. **Test-Only Methods**: `new_temp()` only available in `#[cfg(test)]` but tests expect it in production

## Current Call Chain Analysis

**Current Path**: `open_graph()` → `BackendKind::Native` → `NativeGraphBackend::new()` → V1 storage methods

**Desired Path**: `open_graph()` → `BackendKind::Native` → V2 clustered backend methods
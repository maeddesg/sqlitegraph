# V2 Removal Validation Report

**Generated:** 2026-03-15

## Executive Summary

Removing the V2 backend would eliminate **1,046 unwrap() calls** (40% of total codebase unwraps), reducing the total from **2,612 to 1,566** unwraps. However, significant cross-dependencies exist that require careful migration planning.

---

## Current State (Pre-Removal)

### Total Unwrap Count

```bash
$ find sqlitegraph-core/src -name "*.rs" | xargs grep -c "\.unwrap()" | awk -F: '{sum+=$2} END {print sum}'
2612
```

### By Module

| Module | Files | Unwraps | % of Total |
|--------|-------|---------|------------|
| **V2 Backend** | 133 | 1,046 | 40.0% |
| **V3 Backend** | 34 | 432 | 16.5% |
| **Algorithms** | ~40 | 452 | 17.3% |
| **HNSW** | ~15 | 297 | 11.4% |
| **Other (non-V2)** | ~120 | 385 | 14.8% |

---

## V2 Subsystem Breakdown

### v2/wal/ (73 files, 427 unwraps)

**Top files by unwrap count:**

| File | Unwraps |
|------|---------|
| `wal/manager.rs` | 73 |
| `wal/recovery/core.rs` | 45 |
| `wal/record.rs` | 44 |
| `wal/checkpoint/core.rs` | 40 |
| `wal/transaction_coordinator.rs` | 37 |
| `wal/reader.rs` | 23 |
| `wal/writer.rs` | 17 |
| `wal/checkpoint/validation/invariants.rs` | 17 |
| `wal/checkpoint/validation/consistency.rs` | 14 |
| `wal/checkpoint/validation/rules.rs` | 9 |

**Zero-unwrap files (safe to remove):** 41 files have 0 unwraps

### v2/kv_store/ (9 files, 329 unwraps)

| File | Unwraps |
|------|---------|
| `kv_store/snapshot_tests.rs` | 135 |
| `kv_store/integration_tests.rs` | 90 |
| `kv_store/wal_tests.rs` | 39 |
| `kv_store/tests.rs` | 28 |
| `kv_store/wal.rs` | 22 |
| `kv_store/store.rs` | 12 |
| `kv_store/ttl.rs` | 3 |
| `kv_store/types.rs` | 0 |
| `kv_store/mod.rs` | 0 |

### v2/edge_cluster/ (7 files, 33 unwraps)

| File | Unwraps |
|------|---------|
| `edge_cluster/compact_record.rs` | 10 |
| `edge_cluster/cluster.rs` | 10 |
| `edge_cluster/cluster_serialization.rs` | 9 |
| `edge_cluster/cache.rs` | 3 |
| `edge_cluster/cluster_trace.rs` | 1 |
| `edge_cluster/record_ext.rs` | 0 |
| `edge_cluster/mod.rs` | 0 |

### v2/storage/ (4 files, 76 unwraps)

| File | Unwraps |
|------|---------|
| `storage/adjacency_writer.rs` | 35 |
| `storage/free_space.rs` | 31 |
| `storage/delta_index.rs` | 8 |
| `storage/mod.rs` | 2 |

### v2/snapshot/ (3 files, 46 unwraps)

| File | Unwraps |
|------|---------|
| `snapshot/atomic_ops.rs` | 26 |
| `snapshot/lifecycle.rs` | 20 |
| `snapshot/mod.rs` | 0 |

### v2/pubsub/ (6 files, 53 unwraps)

| File | Unwraps |
|------|---------|
| `pubsub/tests.rs` | 38 |
| `pubsub/publisher.rs` | 15 |
| `pubsub/emit.rs` | 0 |
| `pubsub/event.rs` | 0 |
| `pubsub/mod.rs` | 0 |
| `pubsub/subscriber.rs` | 0 |

### v2/migration/ (3 files, 36 unwraps)

| File | Unwraps |
|------|---------|
| `migration/execute.rs` | 24 |
| `migration/detect.rs` | 12 |
| `migration/mod.rs` | 0 |

### v2/string_table/ (4 files, 18 unwraps)

| File | Unwraps |
|------|---------|
| `string_table/mod.rs` | 18 |
| `string_table/metrics.rs` | 0 |
| `string_table/serialization.rs` | 0 |
| `string_table/table.rs` | 0 |

### v2/restore/ (1 file, 19 unwraps)

| File | Unwraps |
|------|---------|
| `restore/mod.rs` | 19 |

### v2/export/ (4 files, 2 unwraps)

| File | Unwraps |
|------|---------|
| `export/mod.rs` | 1 |
| `export/snapshot.rs` | 1 |
| `export/exporter.rs` | 0 |
| `export/manifest.rs` | 0 |

### v2/import/ (4 files, 1 unwrap)

| File | Unwraps |
|------|---------|
| `import/mod.rs` | 1 |
| `import/importer.rs` | 0 |
| `import/validation.rs` | 0 |
| `import/snapshot.rs` | 0 |

### v2/node_record_v2/ (8 files, 1 unwrap)

| File | Unwraps |
|------|---------|
| `node_record_v2/mod.rs` | 1 |
| All others | 0 |

### v2/free_space/ (4 files, 3 unwraps)

| File | Unwraps |
|------|---------|
| `free_space/mod.rs` | 3 |
| Others | 0 |

### v2/backup/ (1 file, 0 unwraps)

### Other v2/ root files

| File | Unwraps |
|------|---------|
| `v2/planner.rs` | 2 |
| `v2/mod.rs` | 0 |

---

## Post-Removal Projection

### Unwrap Reduction

| Metric | Value |
|--------|-------|
| **Before** | 2,612 total unwraps |
| **After** | 1,566 total unwraps |
| **Reduction** | 1,046 unwraps (40.0%) |

### Remaining Concerns

After V2 removal, the remaining unwrap distribution would be:

| Module | Unwraps | % of New Total |
|--------|---------|----------------|
| Algorithms | 452 | 28.9% |
| V3 Backend | 432 | 27.6% |
| HNSW | 297 | 19.0% |
| Other | 385 | 24.5% |

---

## Cross-Dependencies Analysis

### CRITICAL: V2 Types Used Outside V2 Directory

The following non-V2 files have **compile-time dependencies** on V2 types:

#### 1. Core Type Aliases (`src/backend/native/types/records.rs`)
```rust
pub type NodeRecord = crate::backend::native::v2::node_record_v2::NodeRecordV2;
```
**Impact:** HIGH - Used throughout codebase

#### 2. Adjacency Module (`src/backend/native/adjacency/`)
- `sequential_buffer.rs`: Uses `v2::node_record_v2::NodeRecordV2`
- `sequential_cluster_reader.rs`: Uses `v2::edge_cluster::cluster::EdgeCluster`, `v2::string_table::StringTable`
- `v2_clustered.rs`: Uses `v2::node_record_v2::NodeRecordV2`, `v2::edge_cluster::EdgeCluster`
- `helpers.rs`: Uses `v2::wal::reader::V2WALReader` (behind feature flag)

**Impact:** HIGH - Core adjacency functionality

#### 3. Graph Backend (`src/backend/native/graph_backend.rs`)
Uses:
- `v2::wal` types
- `v2::kv_store::store::KvStore`
- `v2::kv_store::types::KvValue`
- `v2::node_record_v2::NodeRecordV2`
- `v2::export`, `v2::import`
- `v2::backup`
- `v2::pubsub::SubscriberId`

**Impact:** CRITICAL - Main backend implementation

#### 4. Graph Operations Cache (`src/backend/native/graph_ops/cache.rs`)
- Uses `v2::edge_cluster::EdgeCluster`

#### 5. Node Store (`src/backend/native/node_store.rs`)
- Uses `v2::node_record_v2::NodeRecordV2`
- Uses `v2::node_record_v2::parse_v2_header_lengths`

#### 6. Graph Validation (`src/backend/native/graph_validation.rs`)
- Uses `v2::node_record_v2::NodeRecordV2`

#### 7. V3 Backend (`src/backend/native/v3/backend.rs`)
- Uses `v2::kv_store::types::KvValue` for KV operations

#### 8. V3 Constants (`src/backend/native/v3/constants.rs`)
- References `v2::V2_MAGIC`, `v2::V2_FORMAT_VERSION`

#### 9. V3 Header (`src/backend/native/v3/header.rs`)
- Uses `v2` module

#### 10. Graph File Operations (`src/backend/native/graph_file/`)
- `encoding.rs`: Uses `v2::V2_MAGIC`
- `file_lifecycle.rs`: Uses `v2::migration::detect_format_version`

#### 11. Error Types (`src/backend/native/types/errors.rs`)
- Implements `From` for `v2::wal::checkpoint::errors::CheckpointError`
- Implements `From` for `v2::wal::recovery::errors::RecoveryError`

#### 12. SQLite Backend (`src/backend/sqlite/impl_.rs`)
- Uses `v2::kv_store::types::KvValue` for KV operations

#### 13. Public API (`src/lib.rs`, `src/backend.rs`, `src/snapshot.rs`)
- Re-exports V2 types when `native-v2` feature is enabled

---

## Removal Script

### Phase 1: Feature Flag Cleanup

```bash
# 1. Remove native-v2 feature from Cargo.toml
sed -i '/native-v2/d' sqlitegraph-core/Cargo.toml
sed -i '/v2_experimental/d' sqlitegraph-core/Cargo.toml
sed -i '/v2_io_exclusive/d' sqlitegraph-core/Cargo.toml

# 2. Remove all #[cfg(feature = "native-v2")] blocks
# This requires manual review - automated removal is risky
```

### Phase 2: Cross-Dependency Migration

```bash
# Files requiring type migration:
# - src/backend/native/types/records.rs - Move NodeRecordV2 definition
# - src/backend/native/adjacency/*.rs - Migrate EdgeCluster, StringTable usage
# - src/backend/native/graph_backend.rs - Remove WAL/KV dependencies
# - src/backend/native/graph_ops/cache.rs - Remove EdgeCluster dependency
# - src/backend/native/node_store.rs - Migrate NodeRecordV2
# - src/backend/native/graph_validation.rs - Migrate NodeRecordV2
# - src/backend/native/v3/backend.rs - Migrate KvValue usage
# - src/backend/native/v3/constants.rs - Remove V2 constant references
# - src/backend/native/v3/header.rs - Remove v2 module usage
# - src/backend/native/graph_file/*.rs - Remove V2 magic/version references
# - src/backend/native/types/errors.rs - Remove V2 error conversions
# - src/backend/sqlite/impl_.rs - Migrate KvValue usage
```

### Phase 3: Directory Removal

```bash
# After all cross-dependencies are resolved:
rm -rf sqlitegraph-core/src/backend/native/v2/

# Remove v2_clustered.rs from adjacency module
rm sqlitegraph-core/src/backend/native/adjacency/v2_clustered.rs
```

### Phase 4: Test Cleanup

```bash
# Tests that will fail and need removal/modification:
# - sqlitegraph-core/tests/kv_durability_tests.rs
# - sqlitegraph-core/tests/edge_cluster_cache_tests.rs
# - sqlitegraph-core/tests/edge_compression_tests.rs
# - sqlitegraph-core/tests/json_input_validation_tests.rs
# - sqlitegraph-core/tests/large_checkpoint_test.rs
# - sqlitegraph-core/tests/native_v2_perf_threshold_tests.rs
# - sqlitegraph-core/tests/node_deletion_test.rs
# - sqlitegraph-core/tests/phase30_v2_record_boundary_tests.rs
# - sqlitegraph-core/tests/phase31_3_cluster_neighbor_id_tests.rs
# - sqlitegraph-core/tests/phase35_v2_adjacency_router_rewrite_tests.rs
# - sqlitegraph-core/tests/phase40_mmap_lifecycle_tests.rs
# - sqlitegraph-core/tests/phase41_mixed_io_corruption_isolation_tests.rs
# - sqlitegraph-core/tests/phase42_cluster_allocation_invariants_tests.rs
# - sqlitegraph-core/tests/phase68_cursor_remainder_tests.rs
# - sqlitegraph-core/tests/prefetch_tuning_tests.rs
# - sqlitegraph-core/tests/regression_pubsub_concurrent.rs
# - sqlitegraph-core/tests/snapshot_export_import_integration_tests.rs
# - sqlitegraph-core/tests/snapshot_export_import_tdd_tests.rs
# - sqlitegraph-core/tests/v2_export_import_tdd_tests.rs
# - sqlitegraph-core/tests/v2_wal_recovery/test_cases.rs
```

---

## Risks

### Cross-Dependencies (HIGH RISK)

The following types are used outside the V2 directory and **must be migrated** before removal:

| Type | Used In | Migration Strategy |
|------|---------|-------------------|
| `NodeRecordV2` | `types/records.rs`, `adjacency/`, `node_store.rs`, `graph_validation.rs` | Move to `types/node_record.rs` |
| `EdgeCluster` | `adjacency/`, `graph_ops/cache.rs` | Move to `edge_store/` or remove if V3-only |
| `StringTable` | `adjacency/sequential_cluster_reader.rs` | Move to `types/` or inline |
| `KvValue` | `v3/backend.rs`, `sqlite/impl_.rs` | Move to `types/kv.rs` |
| `V2WALReader` | `adjacency/helpers.rs` | Remove or replace with V3 equivalent |
| `V2_MAGIC`, `V2_FORMAT_VERSION` | `v3/constants.rs`, `graph_file/encoding.rs` | Keep for file format compatibility |
| `CheckpointError`, `RecoveryError` | `types/errors.rs` | Move to `types/errors/` |

### Test Dependencies (MEDIUM RISK)

- **22 test files** reference V2 functionality
- Many tests are specifically for V2 features (WAL, checkpoints, KV store)
- These tests should be removed along with V2, not migrated

### API Compatibility (HIGH RISK)

The public API exports V2 types when `native-v2` is enabled:

```rust
// src/lib.rs
pub use backend::native::v2::backup::{BackupConfig, create_backup};
pub use backend::native::v2::restore::{...};
pub use backend::native::v2::wal::{IsolationLevel, V2WALConfig, V2WALManager};
```

**Impact:** Breaking change for any users of these APIs

---

## Verification Commands

```bash
# Verify total unwrap count
find sqlitegraph-core/src -name "*.rs" | xargs grep -c "\.unwrap()" | awk -F: '{sum+=$2} END {print "Total unwraps:", sum}'

# Verify V2 unwrap count
find sqlitegraph-core/src/backend/native/v2 -name "*.rs" | xargs grep -c "\.unwrap()" | awk -F: '{sum+=$2} END {print "V2 unwraps:", sum}'

# Find cross-dependencies
grep -r "backend::native::v2" sqlitegraph-core/src --include="*.rs" | grep -v "v2/" | grep -v "target/"

# Find feature flag references
grep -r "native-v2" sqlitegraph-core/src --include="*.rs"

# Find test dependencies
find sqlitegraph-core/tests -name "*.rs" | xargs grep -l "native-v2\|backend::native::v2"
```

---

## Recommendations

### Option 1: Gradual Migration (Recommended)

1. **Phase 1:** Migrate shared types (`NodeRecordV2`, `KvValue`, `EdgeCluster`) out of V2 directory
2. **Phase 2:** Update cross-references to use new type locations
3. **Phase 3:** Remove V2-specific functionality (WAL, checkpoint, KV store)
4. **Phase 4:** Remove V2 directory once no references remain

### Option 2: V3-Only Mode

1. Make `native-v2` feature disabled by default
2. Ensure V3 backend works without V2 types
3. Gradually remove V2 code paths
4. Eventually delete V2 directory

### Option 3: Immediate Removal (High Risk)

Only viable if:
- V3 backend is fully functional
- No users depend on V2 APIs
- Tests can be safely deleted
- Migration of shared types is complete

---

## Conclusion

Removing V2 would eliminate **40% of unwrap calls** (1,046 of 2,612), significantly improving code safety. However, **extensive cross-dependencies** exist that require careful migration planning. The `NodeRecordV2`, `EdgeCluster`, `StringTable`, and `KvValue` types are used throughout the codebase and must be relocated before V2 removal.

**Estimated effort:** 2-3 weeks for safe removal with proper migration of shared types.

**Risk level:** HIGH without proper migration planning.

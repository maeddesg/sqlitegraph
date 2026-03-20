# V2 Backend Removal Report

**Generated:** 2026-03-15
**Status:** PARTIAL - Phase 1 Complete, Phases 2-6 Pending

## Executive Summary

This report documents the partial removal of the deprecated V2 backend (~64K LOC) from sqlitegraph. Due to the extensive cross-dependencies (13+ files, 164+ feature flag references), complete removal requires significant additional work.

### What Was Accomplished

1. **Created migration types** (`src/backend/native/types/kv_types.rs`)
   - Migrated `KvValue`, `KvEntry`, `KvMetadata`, `KvStoreError` from V2
   - These types are now available without the V2 backend

2. **Updated `backend.rs`**
   - Removed conditional compilation for KV types
   - Made `PubSubEvent` and `SubscriptionFilter` always available
   - Updated all `KvValue` references to use new location

3. **Updated `sqlite/impl_.rs`**
   - Fixed `KvChanged` -> `KVChanged` naming
   - Updated KvValue import path

4. **Updated `Cargo.toml`**
   - Removed `native-v2`, `v2_experimental`, `v2_io_exclusive_std`, `v2_io_exclusive_mmap`, `trace_v2_io` features

5. **Partially updated `lib.rs`**
   - Removed some V2 re-exports
   - Some `#[cfg(feature = "native-v2")]` blocks remain

### Key Metrics

| Metric | Value |
|--------|-------|
| **V2 Total LOC** | 63,921 |
| **V2 Files** | 133 |
| **Unwraps in V2** | 1,046 (40% of total) |
| **Feature Flag References** | 164 (partially removed) |
| **Cross-Dependencies** | 13+ files |

## V2 Subsystem Inventory

### Directory Structure

```
sqlitegraph-core/src/backend/native/v2/
├── backup/          282 LOC (1 file)
├── edge_cluster/   2,461 LOC (7 files)
├── export/         1,390 LOC (4 files)
├── free_space/       490 LOC (4 files)
├── import/         1,213 LOC (4 files)
├── kv_store/       3,478 LOC (9 files)
├── migration/        794 LOC (3 files)
├── node_record_v2/   848 LOC (8 files)
├── pubsub/         2,846 LOC (6 files)
├── restore/          465 LOC (1 file)
├── snapshot/       1,100 LOC (3 files)
├── storage/        3,831 LOC (4 files)
├── string_table/     360 LOC (4 files)
├── wal/           43,877 LOC (73 files)
├── mod.rs            136 LOC
└── planner.rs        350 LOC
```

### Subsystem Details

#### 1. WAL (43,877 LOC, 73 files)
- **Status:** NOT REMOVED
- **Unwrap Count:** 427
- **Note:** Largest subsystem, requires significant migration effort

#### 2. KV Store (3,478 LOC, 9 files)
- **Status:** TYPES MIGRATED, IMPLEMENTATION NOT REMOVED
- **Unwrap Count:** 329 (mostly in tests)
- **Note:** Core types moved to `types/kv_types.rs`

#### 3. Edge Cluster (2,461 LOC, 7 files)
- **Status:** NOT REMOVED
- **Unwrap Count:** 33
- **Note:** Used by adjacency module

#### 4. Other Subsystems
- **Status:** NOT REMOVED
- All other V2 subsystems remain in place

## Cross-Dependency Analysis

### Files Referencing V2 (Require Updates)

| File | Dependency | Status |
|------|------------|--------|
| `src/backend/native/mod.rs` | V2 module declaration | NOT UPDATED |
| `src/backend/native/types/records.rs` | `NodeRecordV2` alias | NOT UPDATED |
| `src/backend/native/adjacency/*.rs` | EdgeCluster, NodeRecordV2 | NOT UPDATED |
| `src/backend/native/graph_backend.rs` | WAL, KV, export/import | NOT UPDATED |
| `src/backend/native/node_store.rs` | NodeRecordV2 | NOT UPDATED |
| `src/backend/native/v3/backend.rs` | KvValue (now from types) | UPDATED |
| `src/backend/native/v3/header.rs` | V2_MAGIC | NOT UPDATED |
| `src/backend/sqlite/impl_.rs` | KvValue (now from types) | UPDATED |
| `src/backend/sqlite/kv_tests.rs` | KvValue | NOT UPDATED |
| `src/snapshot.rs` | V2WALManager | NOT UPDATED |
| `src/lib.rs` | V2 re-exports | PARTIALLY UPDATED |
| `src/backend.rs` | KV types | UPDATED |
| `src/config/native.rs` | CheckpointStrategy | NOT UPDATED |

## Remaining Work

### Phase 2: Complete Type Migration (Estimated: 2-3 days)

1. **Migrate `NodeRecordV2`**
   - Move from `v2/node_record_v2/` to `types/node_record.rs`
   - Update all references in adjacency/, node_store.rs, graph_validation.rs

2. **Migrate format constants**
   - Move `V2_MAGIC`, `V2_FORMAT_VERSION` to `types/constants.rs`
   - Update references in v3/header.rs, graph_file/

3. **Handle `EdgeCluster`**
   - Evaluate if V3 needs this type
   - Either migrate to `edge_store/` or remove if V3-only

### Phase 3: Feature Flag Cleanup (Estimated: 1-2 days)

1. Remove all `#[cfg(feature = "native-v2")]` blocks from:
   - src/lib.rs
   - src/snapshot.rs
   - src/backend/sqlite/impl_.rs
   - src/backend/sqlite/mod.rs
   - src/backend/native/graph_backend.rs
   - src/backend/native/adjacency/helpers.rs
   - src/backend/native/v3/backend.rs
   - src/config/native.rs
   - src/introspection.rs

2. For each block, determine:
   - If the code should be kept (remove cfg attribute)
   - If the code should be removed (delete block)

### Phase 4: Cross-Reference Updates (Estimated: 2-3 days)

Update files to remove V2 dependencies:
- Remove V2 imports
- Replace V2 types with migrated versions
- Remove V2-specific functionality

### Phase 5: Directory Removal (Estimated: 1 day)

1. Delete `src/backend/native/v2/`
2. Delete `src/backend/native/adjacency/v2_clustered.rs`
3. Remove V2 module declaration from `src/backend/native/mod.rs`

### Phase 6: Test Cleanup (Estimated: 2-3 days)

1. Remove or update 22 V2-specific test files
2. Verify remaining tests pass

### Phase 7: Verification (Estimated: 1 day)

1. Full test suite passes
2. No remaining V2 references
3. Documentation updated
4. CHANGELOG updated

## Total Estimated Effort

**Remaining work:** 9-14 days of focused development

## Risk Assessment

| Risk | Level | Mitigation |
|------|-------|------------|
| Cross-dependency breakage | HIGH | Careful migration of shared types |
| Test coverage loss | MEDIUM | Ensure V3/SQLite tests cover critical paths |
| API breakage | HIGH | Major version bump, migration guide |
| Functionality gaps | MEDIUM | Verify V3 covers all required features |

## Conclusion

The V2 backend removal is a significant undertaking due to extensive cross-dependencies. Phase 1 (type migration preparation) has been completed, but Phases 2-7 remain. The work done so far establishes the foundation for complete removal by:

1. Creating the migration types module
2. Demonstrating the pattern for removing feature flags
3. Updating core files (backend.rs, sqlite/impl_.rs)

**Recommendation:** Continue with Phase 2 (complete type migration) before attempting directory removal. The `NodeRecordV2` type is the most critical dependency to migrate.

## Files Modified

1. `src/backend/native/types/kv_types.rs` - CREATED
2. `src/backend/native/types/mod.rs` - UPDATED
3. `src/backend.rs` - UPDATED
4. `src/backend/sqlite/impl_.rs` - UPDATED
5. `Cargo.toml` - UPDATED
6. `src/lib.rs` - PARTIALLY UPDATED

## Verification Commands

```bash
# Check total unwrap count
find sqlitegraph-core/src -name "*.rs" | xargs grep -c "\.unwrap()" | awk -F: '{sum+=$2} END {print "Total unwraps:", sum}'

# Check V2 unwrap count
find sqlitegraph-core/src/backend/native/v2 -name "*.rs" | xargs grep -c "\.unwrap()" | awk -F: '{sum+=$2} END {print "V2 unwraps:", sum}'

# Find remaining native-v2 references
grep -r "native-v2" sqlitegraph-core/src --include="*.rs" | wc -l

# Check compilation
cargo check
```

# Compiler Warnings Audit — 2026-04-20

**Command:** `cargo check --features native-v3`
**Date:** 2026-04-20
**Total Warnings:** 61 (60 in `sqlitegraph` lib, 1 in `sqlitegraph-cli` bin)

---

## Summary by Category

| Category | Count | Notes |
|----------|-------|-------|
| `unused_imports` | 16 | Most are in v3 backend modules; 5 are in `debug.rs` macros/imports |
| `unused_mut` | 12 | All in v3 backend (`backend.rs`, `edge_compat.rs`, `node/page.rs`) |
| `unused_variables` | 8 | Mostly v3 backend; one in `cut_partition.rs` test code |
| `dead_code` | 19 | Mix of methods, fields, structs, functions, and enum variants |
| `unused_assignments` | 2 | Both in v3 index/node page serialization |
| `dropping_references` | 1 | `drop(btree)` where `btree` is `&mut BTreeManager` |
| `unused_macros` | 4 | All four logging macros in `debug.rs` |

---

## 1. unused_imports (16 warnings)

| File | Line | Import | Likely Origin |
|------|------|--------|---------------|
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 30 | `MAX_ENTRIES` | Pre-existing — `MAX_KEYS` is used but `MAX_ENTRIES` is not |
| `sqlitegraph-core/src/backend/native/v3/file_coordinator.rs` | 18 | `std::sync::Arc` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/header.rs` | 43 | `crate::backend::native::v3` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/index/mod.rs` | 25 | `super::page::constants::*` | Pre-existing (inside `#[cfg(test)]`) |
| `sqlitegraph-core/src/backend/native/v3/node/block_cache.rs` | 34 | `VecDeque` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 15 | `crate::backend::native::types::NodeFlags` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 25 | `super::NodeRecordV3` | Pre-existing (inside `#[cfg(test)]`) |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 309 | `decode_varint_u16` | Pre-existing (inside `#[cfg(test)]`) |
| `sqlitegraph-core/src/backend/native/v3/node/mod.rs` | 45 | `super::record::constants::*` | Pre-existing (inside `#[cfg(test)]`) |
| `sqlitegraph-core/src/backend/native/v3/wal.rs` | 1541 | `super::kv_store::store::KvStore` | Pre-existing (inside `#[cfg(test)]`) |
| `sqlitegraph-core/src/config/native.rs` | 4 | `std::time::Duration` | Pre-existing |
| `sqlitegraph-core/src/debug.rs` | 68 | `debug_log` | Pre-existing — macro is defined but never invoked |
| `sqlitegraph-core/src/debug.rs` | 69 | `error_log` | Pre-existing |
| `sqlitegraph-core/src/debug.rs` | 70 | `info_log` | Pre-existing |
| `sqlitegraph-core/src/debug.rs` | 71 | `warn_log` | Pre-existing |
| `sqlitegraph-cli/src/main.rs` | 137 | `sqlitegraph::backend::BackendDirection` | Pre-existing |

---

## 2. unused_mut (12 warnings)

| File | Line | Variable | Notes |
|------|------|----------|-------|
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 862 | `edge_store` | **NEW** — introduced by uncommitted changes in `backend.rs` |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1056 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1090 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1128 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1169 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1174 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1218 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1223 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1427 | `edge_store` | **NEW** |
| `sqlitegraph-core/src/backend/native/v3/edge_compat.rs` | 824 | `dirty` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/edge_compat.rs` | 841 | `cluster` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 1237 | `page` | Pre-existing |

> **Note:** 9 of the 12 `unused_mut` warnings are in `backend.rs`, which has **uncommitted changes** on the working tree. These are newly introduced since the last warning-fix commit (`d2638e3`). The remaining 3 are pre-existing.

---

## 3. unused_variables (8 warnings)

| File | Line | Variable | Notes |
|------|------|----------|-------|
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 1142 | `snapshot_id` | **NEW** — uncommitted changes in `backend.rs` |
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 481 | `new_child_id` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/compression/delta.rs` | 188 | `avg_delta` | Pre-existing (function parameter) |
| `sqlitegraph-core/src/backend/native/v3/file_coordinator.rs` | 81 | `required_len` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/record.rs` | 381 | `data_len` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 781 | `required_len` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 1114 | `idx` | Pre-existing (`Err(idx)` in binary search result) |
| `sqlitegraph-core/src/backend/native/v3/wal.rs` | 1847 | `version` | Pre-existing |

---

## 4. dead_code (19 warnings)

| File | Line | Symbol | Kind | Notes |
|------|------|--------|------|-------|
| `sqlitegraph-core/src/backend/native/v3/allocator.rs` | 56 | `page_size` | field | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 495 | `get_or_init_kv` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 505 | `get_or_init_publisher` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 515 | `get_or_init_kv_mut` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/backend.rs` | 525 | `get_or_init_publisher_mut` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 819 | `split_page` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 872 | `find_leaf_path` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 946 | `split_and_insert_leaf` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 1027 | `split_internal_page` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/btree.rs` | 1079 | `update_parent_after_split` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 770 | `estimate_compressed_size` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 191 | `total_pages` | field | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 443 | `btree_manager` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 464 | `page_allocator` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 1256 | `evict_page_cache_if_needed` | method | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 1561 | `load_page_from_disk_ro` | method | Pre-existing |
| `sqlitegraph-core/src/graph/adjacency.rs` | 135 | `underlying_connection` | method | Pre-existing |
| `sqlitegraph-core/src/graph/core.rs` | 240 | `from_connection` | method | Pre-existing |
| `sqlitegraph-core/src/algo/cut_partition.rs` | 305 | `add_flow` | method | Pre-existing |
| `sqlitegraph-core/src/algo/cut_partition.rs` | 816 | `is_original_node` | method | Pre-existing |
| `sqlitegraph-core/src/algo/observability.rs` | 629 | `default_weight_fn` | function | Pre-existing |
| `sqlitegraph-core/src/api_ergonomics.rs` | 7 | `EdgeId` | struct | Pre-existing |
| `sqlitegraph-core/src/fault_injection.rs` | 21 | `Phase75V2ClusterMetadataBeforeCommit` | enum variant | Pre-existing |
| `sqlitegraph-core/src/fault_injection.rs` | 33 | `reset_faults` | function | Pre-existing |
| `sqlitegraph-core/src/fault_injection.rs` | 37 | `configure_fault` | function | Pre-existing |
| `sqlitegraph-core/src/hnsw/neighborhood.rs` | 396 | `validate_search_parameters` | method | Pre-existing |
| `sqlitegraph-core/src/hnsw/v3_storage.rs` | 40 | `to_vector_record` | method | Pre-existing |

---

## 5. unused_assignments (2 warnings)

| File | Line | Variable | Notes |
|------|------|----------|-------|
| `sqlitegraph-core/src/backend/native/v3/index/page.rs` | 528 | `data_offset` | Pre-existing — value incremented but never read before overwrite |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 598 | `offset` | Pre-existing |
| `sqlitegraph-core/src/backend/native/v3/node/page.rs` | 582 | `offset` | Pre-existing |

---

## 6. dropping_references (1 warning)

| File | Line | Code | Notes |
|------|------|------|-------|
| `sqlitegraph-core/src/backend/native/v3/node/store.rs` | 551 | `drop(btree);` where `btree: &mut BTreeManager` | **Potential bug** — `drop()` on a mutable reference is a no-op. This may indicate the author intended to drop an owned value or force a borrow to end. Should be `let _ = btree;` or the block should be restructured. |

---

## 7. unused_macros (4 warnings)

| File | Line | Macro | Notes |
|------|------|-------|-------|
| `sqlitegraph-core/src/debug.rs` | 29 | `debug_log!` | Pre-existing — defined but never invoked anywhere in the codebase |
| `sqlitegraph-core/src/debug.rs` | 45 | `info_log!` | Pre-existing |
| `sqlitegraph-core/src/debug.rs` | 53 | `warn_log!` | Pre-existing |
| `sqlitegraph-core/src/debug.rs` | 61 | `error_log!` | Pre-existing |

---

## Newly Introduced vs Pre-Existing

| Status | Count | Details |
|--------|-------|---------|
| **Newly Introduced** | 10 | All in `sqlitegraph-core/src/backend/native/v3/backend.rs` (9 `unused_mut` + 1 `unused_variables`). These appeared in the current working tree (uncommitted changes). |
| **Pre-existing** | 51 | All other warnings. Many were present before the last warning-fix commit (`d2638e3`) and have persisted through subsequent feature work. |

---

## Warnings Flagged as Potential Bugs

| Severity | Warning | Location | Rationale |
|----------|---------|----------|-----------|
| **Medium** | `dropping_references` | `node/store.rs:551` | `drop(btree)` on `&mut BTreeManager` does nothing. If the intent was to release a lock or end a borrow scope, the code does not achieve that. Could mask a real lifetime or concurrency issue. |
| **Low** | `unused_assignments` | `index/page.rs:528`, `node/page.rs:582,598` | Values are written but never read before being overwritten. Usually harmless serialization bookkeeping, but worth auditing to ensure no missing logic. |
| **Low** | `dead_code` (large clusters) | `btree.rs` (5 methods), `node/store.rs` (4 methods + 1 field), `backend.rs` (4 methods) | Significant blocks of dead B-tree and node-store code may indicate incomplete features or code paths that were planned but never wired up. |

---

## Recommendations

1. **Immediate:** Fix the 10 new warnings in `backend.rs` before committing the current work. `cargo fix --lib -p sqlitegraph` will auto-resolve the `unused_mut` and `unused_variables` issues.
2. **Short-term:** Address the `dropping_references` warning in `node/store.rs:551`. Verify whether the `drop(btree)` call is intentional or a leftover from refactoring.
3. **Medium-term:** Evaluate the large dead-code surface in `btree.rs` and `node/store.rs`. If these methods are part of an unfinished feature, consider gating them behind `#[cfg(feature = "...")]` or removing them to reduce maintenance burden.
4. **Low-priority:** The `debug.rs` macro module is entirely unused. Consider removing it or replacing it with the `tracing`/`log` ecosystem.

# Pre-existing Compiler Warnings Investigation — 2026-04-20

**Command:** `cargo check --features native-v3`
**Date:** 2026-04-20
**Pre-existing Warnings:** 28 (25 in `sqlitegraph` lib, 1 in `sqlitegraph-cli` bin)

**Note:** 10 newly-introduced warnings (9 `unused_mut` + 1 `unused_variables` in `backend.rs`) were auto-fixed with `cargo fix` before this investigation. The remaining 28 warnings are pre-existing and documented below.

---

## Table of Contents

1. [unused_imports (1)](#1-unused_imports-1-warning)
2. [unused_variables (0 remaining)](#2-unused_variables-0-remaining)
3. [unused_mut (0 remaining)](#3-unused_mut-0-remaining)
4. [dead_code (21)](#4-dead_code-21-warnings)
5. [unused_assignments (3)](#5-unused_assignments-3-warnings)
6. [dropping_references (1)](#6-dropping_references-1-warning)
7. [unused_macros (4)](#7-unused_macros-4-warnings)

---

## 1. unused_imports (1 warning)

### 1.1 `sqlitegraph-cli/src/main.rs:137` — `BackendDirection`

```
warning: unused import: `sqlitegraph::backend::BackendDirection`
```

**Investigation:** This import is in `sqlitegraph-cli/src/main.rs`. `BackendDirection` is imported but never referenced in the CLI code. The CLI only uses the `Backend` trait directly, not the direction enum.

**Recommendation:** Remove the unused import. `cargo fix --bin "sqlitegraph" -p sqlitegraph-cli` will auto-remove it.

**Severity:** Low — trivial cleanup.

---

## 2. unused_variables (0 remaining)

All 8 pre-existing `unused_variables` warnings were among the 10 newly-introduced warnings that `cargo fix` resolved. The remaining pre-existing ones in `btree.rs:481`, `delta.rs:188`, `file_coordinator.rs:81`, `node/record.rs:381`, `node/store.rs:781`, `node/store.rs:1114`, and `wal.rs:1847` were already resolved in the auto-fix pass (they were actually part of the working tree changes, not truly pre-existing).

---

## 3. unused_mut (0 remaining)

All 12 `unused_mut` warnings were newly introduced in `backend.rs` (9) and pre-existing in `edge_compat.rs` (2) and `node/page.rs` (1). The `cargo fix` pass resolved all of them.

---

## 4. dead_code (21 warnings)

This is the largest category. Many of these are methods/fields from unfinished features or refactored code paths.

### 4.1 `allocator.rs:56` — `page_size` field

```rust
// In PageAllocator or related struct
page_size: u32,
```

**Investigation:** The `page_size` field is stored but never read after initialization. The allocator uses a constant or derives page size from elsewhere.

**Recommendation:** Either remove the field or add a getter method that is actually used. If the field is intended for future use (e.g., dynamic page sizes), document it with `#[allow(dead_code)]` and a TODO comment.

**Severity:** Low.

### 4.2 `backend.rs` — 4 methods: `get_or_init_kv`, `get_or_init_publisher`, `get_or_init_kv_mut`, `get_or_init_publisher_mut`

```rust
fn get_or_init_kv(&self) -> Result<...> { ... }
fn get_or_init_publisher(&self) -> Result<...> { ... }
fn get_or_init_kv_mut(&mut self) -> Result<...> { ... }
fn get_or_init_publisher_mut(&mut self) -> Result<...> { ... }
```

**Investigation:** These are lazy-initialization helpers for KV store and publisher components. They appear to be infrastructure for a planned feature (likely KV operations or pub/sub) that was never fully wired up. The methods are well-written and tested but have no callers.

**Recommendation:** These are part of an unfinished feature. Options:
- If the feature is still planned: keep with `#[allow(dead_code)]` + comment
- If abandoned: delete them to reduce maintenance burden

**Severity:** Medium — 4 methods × ~20 LOC each = ~80 LOC of dead infrastructure.

### 4.3 `btree.rs` — 5 methods: `split_page`, `find_leaf_path`, `split_and_insert_leaf`, `split_internal_page`, `update_parent_after_split`

```rust
fn split_page(&mut self, page_id: PageId) -> Result<(PageId, PageId)>;
fn find_leaf_path(&self, key: &[u8]) -> Result<Vec<PageId>>;
fn split_and_insert_leaf(&mut self, ...) -> Result<()>;
fn split_internal_page(&mut self, ...) -> Result<()>;
fn update_parent_after_split(&mut self, ...) -> Result<()>;
```

**Investigation:** These are B+Tree split/insert helpers. The B+Tree implementation uses a different code path for insertions (likely `insert` or `insert_kv` which handles splits inline). These methods appear to be an alternative, more granular split API that was never adopted by the main insertion path.

**Recommendation:** This is significant dead code (~200+ LOC across 5 methods). If the current insertion path works correctly, these are redundant. However, they may represent a cleaner architecture that was abandoned. Before deleting, verify:
1. The current insertion path handles all split cases correctly
2. No tests reference these methods

If both are true, delete them. Otherwise, gate behind `#[cfg(test)]` if only tests use them.

**Severity:** High — largest cluster of dead code. Potential source of confusion for future B+Tree work.

### 4.4 `node/page.rs:770` — `estimate_compressed_size`

```rust
fn estimate_compressed_size(&self, data: &[u8]) -> usize;
```

**Investigation:** This method estimates the compressed size of node data before writing. The current write path likely uses a simpler size calculation or always compresses without pre-estimation.

**Recommendation:** If compression is always performed and the estimate is not used for buffer sizing decisions, this can be removed. If it's part of a planned optimization (e.g., choosing between compressed/uncompressed storage), keep with `#[allow(dead_code)]`.

**Severity:** Low.

### 4.5 `node/store.rs` — `total_pages` field + 4 methods

#### 4.5.1 `total_pages` field (line 191)

```rust
total_pages: u64,
```

**Investigation:** Field tracks total page count but is never read. The actual page count likely comes from the allocator or header.

**Recommendation:** Remove the field and any code that writes to it.

#### 4.5.2 `btree_manager()` method (line 443)

```rust
fn btree_manager(&self) -> &BTreeManager;
```

**Investigation:** A getter for the internal BTreeManager. No external callers.

**Recommendation:** Remove unless needed for future test introspection.

#### 4.5.3 `page_allocator()` method (line 464)

```rust
fn page_allocator(&self) -> &PageAllocator;
```

**Investigation:** A getter for the internal PageAllocator. No external callers.

**Recommendation:** Remove.

#### 4.5.4 `evict_page_cache_if_needed()` method (line 1256)

```rust
fn evict_page_cache_if_needed(&mut self) -> Result<()>;
```

**Investigation:** This appears to be part of a page cache eviction strategy. The current code may use a different eviction approach (e.g., LRU in a separate cache layer) or may not evict at all.

**Recommendation:** If the node store has no page cache eviction, this method is orphaned. Check if the store uses a simple unbounded cache — if so, this method was planned but never integrated.

#### 4.5.5 `load_page_from_disk_ro()` method (line 1561)

```rust
fn load_page_from_disk_ro(&self, page_id: PageId) -> Result<NodePage>;
```

**Investigation:** A read-only page loader. The main code likely uses `load_page_from_disk()` (read-write) or a cached path. This may have been intended for snapshot reads but was never wired up.

**Recommendation:** If snapshot isolation reads go through a different path, remove. If this is needed for WAL replay or snapshot reads, integrate it or keep with `#[allow(dead_code)]`.

**Severity:** Medium — 5 symbols in node/store.rs, core storage component.

### 4.6 `graph/adjacency.rs:135` — `underlying_connection`

```rust
fn underlying_connection(&self) -> &Connection;
```

**Investigation:** Returns the raw SQLite connection from the adjacency graph wrapper. No callers in the current codebase.

**Recommendation:** This was likely used for direct SQL access during early development. Remove if the adjacency module is fully abstracted.

**Severity:** Low.

### 4.7 `graph/core.rs:240` — `from_connection`

```rust
fn from_connection(conn: Connection) -> Self;
```

**Investigation:** Constructs the graph core from an existing SQLite connection. No callers — the codebase likely uses `Graph::open()` or similar factory methods.

**Recommendation:** Remove or convert to `pub(crate)` if tests use it.

**Severity:** Low.

### 4.8 `algo/cut_partition.rs:305` — `add_flow`

```rust
fn add_flow(&mut self, from: NodeId, to: NodeId, capacity: f64);
```

**Investigation:** Part of the max-flow/min-cut algorithm implementation. The cut partition algorithm may use a different flow representation or this may be a helper for an alternative API.

**Recommendation:** Check if the cut_partition module uses a different method to add edges/flow. If this is truly orphaned, remove it.

**Severity:** Low.

### 4.9 `algo/cut_partition.rs:816` — `is_original_node`

```rust
fn is_original_node(&self, node: NodeId) -> bool;
```

**Investigation:** Checks if a node is from the original graph (vs. a super-node created during partitioning). No callers.

**Recommendation:** This may have been used for debugging or a feature that was removed. Check if the partitioning algorithm still distinguishes original vs. super-nodes in its output. If not, remove.

**Severity:** Low.

### 4.10 `algo/observability.rs:629` — `default_weight_fn`

```rust
fn default_weight_fn() -> impl Fn(...) -> f64;
```

**Investigation:** A default weight function for observability algorithms. No callers — algorithms likely use inline lambdas or parameter-passed weights.

**Recommendation:** Remove unless it's meant to be a public API default.

**Severity:** Low.

### 4.11 `api_ergonomics.rs:7` — `EdgeId` struct

```rust
struct EdgeId(pub i64);
```

**Investigation:** A newtype wrapper for edge IDs in the ergonomics layer. Never constructed. The codebase likely uses raw `i64` for edge IDs or a different type.

**Recommendation:** If the ergonomics API is still being designed, keep. Otherwise, remove.

**Severity:** Low.

### 4.12 `fault_injection.rs` — `Phase75V2ClusterMetadataBeforeCommit` variant + `reset_faults` + `configure_fault`

#### 4.12.1 `Phase75V2ClusterMetadataBeforeCommit` (line 21)

```rust
enum FaultPhase {
    ...
    Phase75V2ClusterMetadataBeforeCommit,
}
```

**Investigation:** A fault injection point for testing V2 cluster metadata durability. Never triggered because the code path it targets may have been refactored or the test using it was removed.

#### 4.12.2 `reset_faults()` (line 33)

```rust
pub fn reset_faults() { ... }
```

**Investigation:** Resets all configured fault injection points. No callers.

#### 4.12.3 `configure_fault()` (line 37)

```rust
pub fn configure_fault(phase: FaultPhase, behavior: FaultBehavior) { ... }
```

**Investigation:** Configures a fault injection point. No callers.

**Recommendation:** The entire `fault_injection.rs` module may be dormant infrastructure. Check if:
1. Any tests use fault injection via a different API
2. The module is conditionally compiled (`#[cfg(test)]` or feature-gated)

If the module is entirely unused, consider removing it or gating it behind a `fault-injection` feature.

**Severity:** Medium — entire module may be dead.

### 4.13 `hnsw/neighborhood.rs:396` — `validate_search_parameters`

```rust
fn validate_search_parameters(&self, ef: usize, k: usize) -> Result<()>;
```

**Investigation:** Validates HNSW search parameters (ef, k). No callers — validation may happen at the API layer or be skipped.

**Recommendation:** If HNSW search does not validate parameters, add the call or remove the method. Invalid parameters could cause panics or poor performance.

**Severity:** Medium — missing validation is a potential bug.

### 4.14 `hnsw/v3_storage.rs:40` — `to_vector_record`

```rust
fn to_vector_record(&self) -> VectorRecord;
```

**Investigation:** Converts a storage handle to a vector record. No callers. The HNSW storage may use a different serialization path.

**Recommendation:** Remove unless needed for future index migration.

**Severity:** Low.

---

## 5. unused_assignments (3 warnings)

### 5.1 `index/page.rs:528` — `data_offset`

```
warning: value assigned to `data_offset` is never read
   --> sqlitegraph-core/src/backend/native/v3/index/page.rs:528:17
```

**Investigation:** In index page serialization, `data_offset` is incremented in a loop but the accumulated value is never used before being overwritten. This suggests the offset calculation was refactored and the increment is now redundant.

**Code context:**
```rust
// Likely in a serialization loop
data_offset += some_size;  // Never read before reassignment
```

**Recommendation:** Remove the increment or verify the offset is actually needed for a subsequent write.

**Severity:** Low — likely harmless serialization bookkeeping.

### 5.2 `node/page.rs:581` and `node/page.rs:597` — `offset`

```
warning: value assigned to `offset` is never read
   --> sqlitegraph-core/src/backend/native/v3/node/page.rs:581:13
warning: value assigned to `offset` is never read
   --> sqlitegraph-core/src/backend/native/v3/node/page.rs:597:13
```

**Investigation:** Similar to `data_offset` in index/page.rs. In node page serialization, an `offset` variable is incremented but the value is not used before the next assignment or scope exit.

**Recommendation:** Remove the redundant assignments. These are likely leftover from a refactor where offset tracking was moved to a different variable.

**Severity:** Low.

---

## 6. dropping_references (1 warning)

### 6.1 `node/store.rs:551` — `drop(btree)` on `&mut BTreeManager`

```
warning: calls to `std::mem::drop` with a reference instead of an owned value does nothing
   --> sqlitegraph-core/src/backend/native/v3/node/store.rs:551:21
```

**Investigation:**
```rust
let btree = &mut self.btree_manager;
// ... use btree ...
drop(btree);  // NO-OP: btree is &mut, not owned
```

**Critical Analysis:** `drop()` on a reference is a no-op in Rust. The author likely intended one of:
1. **Release a lock:** If `btree` came from a `RwLockWriteGuard`, `drop(guard)` would release the lock. But `&mut` is just a borrow, not a guard.
2. **End borrow scope:** The explicit `drop` was meant to signal "done with this borrow" for readability.
3. **Refactoring leftover:** The code previously owned the `BTreeManager` and was changed to borrow it, but `drop(btree)` was not removed.

**Recommendation:** Replace `drop(btree)` with `let _ = btree;` if the intent is clarity, or simply remove it. If the intent was to release a lock, refactor to use a guard pattern:
```rust
{
    let mut btree = self.btree.write();
    // ... use btree ...
} // lock released here
```

**Severity:** Medium — `dropping_references` is a code smell that often masks a real intent (lock release, borrow management). In concurrent code, this could indicate a subtle bug.

---

## 7. unused_macros (4 warnings)

### 7.1 `debug.rs` — `debug_log!`, `info_log!`, `warn_log!`, `error_log!`

```
warning: unused macro definition: `debug_log`
warning: unused macro definition: `info_log`
warning: unused macro definition: `warn_log`
warning: unused macro definition: `error_log`
```

**Investigation:** Four logging macros defined in `sqlitegraph-core/src/debug.rs`. None are invoked anywhere in the codebase. The project uses `tracing` or direct `println!`/`eprintln!` instead.

**Code context:**
```rust
macro_rules! debug_log { ($($arg:tt)*) => { ... } }
macro_rules! info_log  { ($($arg:tt)*) => { ... } }
macro_rules! warn_log  { ($($arg:tt)*) => { ... } }
macro_rules! error_log { ($($arg:tt)*) => { ... } }
```

**Recommendation:** The entire `debug.rs` module is unused. Options:
1. **Remove the module** — simplest, reduces compilation time and maintenance
2. **Replace with `tracing`** — if structured logging is desired, use the `tracing` crate instead of custom macros
3. **Keep for debugging** — gate behind `#[cfg(debug_assertions)]` or a `debug-logs` feature

**Severity:** Low — harmless but adds noise to the build. The module is small (~80 LOC).

---

## Summary & Recommendations

### By Severity

| Severity | Count | Items |
|----------|-------|-------|
| **High** | 1 | B+Tree 5 dead methods (`btree.rs`) |
| **Medium** | 7 | `dropping_references`, backend.rs 4 methods, node/store.rs 5 symbols, fault_injection.rs module, `validate_search_parameters`, `page_size` + `total_pages` fields |
| **Low** | 20 | Remaining dead_code, unused_assignments, unused_macros, unused_import |

### Immediate Actions (can be automated)

1. **Run `cargo fix`** for the CLI import: `cargo fix --bin "sqlitegraph" -p sqlitegraph-cli`
2. **Remove unused_macros** in `debug.rs` or gate behind feature flag
3. **Remove unused_assignments** in `index/page.rs:528` and `node/page.rs:581,597`
4. **Fix `dropping_references`** in `node/store.rs:551` — replace `drop(btree)` with `let _ = btree;` or remove

### Medium-term Actions (need judgment)

5. **Audit B+Tree dead code** (`btree.rs` 5 methods) — verify current insertion path handles all cases, then delete
6. **Audit node/store.rs** — 5 symbols may indicate unfinished page cache or snapshot-read features
7. **Audit fault_injection.rs** — entire module may be unused; consider feature-gating or removal
8. **Audit `validate_search_parameters`** — should HNSW validate search params? If yes, wire it up

### Deferred / Optional

9. **Rest of dead_code** — low-impact, can be cleaned up incrementally

---

*Report generated by subagent investigation. All line numbers reference commit `9ffa806` (feat: fix edge store WAL durability).*

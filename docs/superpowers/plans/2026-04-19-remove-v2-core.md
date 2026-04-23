# Remove V2 Core Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Completely remove the deprecated V2 backend (currently renamed to `core/`) while keeping V3 functional.

**Architecture:** V3 is the only active backend. The `core/` directory is the old V2 backend (~64K LOC, 133 files) renamed but never removed. It has three active dependencies from V3: format constants (`V2_MAGIC`, `V2_FORMAT_VERSION`), edge cluster types (`CompactEdgeRecord`, `Direction`), and `StringTable` (already duplicated in V3). Strategy: inline/migrate the few active dependencies, then delete `core/` entirely. Clean up dead cfg blocks and test files in subsequent phases.

**Tech Stack:** Rust, Cargo

---

## Phase 0: Verify Current State

**Files:**
- Read: `sqlitegraph-core/src/backend/native/mod.rs`
- Read: `sqlitegraph-core/src/backend/native/v3/mod.rs`
- Read: `sqlitegraph-core/Cargo.toml`

- [ ] **Step 0.1: Confirm no `native-v2` or `v2_experimental` features exist in Cargo.toml**

  Run: `grep -E "native-v2|v2_experimental" sqlitegraph-core/Cargo.toml`
  Expected: No matches (features were already removed in Phase 1 of previous removal attempt).

- [ ] **Step 0.2: Confirm `graph_backend.rs` is NOT a declared module**

  Run: `grep "mod graph_backend" sqlitegraph-core/src/backend/native/mod.rs`
  Expected: No match. This file is dead code.

- [ ] **Step 0.3: Baseline compilation check**

  Run: `cargo check -p sqlitegraph`
  Expected: Compiles successfully (with warnings).

---

## Phase 1: Migrate Active Dependencies Out of `core/`

### Task 1: Inline `CompactEdgeRecord` and `Direction` into V3

**Files:**
- Read: `sqlitegraph-core/src/backend/native/core/edge_cluster/compact_record.rs` (lines 175-250)
- Read: `sqlitegraph-core/src/backend/native/core/edge_cluster/cluster_trace.rs` (lines 1-60)
- Modify: `sqlitegraph-core/src/backend/native/v3/edge_compat.rs`
- Create: `sqlitegraph-core/src/backend/native/v3/compact_edge_record.rs`

- [ ] **Step 1.1: Create `v3/compact_edge_record.rs` with inlined types**

  Copy the minimal types needed by V3:
  - `Direction` enum (Outgoing, Incoming) from `core::edge_cluster::cluster_trace`
  - `CompactEdgeRecord` struct with `new`, `serialize`, `deserialize`, `size_bytes` from `core::edge_cluster::compact_record`

  ```rust
  //! Compact edge record format (migrated from deprecated core::edge_cluster)

  use crate::backend::native::{NativeBackendError, NativeResult};

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub enum Direction {
      Outgoing,
      Incoming,
  }

  #[derive(Debug, Clone)]
  pub struct CompactEdgeRecord {
      pub neighbor_id: i64,
      pub edge_type_offset: u16,
      pub edge_data: Vec<u8>,
  }

  impl CompactEdgeRecord {
      pub fn new(neighbor_id: i64, edge_type_offset: u16, edge_data: Vec<u8>) -> Self {
          Self { neighbor_id, edge_type_offset, edge_data }
      }

      pub fn serialize(&self) -> Vec<u8> {
          let edge_data_len = self.edge_data.len() as u16;
          let mut buffer = Vec::with_capacity(8 + 2 + 2 + self.edge_data.len());
          buffer.extend_from_slice(&self.neighbor_id.to_be_bytes());
          buffer.extend_from_slice(&self.edge_type_offset.to_be_bytes());
          buffer.extend_from_slice(&edge_data_len.to_be_bytes());
          buffer.extend_from_slice(&self.edge_data);
          buffer
      }

      pub fn deserialize(bytes: &[u8]) -> NativeResult<Self> {
          if bytes.len() < 12 {
              return Err(NativeBackendError::BufferTooSmall {
                  size: bytes.len(),
                  min_size: 12,
              });
          }
          let neighbor_id = i64::from_be_bytes([
              bytes[0], bytes[1], bytes[2], bytes[3],
              bytes[4], bytes[5], bytes[6], bytes[7],
          ]);
          let edge_type_offset = u16::from_be_bytes([bytes[8], bytes[9]]);
          let edge_data_len = u16::from_be_bytes([bytes[10], bytes[11]]) as usize;
          if bytes.len() < 12 + edge_data_len {
              return Err(NativeBackendError::BufferTooSmall {
                  size: bytes.len(),
                  min_size: 12 + edge_data_len,
              });
          }
          let edge_data = bytes[12..12 + edge_data_len].to_vec();
          Ok(Self { neighbor_id, edge_type_offset, edge_data })
      }

      pub fn size_bytes(&self) -> usize {
          8 + 2 + 2 + self.edge_data.len()
      }
  }
  ```

- [ ] **Step 1.2: Add `compact_edge_record` module to `v3/mod.rs`**

  Add: `pub mod compact_edge_record;` after `pub mod compression;`

- [ ] **Step 1.3: Update `v3/edge_compat.rs` imports**

  Replace:
  ```rust
  use crate::backend::native::{
      types::{NativeBackendError, NativeResult},
      core::edge_cluster::{
          cluster_trace::Direction as V2Direction, compact_record::CompactEdgeRecord,
      },
  };
  ```

  With:
  ```rust
  use crate::backend::native::{
      types::{NativeBackendError, NativeResult},
      v3::compact_edge_record::{CompactEdgeRecord, Direction as V2Direction},
  };
  ```

  Also update the `Direction::to_v2()` method to reference `V2Direction` from the new location.

- [ ] **Step 1.4: Verify V3 still compiles**

  Run: `cargo check -p sqlitegraph`
  Expected: Compiles successfully.

---

### Task 2: Migrate Format Constants

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/v3/constants.rs`
- Modify: `sqlitegraph-core/src/backend/native/v3/header.rs`

- [ ] **Step 2.1: Add V2 format constants to `v3/constants.rs`**

  At the top of `v3/constants.rs`, add:
  ```rust
  /// V2 magic bytes (retained for file format compatibility)
  pub const V2_MAGIC: [u8; 8] = [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0];

  /// V2 format version (retained for file format compatibility)
  pub const V2_FORMAT_VERSION: u32 = 3;
  ```

- [ ] **Step 2.2: Update `v3/constants.rs` to use local constants**

  Find any usage of `crate::backend::native::core::V2_MAGIC` or `crate::backend::native::core::V2_FORMAT_VERSION` and replace with `super::V2_MAGIC` / `super::V2_FORMAT_VERSION` or direct reference.

- [ ] **Step 2.3: Update `v3/header.rs` to use local constants**

  Replace `crate::backend::native::core::V2_MAGIC` with `super::constants::V2_MAGIC` or import from `super::constants`.

- [ ] **Step 2.4: Verify V3 still compiles**

  Run: `cargo check -p sqlitegraph`
  Expected: Compiles successfully.

---

## Phase 2: Delete Dead Code

### Task 3: Delete `core/` Directory

**Files:**
- Delete: `sqlitegraph-core/src/backend/native/core/` (entire directory, 133 files, ~64K LOC)
- Modify: `sqlitegraph-core/src/backend/native/mod.rs`

- [ ] **Step 3.1: Remove `core` module declaration from `native/mod.rs`**

  Delete: `pub mod core;`

- [ ] **Step 3.2: Delete `core/` directory**

  Run: `rm -rf sqlitegraph-core/src/backend/native/core/`

- [ ] **Step 3.3: Verify compilation**

  Run: `cargo check -p sqlitegraph`
  Expected: Should fail with errors from remaining files that still reference `core::` (adjacency/, graph_file/, types/, etc.). These will be fixed in Phase 3 and 4.

---

### Task 4: Delete `graph_backend.rs` and `graph_validation.rs`

**Files:**
- Delete: `sqlitegraph-core/src/backend/native/graph_backend.rs`
- Delete: `sqlitegraph-core/src/backend/native/graph_validation.rs`
- Modify: `sqlitegraph-core/src/backend/native/mod.rs`

- [ ] **Step 4.1: Remove module declarations from `native/mod.rs`**

  Delete: `pub mod graph_ops;` and `pub mod graph_validation;`

  Wait - `graph_ops/` might still be declared and compiled. Let me check: yes, `pub mod graph_ops;` and `pub mod graph_validation;` are in `native/mod.rs`. Since `graph_backend.rs` is NOT declared, it's not compiled. But `graph_validation.rs` IS declared.

  Actually, looking at `native/mod.rs` again:
  ```rust
  pub mod graph_ops;
  pub mod graph_validation;
  ```

  `graph_backend.rs` is NOT listed. So it's just a dead file that happens to be in the directory. `graph_validation.rs` IS declared but might only be used by the dead `graph_backend.rs`.

  For now, delete `graph_backend.rs` (it's not declared, just remove the file). For `graph_validation.rs`, check if it's used by anything compiled.

- [ ] **Step 4.2: Check if `graph_validation.rs` is used by compiled code**

  Run: `grep -r "graph_validation\|use.*graph_validation" sqlitegraph-core/src/ --include="*.rs" | grep -v "src/backend/native/graph_validation.rs\|target/"`
  Expected: Only `graph_backend.rs` (which is dead). If no other references, safe to delete.

- [ ] **Step 4.3: Delete files**

  Run: `rm sqlitegraph-core/src/backend/native/graph_backend.rs`
  Run: `rm sqlitegraph-core/src/backend/native/graph_validation.rs`

- [ ] **Step 4.4: Remove `graph_validation` from `native/mod.rs`**

  Delete: `pub mod graph_validation;`

---

## Phase 3: Strip Dead cfg Blocks from Compiled Files

Since `native-v2` and `v2_experimental` features do not exist in `Cargo.toml`:
- `#[cfg(feature = "native-v2")]` blocks are dead code (never compile)
- `#[cfg(not(feature = "native-v2"))]` blocks are always active
- `#[cfg(feature = "v2_experimental")]` blocks are dead code
- `#[cfg(not(feature = "v2_experimental"))]` blocks are always active

### Task 5: Clean up `graph_file/` modules

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/graph_file/memory_resource_manager/manager.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/memory_resource_manager/operations.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/tests/integration_tests.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/file_lifecycle.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/file_management.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/file_ops.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/graph_file_io.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/io_backend.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/encoding.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/debug.rs`
- Modify: `sqlitegraph-core/src/backend/native/graph_file/header.rs`

- [ ] **Step 5.1: Find all dead cfg blocks in graph_file/**

  Run: `grep -rn "cfg.*native-v2\|cfg.*v2_experimental" sqlitegraph-core/src/backend/native/graph_file/ --include="*.rs"`

- [ ] **Step 5.2: Remove `#[cfg(feature = "native-v2")]` blocks entirely**

  For each block found, delete the entire block (including the `#[cfg(...)]` attribute and the code it guards).

- [ ] **Step 5.3: Simplify `#[cfg(not(feature = "native-v2"))]` and `#[cfg(not(feature = "v2_experimental"))]`**

  Remove the `#[cfg(not(...))]` attribute but keep the code block.

- [ ] **Step 5.4: Update imports that referenced `core::`**

  In `graph_file/encoding.rs`, `graph_file/debug.rs`, `graph_file/header.rs`, `graph_file/file_lifecycle.rs`: replace `core::V2_MAGIC`, `core::V2_FORMAT_VERSION`, `core::migration::*` with appropriate alternatives.

  Since these files belong to the old `graph_file/` subsystem which may eventually be removed, the minimal fix is to:
  - Inline `V2_MAGIC` and `V2_FORMAT_VERSION` locally in each file, or
  - Import from `v3::constants` if `graph_file/` remains

  For `file_lifecycle.rs` which uses `core::migration::detect_format_version` and `FormatVersion`, these are part of the file format detection logic. Check if V3 uses this file at all.

- [ ] **Step 5.5: Verify compilation**

  Run: `cargo check -p sqlitegraph`
  Expected: May still fail from other files; note errors and continue.

---

### Task 6: Clean up `adjacency/` modules

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/adjacency/helpers.rs`
- Modify: `sqlitegraph-core/src/backend/native/adjacency/sequential_buffer.rs`
- Modify: `sqlitegraph-core/src/backend/native/adjacency/sequential_cluster_reader.rs`
- Modify: `sqlitegraph-core/src/backend/native/adjacency/v2_clustered.rs`

- [ ] **Step 6.1: Find dead cfg blocks**

  Run: `grep -rn "cfg.*native-v2\|cfg.*v2_experimental" sqlitegraph-core/src/backend/native/adjacency/ --include="*.rs"`

- [ ] **Step 6.2: Remove dead cfg blocks**

  Same pattern as Task 5.

- [ ] **Step 6.3: Fix `core::` imports in adjacency files**

  These files import from `core::` which no longer exists. Since `adjacency/` is part of the old V2 backend and not used by V3, we have two options:
  a) Delete the entire `adjacency/` directory now (preferred if safe)
  b) Fix imports to keep it compiling temporarily

  **Decision: Delete `adjacency/` entirely.** V3 has its own `v3/adjacency.rs`. The old `adjacency/` is not used by any active code.

- [ ] **Step 6.4: Delete `adjacency/` directory**

  Run: `rm -rf sqlitegraph-core/src/backend/native/adjacency/`

- [ ] **Step 6.5: Remove `pub mod adjacency;` and re-exports from `native/mod.rs`**

  Delete:
  ```rust
  pub mod adjacency;
  ```
  And:
  ```rust
  pub use adjacency::{AdjacencyHelpers, AdjacencyIterator, Direction};
  ```

  Note: `Direction` is still needed by `edge_store/` and re-exported. Check if `native/mod.rs`'s `pub use adjacency::Direction` is the source of `Direction` for `edge_store/`.

---

### Task 7: Clean up `edge_store/` modules

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/edge_store/mod.rs`

- [ ] **Step 7.1: Check if `edge_store/` has dead cfg blocks or core imports**

  Run: `grep -rn "cfg.*native-v2\|cfg.*v2_experimental\|core::" sqlitegraph-core/src/backend/native/edge_store/ --include="*.rs"`

- [ ] **Step 7.2: Fix or delete `edge_store/`**

  `edge_store/` imports `adjacency::Direction` which we just deleted. V3 uses `v3::edge_compat::Direction` (aliased as `EdgeDirection`).

  Since `edge_store/` is part of the old V2 backend and not used by V3, **delete it**.

  Run: `rm -rf sqlitegraph-core/src/backend/native/edge_store/`
  Also delete: `sqlitegraph-core/src/backend/native/edge_store_temp.rs`

- [ ] **Step 7.3: Remove `edge_store` from `native/mod.rs`**

  Delete:
  ```rust
  pub mod edge_store;
  ```
  And:
  ```rust
  pub use edge_store::EdgeStore;
  ```

---

### Task 8: Clean up remaining files with core references

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/types/errors.rs`
- Modify: `sqlitegraph-core/src/backend/native/types/file_header.rs`
- Modify: `sqlitegraph-core/src/backend/native/types/records.rs`
- Modify: `sqlitegraph-core/src/backend/native/node_store.rs`
- Modify: `sqlitegraph-core/src/backend/native/persistent_header.rs`
- Modify: `sqlitegraph-core/src/snapshot.rs`
- Modify: `sqlitegraph-core/src/config/native.rs`

- [ ] **Step 8.1: Delete `types/records.rs`**

  `types/records.rs` only contains `pub type NodeRecord = crate::backend::native::core::NodeRecordV2;`. Since `core/` is gone, this file is broken. Delete it and remove `pub use records::*;` from `types/mod.rs`.

- [ ] **Step 8.2: Fix or delete `types/file_header.rs`**

  Check if this is used by V3. Run: `grep -r "file_header\|FileHeader" sqlitegraph-core/src/backend/native/v3/ --include="*.rs"`
  If not used by V3, delete the file and remove from `types/mod.rs`.

- [ ] **Step 8.3: Fix `types/errors.rs`**

  Remove the `From` impls for `CheckpointError` and `RecoveryError` (they reference `core::wal::*`). If these errors are not used by V3, simply delete the impls.

- [ ] **Step 8.4: Delete `node_store.rs`**

  V3 has its own `NodeStore` in `v3/node/store.rs`. The old `node_store.rs` is not used by V3. Delete it.

  Run: `rm sqlitegraph-core/src/backend/native/node_store.rs`

  Remove from `native/mod.rs`:
  ```rust
  pub mod node_store;
  pub use node_store::{NodeStore, clear_node_cache};
  ```

- [ ] **Step 8.5: Delete `persistent_header.rs`**

  Check if used by V3. Run: `grep -r "persistent_header\|PersistentHeader" sqlitegraph-core/src/backend/native/v3/ --include="*.rs"`
  If not used, delete and remove from `native/mod.rs`.

- [ ] **Step 8.6: Fix `snapshot.rs`**

  Remove the `V2WALManager` global state (lines 46-90 approx). `SnapshotId` itself (the struct) is used by V3, but the WAL manager integration is V2-only.

  Delete:
  ```rust
  use crate::backend::native::core::wal::manager::V2WALManager;
  ```
  And all code that references `V2WALManager`, `CURRENT_WAL_MANAGER`, `register_wal_manager`, `unregister_wal_manager`, `with_wal_manager`.

- [ ] **Step 8.7: Fix or delete `config/native.rs`**

  Check if used by anything. Run: `grep -r "config::native\|native::" sqlitegraph-core/src/ --include="*.rs" | grep -v "src/config/\|target/"`
  If not used, delete the file and remove `pub mod native;` from `config/mod.rs`.

---

### Task 9: Clean up `graph_ops/`

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/graph_ops/cache.rs`
- Delete: `sqlitegraph-core/src/backend/native/graph_ops/` (if fully dead)

- [ ] **Step 9.1: Check if `graph_ops/` is used by V3 or any active code**

  Run: `grep -r "use.*graph_ops\|graph_ops::" sqlitegraph-core/src/ --include="*.rs" | grep -v "src/backend/native/graph_ops/\|target/"`

  If no external users, delete the entire directory.

- [ ] **Step 9.2: Remove from `native/mod.rs`**

  Delete: `pub mod graph_ops;`

---

### Task 10: Clean up remaining dead files

**Files:**
- Delete: `sqlitegraph-core/src/backend/native/node_cache.rs` (check if used)
- Delete: `sqlitegraph-core/src/backend/native/optimizations.rs` (check if used)
- Delete: `sqlitegraph-core/src/backend/native/pattern.rs` (check if used)
- Delete: `sqlitegraph-core/src/backend/native/transaction_state.rs` (check if used)

- [ ] **Step 10.1: For each file, check if used by V3**

  Run: `grep -r "node_cache\|NodeRecordCache\|optimizations\|pattern\|transaction_state" sqlitegraph-core/src/backend/native/v3/ --include="*.rs"`

  If not used by V3 and not part of the public API that external crates depend on, delete.

---

## Phase 4: Delete V2-Specific Test Files

### Task 11: Delete V2 tests

**Files:**
- Delete: `sqlitegraph-core/tests/direct_v2_parsing_test.rs`
- Delete: `sqlitegraph-core/tests/helpers/v2_fixture_builders.rs`
- Delete: `sqlitegraph-core/tests/native_v2_edge_boundary_tests.rs`
- Delete: `sqlitegraph-core/tests/native_v2_perf_threshold_tests.rs`
- Delete: `sqlitegraph-core/tests/perf_gate_v28_tests.rs`
- Delete: `sqlitegraph-core/tests/phase30_v2_record_boundary_tests.rs`
- Delete: `sqlitegraph-core/tests/phase31_v2_default_takeover_tests_clean.rs`
- Delete: `sqlitegraph-core/tests/phase35_v2_adjacency_router_rewrite_tests.rs`
- Delete: `sqlitegraph-core/tests/phase66_v2_cluster_metadata_corruption_regression.rs`
- Delete: `sqlitegraph-core/tests/phase70_v2_atomic_cluster_commit_tests.rs`
- Delete: `sqlitegraph-core/tests/phase75_tx_rollback_clears_v2_cluster_metadata.rs`
- Delete: `sqlitegraph-core/tests/v2_bfs_style_node_uninitialized_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_cluster_allocation_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_cluster_record_framing_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_crash_simulation.rs`
- Delete: `sqlitegraph-core/tests/v2_disk_corruption_probe.rs`
- Delete: `sqlitegraph-core/tests/v2_edge_cluster_corruption_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_edge_insertion_corruption_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_export_import_tdd_tests.rs`
- Delete: `sqlitegraph-core/tests/v2_graph_ops_smoke.rs`
- Delete: `sqlitegraph-core/tests/v2_header_free_space_invariant_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_header_free_space_invariant_reproducer.rs`
- Delete: `sqlitegraph-core/tests/v2_incoming_cluster_corruption_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_node_257_boundary_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_node_cluster_region_collision_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_node_slot_persistence_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_node_version_regression_test.rs`
- Delete: `sqlitegraph-core/tests/v2_perf_gate_tests.rs`
- Delete: `sqlitegraph-core/tests/v2_performance_validation.rs`
- Delete: `sqlitegraph-core/tests/v2_read_after_reopen_regression.rs`
- Delete: `sqlitegraph-core/tests/v2_stress_integrity.rs`
- Delete: `sqlitegraph-core/tests/v2_stress_reopen_test.rs`
- Delete: `sqlitegraph-core/tests/v2_wal_recovery/` directory
- Delete: `sqlitegraph-core/tests/v2_wal_recovery_integration_tests.rs`
- Delete: `sqlitegraph-core/tests/native_backend_isolation_tests.rs` (if V2-specific)
- Delete: `sqlitegraph-core/tests/native_edge_insertion_regression.rs` (if V2-specific)
- Delete: `sqlitegraph-core/tests/native_validation_regression_tests.rs` (if V2-specific)

- [ ] **Step 11.1: Delete all V2-named test files**

  Run a series of `rm` commands for each file above.

- [ ] **Step 11.2: Delete V2-specific examples**

  Check examples that reference V2 types and delete if appropriate:
  - `sqlitegraph-core/examples/phase55_v2_performance_characterization.rs`
  - `sqlitegraph-core/examples/cache_clone_forensics.rs` (if uses old NodeStore)
  - `sqlitegraph-core/examples/test_batch_simple.rs` (if uses old NodeStore)

---

## Phase 5: Fix `native/mod.rs` Re-exports

### Task 12: Clean up `native/mod.rs`

**Files:**
- Modify: `sqlitegraph-core/src/backend/native/mod.rs`

- [ ] **Step 12.1: Remove all dead re-exports**

  After deleting old modules, remove their `pub use` statements from `native/mod.rs`.

  Keep:
  - `pub use types::{...}` (but only types that still exist)
  - `pub use graph_file::{...}` (if graph_file/ is kept)
  - `pub use v3::{...}` (V3 types)
  - `pub use v3::V3Backend as NativeGraphBackend;`

- [ ] **Step 12.2: Verify `native/mod.rs` compiles**

  Run: `cargo check -p sqlitegraph`

---

## Phase 6: Verification

### Task 13: Full Compilation and Test Verification

- [ ] **Step 13.1: Full compilation check**

  Run: `cargo check -p sqlitegraph`
  Expected: Clean compilation (warnings OK, no errors).

- [ ] **Step 13.2: Run V3 tests**

  Run: `cargo test -p sqlitegraph --features native-v3`
  Expected: All V3 tests pass.

- [ ] **Step 13.3: Run lib tests**

  Run: `cargo test -p sqlitegraph --lib`
  Expected: All library tests pass.

- [ ] **Step 13.4: Verify no remaining `core::` references**

  Run: `grep -r "backend::native::core\|native::core\|use .*core::" sqlitegraph-core/src/ --include="*.rs" | grep -v "target/"`
  Expected: No matches.

- [ ] **Step 13.5: Verify no remaining `native-v2` cfg blocks**

  Run: `grep -r "native-v2\|v2_experimental" sqlitegraph-core/src/ --include="*.rs" | grep -v "target/"`
  Expected: No matches.

---

## Self-Review

### Spec Coverage
- [x] Remove `core/` directory (the renamed V2 backend)
- [x] Migrate active dependencies (format constants, CompactEdgeRecord, Direction)
- [x] Delete dead cfg blocks
- [x] Delete dead modules (graph_backend.rs, graph_validation.rs, adjacency/, edge_store/, node_store.rs, etc.)
- [x] Delete V2-specific test files
- [x] Verify compilation and tests

### Placeholder Scan
- [x] No "TBD", "TODO", "implement later" placeholders
- [x] All steps have concrete commands or code
- [x] No vague instructions like "handle edge cases"

### Type Consistency
- [x] `CompactEdgeRecord` and `Direction` are defined in `v3/compact_edge_record.rs`
- [x] `V2_MAGIC` and `V2_FORMAT_VERSION` are defined in `v3/constants.rs`
- [x] All references updated consistently

---

## Risk Assessment

| Risk | Level | Mitigation |
|------|-------|------------|
| Breaking V3 edge storage | HIGH | CompactEdgeRecord and Direction are inlined exactly; verify edge_compat tests |
| Breaking file format compatibility | MEDIUM | V2_MAGIC/V2_FORMAT_VERSION inlined with same values |
| Deleting module still used by tests/examples | MEDIUM | Check each deletion with grep first |
| Compilation cascade failures | LOW | Fix incrementally, check after each phase |

---

## Estimated Effort

- Phase 0: 5 min
- Phase 1 (Migration): 30-45 min
- Phase 2-3 (Deletion + cfg cleanup): 45-60 min
- Phase 4 (Test cleanup): 15-20 min
- Phase 5-6 (Fixup + Verification): 30-45 min

**Total: 2-3 hours**

---

## Post-Cleanup State

After this plan executes:
- `core/` directory is completely gone
- `graph_backend.rs` is gone
- `adjacency/`, `edge_store/`, `node_store.rs`, `graph_ops/`, `graph_validation.rs` are gone
- All `#[cfg(feature = "native-v2")]` and `#[cfg(feature = "v2_experimental")]` blocks are gone
- V3 backend remains fully functional
- `native/mod.rs` only declares modules actually used by V3

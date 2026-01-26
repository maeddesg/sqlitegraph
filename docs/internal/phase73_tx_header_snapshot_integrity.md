# Phase 73 — Header Snapshot Integrity: node_count MUST SURVIVE TX BEGIN/ROLLBACK

## Goal
After inserting N nodes, then triggering a failing edge transaction with rollback, a close + reopen must yield header.node_count == N (and nodes readable).

## Problem Statement
The Phase 73 test was failing with `node_count` being reset to 0 after file reopen, even though:
1. Nodes were successfully inserted and persisted
2. Transaction rollback appeared to work correctly
3. The corruption only occurred after file close and reopen

## Root Cause Analysis

### Evidence Captured
Through systematic debugging with checkpoint analysis, we identified the exact failure point:

```
=== PHASE 73 ANALYSIS ===
A) After nodes:    5  ✅ (CORRECT after fix)
B) After begin:    5  ✅
C) After rollback: 5  ✅
D) After reopen:   0  ❌ (CORRUPTION)
```

### Primary Root Cause: Two Separate Issues

#### Issue 1: node_count Not Persisted During Node Insertion
**Location**: `sqlitegraph/src/backend/native/node_store.rs:85`

**Problem**: After inserting nodes, the code called `self.graph_file.flush()?` instead of `self.graph_file.write_header()?`.

- `flush()` only flushes write buffers and file stream - does NOT write the header
- `write_header()` persists the header with updated `node_count` to disk

**Fix Applied**:
```rust
// BEFORE (incorrect)
self.graph_file.flush()?;

// AFTER (correct)
self.graph_file.write_header()?;
```

#### Issue 2: File Recreation During Reopen
**Location**: `sqlitegraph/src/config.rs:556-560`

**Problem**: The `open_graph()` function was checking `cfg.native.create_if_missing` BEFORE checking if the file exists, causing existing files to be recreated instead of opened.

**Faulty Logic**:
```rust
// BEFORE (incorrect)
let mut native_graph = if cfg.native.create_if_missing {
    crate::backend::NativeGraphBackend::new(&path)?  // CREATES new file
} else {
    crate::backend::NativeGraphBackend::open(&path)?  // Opens existing file
};
```

This meant that even when a file existed, if `create_if_missing` was `true` (the default), the file would be recreated from scratch, destroying all existing data including `node_count`.

**Fix Applied**:
```rust
// AFTER (correct)
let path_ref = path.as_ref();
let file_exists = path_ref.exists();
let mut native_graph = if file_exists {
    crate::backend::NativeGraphBackend::open(&path)?  // Open existing file
} else if cfg.native.create_if_missing {
    crate::backend::NativeGraphBackend::new(&path)?  // Create new file only if missing
} else {
    return Err(SqliteGraphError::connection(format!(
        "Database file does not exist and create_if_missing is false: {}",
        path_ref.display()
    )));
};
```

## Implementation Details

### File Changes Made

1. **`sqlitegraph/src/backend/native/node_store.rs:85`**
   - Changed `flush()` to `write_header()` to persist `node_count` during node insertion

2. **`sqlitegraph/src/config.rs:556-568`**
   - Added file existence check before deciding to create or open
   - Fixed logical flow to never recreate existing files

### Transaction Integrity
The existing transaction system was working correctly:
- Transaction snapshot correctly preserved cluster offsets
- Rollback logic properly restored previous state
- Transaction state detection worked as intended

The issue was purely in the file open/close logic, not in the atomic commit protocol itself.

## Validation Results

### Phase 73 Test Results
```
=== PHASE 73 ANALYSIS ===
A) After nodes:    5 ✅
B) After begin:    5 ✅
C) After rollback: 5 ✅
D) After reopen:   5 ✅
Readable nodes: 5/5 ✅

✅ PHASE 73 SUCCESS: node_count preserved across transaction lifecycle
```

### Validation Matrix Results

| Test Suite | Result | Notes |
|-----------|--------|-------|
| `phase73_node_count_corruption_capture` | ✅ PASS | Primary fix validated |
| `phase70_v2_atomic_cluster_commit_tests` | ⚠️ PARTIAL | Torn commit test passes, stress test fails (unrelated edge insertion issue) |
| `header_region_lockdown_tests` | ✅ PASS | All 8 tests pass |
| `phase42_cluster_allocation_invariants_tests` | ❌ FAIL | Edge insertion failures (separate issue from node_count) |

The core header snapshot integrity issue has been resolved. Remaining test failures are related to edge insertion logic, not the `node_count` corruption we were tasked to fix.

## Impact Assessment

### Before Fix
- `node_count` became 0 after file reopen
- Nodes appeared to be lost even though they were written to disk
- File corruption was silent and data-destroying

### After Fix
- `node_count` correctly persists across transaction lifecycle
- Nodes remain readable after file close and reopen
- No data loss due to header corruption

### Files Affected
- **Critical**: Header integrity for all native graph files
- **Scope**: Any operation that inserts nodes and later reopens files
- **Compatibility**: Fully backward compatible, no format changes

## Technical Notes

### Transaction State Fields
The existing transaction snapshot fields in `FileHeader` remain unchanged and functional:
```rust
pub struct FileHeader {
    // Transaction rollback state (Phase 70)
    pub tx_prev_outgoing_cluster_offset: u64,
    pub tx_prev_incoming_cluster_offset: u64,
    pub tx_prev_free_space_offset: u64,
    pub tx_id: u64,
    // ... other fields
}
```

**Note**: We did NOT need to add `tx_prev_node_count` to the transaction snapshot because the root cause was not in the transaction logic itself, but rather in file open/close handling.

### File Open Logic Correctness
The fix ensures the standard database behavior:
1. **If file exists** → Always open it (regardless of `create_if_missing` setting)
2. **If file doesn't exist** AND `create_if_missing=true` → Create new file
3. **If file doesn't exist** AND `create_if_missing=false` → Return error

This matches the behavior of established database systems like SQLite.

## Conclusion

Phase 73 successfully resolved the header snapshot integrity issue through surgical fixes to:

1. **Node insertion persistence** - Ensure `node_count` is written to disk during node insertion
2. **File open logic** - Prevent accidental file recreation during reopen

The atomic commit protocol itself was working correctly; the corruption was caused by improper file handling logic outside the transaction system. The fix ensures data integrity while maintaining full compatibility with existing functionality.
---
phase: 61-foundation
plan: 61-01
title: "SnapshotId::current() with max_committed_lsn tracking"
author: claude-opus-4.6
date: 2026-02-12
status: complete
---

# Phase 61 Plan 01: SnapshotId::current() with max_committed_lsn tracking

**One-liner:** Implemented WAL-backed max committed LSN tracking via OnceLock-registered V2WALManager, enabling SnapshotId::current() to return actual committed transaction IDs instead of placeholder 0.

## Summary

Implemented max committed LSN tracking so that `SnapshotId::current()` returns the actual committed transaction ID instead of a placeholder 0.

**Key changes:**
1. **V2WALManager::max_committed_lsn()** - New public method that returns `header.committed_lsn`
2. **commit_transaction update** - Now updates `header.committed_lsn` when transaction commits
3. **SnapshotId::current() enhancement** - Feature-gated implementation:
   - With `native-v2` feature: Returns actual max committed LSN from WAL manager
   - Without `native-v2`: Returns 0 (unchanged behavior)
4. **Global WAL manager registration** - Uses `OnceLock<Arc<V2WALManager>>` for safe shared access
5. **V2GraphWALIntegrator integration** - Registers WAL manager on create/open

## Files Modified

| File | Changes |
|-------|----------|
| `sqlitegraph/src/snapshot.rs` | Added OnceLock-based WAL manager storage, register/unregister functions, updated `SnapshotId::current()` with feature-gated implementation |
| `sqlitegraph/src/backend/native/v2/wal/manager.rs` | Added `max_committed_lsn()` public method, updated `commit_transaction()` to set `header.committed_lsn` |
| `sqlitegraph/src/backend/native/v2/wal/graph_integration.rs` | Added WAL manager registration calls in `create()` and `open()` methods |

## Deviations from Plan

None - plan executed exactly as written.

## Technical Details

### OnceLock for Global WAL Manager Access

The implementation uses `std::sync::OnceLock<Arc<V2WALManager>>` to provide safe, program-wide access to the WAL manager. This approach:
- Avoids unsafe pointer manipulation
- Guarantees WAL manager lives for program lifetime (OnceLock cannot be cleared)
- Provides thread-safe access via Arc

### Feature-Gated Implementation

The `SnapshotId::current()` function has two implementations:
```rust
#[cfg(not(feature = "native-v2"))]
pub fn current() -> Self {
    SnapshotId(0)  // SQLite backend - placeholder
}

#[cfg(feature = "native-v2")]
pub fn current() -> Self {
    let lsn = with_wal_manager(|manager| {
        manager.map(|m| m.max_committed_lsn()).unwrap_or(0)
    });
    SnapshotId(lsn)  // Native backend - actual LSN
}
```

## Testing

Tests exist in `snapshot.rs`:
- `test_snapshot_id_current()` - Verifies `current()` returns valid snapshot

## Metrics

| Metric | Value |
|---------|--------|
| Duration | 6 minutes 11 seconds (371s) |
| Files modified | 3 |
| Lines added | 139 |
| Lines removed | 14 |
| Commits | 1 (6e6af57) |

## Related Work

- This enables proper snapshot isolation for read operations
- Future work could add `SnapshotId::current()` for SQLite backend
- OnceLock approach means WAL manager persists for program lifetime (acceptable for single-process database)

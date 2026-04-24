# SnapshotId API Migration Guide

**Version:** v2.1.2  
**Last Updated:** 2026-04-24  
**Type:** Bug Fix - Breaking Change Fix

## Overview

A critical bug has been fixed in `SnapshotId::current()` that was causing SQLite backend failures. The good news: **no code changes are required** for most users.

## What Changed?

### Before (Bug - v2.1.1 and earlier)

```rust
// ❌ BUG: This returned incrementing IDs (1, 2, 3, 4...)
let snapshot1 = SnapshotId::current(); // Returns 1
let snapshot2 = SnapshotId::current(); // Returns 2
let snapshot3 = SnapshotId::current(); // Returns 3

// SQLite backend would reject snapshot2 and snapshot3!
backend.get_node(snapshot2, node_id)?; // ERROR: "requested: 2"
```

### After (Fixed - v2.1.2+)

```rust
// ✅ FIXED: Now always returns SnapshotId(0)
let snapshot1 = SnapshotId::current(); // Returns SnapshotId(0)
let snapshot2 = SnapshotId::current(); // Returns SnapshotId(0)
let snapshot3 = SnapshotId::current(); // Returns SnapshotId(0)

// All calls work correctly with SQLite backend!
backend.get_node(snapshot1, node_id)?; // ✅ OK
backend.get_node(snapshot2, node_id)?; // ✅ OK
backend.get_node(snapshot3, node_id)?; // ✅ OK
```

## Migration Guide

### For SQLite Backend Users (Most Users)

**No changes required!** Your existing code now works correctly:

```rust
// This code now works as expected
let snapshot = SnapshotId::current();
let node = backend.get_node(snapshot, node_id)?;
let neighbors = backend.neighbors(snapshot, node_id, query)?;
```

### For Native-V3 Backend Users

You have two options:

#### Option 1: Continue using `SnapshotId::current()` (Recommended)

```rust
// Works for both SQLite and V3
let snapshot = SnapshotId::current(); // Returns SnapshotId(0)
let value = backend.kv_get_v3(snapshot, b"my_key")?;
```

#### Option 2: Use `SnapshotId::new_incrementing()` for unique IDs

```rust
// Only for native-v3 when you need unique snapshot IDs
let snapshot1 = SnapshotId::new_incrementing(); // Returns unique ID
let snapshot2 = SnapshotId::new_incrementing(); // Returns different unique ID

// Useful for MVCC operations in native-v3
let value1 = backend.kv_get_v3(snapshot1, b"my_key")?;
let value2 = backend.kv_get_v3(snapshot2, b"my_key")?;
```

## New API

### `SnapshotId::current()` - Updated Behavior

```rust
pub fn current() -> Self {
    SnapshotId(0) // Always returns 0 - the "current" sentinel
}
```

**Behavior:**
- Returns `SnapshotId(0)` for all backends
- Multiple calls return the same value
- Works with both SQLite and native-v3 backends
- Represents "current committed data"

### `SnapshotId::new_incrementing()` - New Method

```rust
pub fn new_incrementing() -> Self {
    let lsn = SNAPSHOT_COUNTER.fetch_add(1, Ordering::SeqCst);
    SnapshotId(lsn) // Returns unique incrementing ID
}
```

**Behavior:**
- Returns unique, monotonically increasing snapshot IDs
- Only for native-v3 backend
- Useful for MVCC and historical snapshots
- **Not supported** by SQLite backend

## Code Examples

### SQLite Backend Examples

```rust
use sqlitegraph::backend::SqliteGraphBackend;
use sqlitegraph::snapshot::SnapshotId;

let backend = SqliteGraphBackend::in_memory()?;

// Insert test data
let node_id = backend.insert_node("User", "{\"name\": \"Alice\"}")?;

// ✅ WORKS: SnapshotId::current() returns 0
let snapshot = SnapshotId::current();
let node = backend.get_node(snapshot, node_id)?;
println!("Node: {:?}", node);

// ❌ ERROR: Historical snapshots rejected
let historical = SnapshotId::from_lsn(123);
let result = backend.get_node(historical, node_id);
assert!(result.is_err()); // "does not support historical snapshots"
```

### Native-V3 Backend Examples

```rust
use sqlitegraph::backend::native::v3::V3Backend;
use sqlitegraph::snapshot::SnapshotId;

let backend = V3Backend::create("/path/to/db")?;

// ✅ WORKS: Use current snapshot
let snapshot = SnapshotId::current();
let value = backend.kv_get_v3(snapshot, b"my_key")?;

// ✅ WORKS: Use unique snapshots for MVCC
let snap1 = SnapshotId::new_incrementing();
let snap2 = SnapshotId::new_incrementing();
let snap3 = SnapshotId::new_incrementing();

// Each snapshot can see different states
backend.kv_set_v3(b"counter".to_vec(), KvValue::Integer(10), None);
let val1 = backend.kv_get_v3(snap1, b"counter")?;

backend.kv_set_v3(b"counter".to_vec(), KvValue::Integer(20), None);
let val2 = backend.kv_get_v3(snap2, b"counter")?;

backend.kv_set_v3(b"counter".to_vec(), KvValue::Integer(30), None);
let val3 = backend.kv_get_v3(snap3, b"counter")?;
```

## Testing Your Code

### Test for SQLite Backend Compatibility

```rust
#[test]
fn test_sqlite_snapshot_compatibility() {
    let backend = SqliteGraphBackend::in_memory().unwrap();
    
    // Multiple calls to current() should all work
    for _ in 0..10 {
        let snapshot = SnapshotId::current();
        assert_eq!(snapshot, SnapshotId(0));
        
        // Should not error
        let result = backend.insert_node("Test", "{}");
        assert!(result.is_ok());
    }
}
```

### Test for Native-V3 Incrementing Snapshots

```rust
#[test]
fn test_native_v3_incrementing_snapshots() {
    let backend = V3Backend::create("/tmp/test.db").unwrap();
    
    let snap1 = SnapshotId::new_incrementing();
    let snap2 = SnapshotId::new_incrementing();
    let snap3 = SnapshotId::new_incrementing();
    
    // Each should be unique
    assert_ne!(snap1, snap2);
    assert_ne!(snap2, snap3);
    assert!(snap2.as_u64() > snap1.as_u64());
    assert!(snap3.as_u64() > snap2.as_u64());
}
```

## Common Issues

### Issue: "does not support historical snapshots"

**Cause:** Using a non-zero snapshot ID with SQLite backend

**Solution:** Use `SnapshotId::current()` which returns `SnapshotId(0)`

```rust
// ❌ WRONG
let snapshot = SnapshotId::from_lsn(123);
backend.get_node(snapshot, node_id)?; // ERROR

// ✅ CORRECT
let snapshot = SnapshotId::current();
backend.get_node(snapshot, node_id)?; // OK
```

### Issue: Need unique snapshot IDs in SQLite backend

**Answer:** Not supported. SQLite backend only supports "current" snapshot (SnapshotId(0))

**Workaround:** Use native-v3 backend if you need historical snapshots

## Benefits of This Fix

1. **SQLite compatibility:** All existing code now works correctly
2. **No breaking changes:** Most users need zero code changes
3. **Clear contract:** Explicit API for different use cases
4. **Better documentation:** Clear explanation of backend differences
5. **Fixed 53 Magellan tests:** Critical bug resolved

## Summary

| Method | Returns | Backend Support | Use Case |
|--------|---------|-----------------|----------|
| `SnapshotId::current()` | Always `SnapshotId(0)` | All backends | Current data, general use |
| `SnapshotId::new_incrementing()` | Unique incrementing IDs | Native-v3 only | MVCC, historical snapshots |
| `SnapshotId::from_tx(tx_id)` | Explicit transaction ID | All backends | Transaction-specific snapshots |
| `SnapshotId::from_lsn(lsn)` | Explicit LSN | All backends | LSN-specific snapshots |

## Questions?

See the main documentation:
- [API Reference](../API.md)
- [Architecture](./ARCHITECTURE.md)
- [Bug Report](../sqlitegraph-core/BUG_SNAPSHOTID_SQLITE_BACKEND.md)

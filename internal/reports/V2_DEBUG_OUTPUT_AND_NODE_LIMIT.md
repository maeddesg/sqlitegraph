# V2 Debug Output and Node Limit Context

**Date:** 2026-02-09
**Status:** Design Decision Record

---

## Part 1: V2 Slot Allocation Debug Output (REMOVED in v1.5.4)

### Location
`sqlitegraph/src/backend/native/node_store.rs`

### Status
**Removed in v1.5.4** - The V2_SLOT_DEBUG instrumentation has been removed from the codebase.

### What Was Removed
- All `println!` debug statements with `[V2_SLOT_DEBUG]` prefix
- `V2_SLOT_DEBUG` environment variable checks
- `SLOT_CORRUPTION_DEBUG` environment variable checks
- Phase 76 `trace_v2_io` feature instrumentation
- Phase 2C forensic dual-API instrumentation
- Approximately 100 lines of debug scaffolding

### Rationale for Removal
1. Zero production value - only useful when actively debugging corruption
2. Cluttered the codebase with vestigial forensic code
3. Better alternatives exist: proper logging framework, debuggers, targeted tests
4. Can be restored from git history if needed for future debugging

### If You Need Debug Output
The debug code can be restored from git history:

```bash
# View the debug code before v1.5.4
git show v1.5.3:sqlitegraph/src/backend/native/node_store.rs
```

---

## Part 2: The 2048 Node Limit

### Current Limit (V2)

```rust
// src/backend/native/graph_file/mod.rs:58
pub const RESERVED_NODE_REGION_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
const NODE_SLOT_SIZE: u64 = 4096; // 4KB per node

// Calculation: 8 MiB / 4 KB = 2048 nodes (hard limit)
```

### Why This Limit Exists

V2 trades scale for speed:
- **O(1) node lookup** via direct slot indexing: `slot_offset = base + (node_id * 4096)`
- Fixed slot layout enables this, but creates hard ceiling
- No dynamic allocation for node metadata

### Error When Exceeded
```rust
// src/backend/native/node_store.rs:40-47
if node_slot_offset >= max_node_offset {
    return Err(NativeBackendError::CorruptFreeSpace {
        reason: format!(
            "Node region overflow: node_id={} would exceed reserved region. \
            Increase RESERVED_NODE_REGION_BYTES or implement node relocation.",
            next_id, node_slot_offset, max_node_offset
        ),
    });
}
```

### Why You Can't Just "Remove" the Limit in V2

| Option | Problem |
|--------|---------|
| **Increase `RESERVED_NODE_REGION_BYTES`** | Still finite, bloats header, wastes space for small DBs |
| **Dynamic allocation in V2** | Requires B+tree index → breaks V2's O(1) lookup → new format anyway |

### V2 vs SQLite Backend

```
SQLiteGraph
├── SQLite Backend
│   ├── Arbitrary scale (proven)
│   ├── Mature tooling (sqlite3, dumps, etc.)
│   └── 10-100x slower on graph traversals
│
└── Native V2 Backend
    ├── Clustered edge storage (I/O locality)
    ├── MVCC snapshot isolation
    ├── WAL-based transactions
    ├── KV store + Pub/Sub
    ├── HNSW vector search
    └── Hard limit: 2048 nodes
```

### Decision: Create native-v3

**Rationale:**
1. The 2048 limit is **architectural**, not a constant you can just bump
2. V2 remains optimal for small graphs (<2048 nodes) with O(1) lookups
3. V3 properly addresses scale rather than hacking V2

### V3 Requirements (Already Planned)

From `.planning/REQUIREMENTS.md`:

**Storage Engine:**
- **STOR-01**: Dynamic page allocation → unlimited nodes (4B+ theoretical limit)
- **STOR-02**: B+tree index → O(log n) lookup (acceptable trade-off)
- **STOR-03**: Node pages (16KB) store compressed records (~64 nodes/page vs 1/slot in V2)
- **STOR-04**: Page state tracking prevents double-free corruption
- **STOR-05**: Page checksums detect corruption early
- **STOR-06**: LRU cache for node lookups mitigates O(log n) cost
- **STOR-07**: Free list + bitmap allocator for O(1) page allocation

**Header & File Format:**
- **FMT-01**: V3 magic bytes (`SQLTGF03`) differentiate from V2 (`SQLTGF00`)
- **FMT-02**: PersistentHeaderV3 extends V2 with B+tree root page metadata
- **FMT-03**: Version auto-detection selects backend on open
- **FMT-04**: GraphConfig::native_v3() API for backend selection
- **FMT-05**: Backward compatible - V2 databases still openable

**WAL & Transactions:**
- **WAL-01** through **WAL-05**: Page allocation transactional, WAL replay validates checksums

**Feature Parity:**
- **PARITY-01** through **PARITY-07**: All 530+ V2 tests pass with V3 storage

**Migration:**
- **MIG-01** through **MIG-05**: V2→V3 offline migration tool with backup

### V3 Phases (Roadmap)

| Phase | Focus | Status |
|-------|-------|--------|
| Phase 61 | FMT-01 through FMT-05 (Header & File Format) | Pending |
| Phase 62 | STOR-01, STOR-03, STOR-04, STOR-05, STOR-07 (Storage Engine) | Pending |
| Phase 63 | STOR-02 (B+tree index) | Pending |
| Phase 64 | WAL-01 through WAL-05 (WAL & Transactions) | Pending |
| Phase 65 | PARITY-01 through PARITY-07 (Feature Parity) | Pending |
| Phase 66 | MIG-01 through MIG-05 (Migration & Tooling) + STOR-06 (LRU cache) | Pending |

### Performance Trade-offs

| Metric | V2 | V3 | SQLite |
|--------|-----|-----|--------|
| Max nodes | 2,048 | 4B+ | Unlimited |
| Node lookup | O(1), ~20ns | O(log n), ~40-80ns | Variable |
| Best workload | Small, branching graphs | Any graph | Any graph |
| Chain traversal | 100ms (1000 nodes) | TBD | 10ms (1000 nodes) |

### Conclusion

**V2 is not broken** - it's designed for a specific use case (small graphs with fast lookups). The 2048 limit is a feature, not a bug.

**V3 is the correct path** for unlimited scale. The planning is complete (27 requirements mapped to phases). Ready to start Phase 61.

---

## References

- `.planning/REQUIREMENTS.md` - V3 requirements specification
- `docs/ARCHITECTURAL_ANALYSIS.md` - Architectural analysis of V2 limitations
- `sqlitegraph/src/backend/native/node_store.rs` - V2 slot implementation
- `sqlitegraph/src/backend/native/graph_file/mod.rs` - RESERVED_NODE_REGION_BYTES constant

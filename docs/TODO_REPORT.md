# SQLiteGraph TODO Report

**Generated:** 2026-02-20
**Scope:** V3 Backend and Native Backend (V2 excluded as deprecated)

## Summary

| Category | Count |
|----------|-------|
| V3 Backend | 20 |
| Native Backend | 9 |
| **Total** | **29** |

---

## V3 Backend TODOs (20)

### `edge_compat.rs` - Edge Store Compatibility Layer

| Line | Priority | Description |
|------|----------|-------------|
| 312 | High | Create proper WAL record for edge insert |
| 348 | High | Write dirty clusters to pages |
| 349 | High | Update B+Tree index |
| 350 | High | WAL checkpoint |

**Note:** These are critical for durability. The `flush()` function is essentially a stub.

### `adjacency.rs` - Adjacency Operations

| Line | Priority | Description |
|------|----------|-------------|
| 20 | Medium | Phase 66: B+Tree-based neighbor lookup |
| 34 | Medium | Phase 66: B+Tree-based neighbor lookup |
| 48 | Medium | Phase 66: B+Tree-based degree lookup |
| 61 | Medium | Phase 66: B+Tree-based degree lookup |

**Note:** These are stub implementations pending Phase 66.

### `kv_store/store.rs` - Key-Value Storage

| Line | Priority | Description |
|------|----------|-------------|
| 200 | Low | Implement background cleanup |

### `backend.rs` - V3 Backend Implementation

| Line | Priority | Description |
|------|----------|-------------|
| 804 | Medium | kind_offset (currently placeholder 0) |
| 805 | Medium | name_offset (currently placeholder 0) |
| 885 | Low | Add file_path to compact format if needed |
| 1084 | High | Implement edge type filtering |
| 1115 | High | Apply kind filter from step.target_kind |
| 1132 | High | Implement pattern matching |
| 1197 | Medium | Calculate checksum |
| 1199 | Low | Measure duration |
| 1241 | High | Implement snapshot import |
| 1255 | High | Implement kind-based query using string table |
| 1266 | High | Implement pattern-based query |

---

## Native Backend TODOs (9)

### `graph_backend.rs` - Native Graph Backend

| Line | Priority | Description |
|------|----------|-------------|
| 225 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 368 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 382 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 396 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 413 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 437 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 460 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |
| 474 | Medium | Pass snapshot_id to filter WAL records (Phase 38-04) |

**Note:** These are all related to Phase 38-04 snapshot isolation work.

### `adjacency/helpers.rs` - Adjacency Helpers

| Line | Priority | Description |
|------|----------|-------------|
| 274 | Medium | Phase 61-02: Full WAL record integration |

---

## Priority Breakdown

| Priority | Count |
|----------|-------|
| High | 9 |
| Medium | 17 |
| Low | 3 |

---

## By Phase Reference

| Phase | Count | Items |
|-------|-------|-------|
| Phase 38-04 | 8 | Snapshot ID filtering in WAL records |
| Phase 61-02 | 1 | WAL record integration |
| Phase 66 | 4 | B+Tree-based neighbor/degree lookup |
| Unphased | 16 | Various implementation gaps |

---

## Recommendations

1. **Critical Path:** The `edge_compat.rs` WAL-related TODOs (4 items) block durability guarantees
2. **Query Features:** Pattern matching, edge type filtering, and kind-based queries are high-priority user-facing features
3. **Phase Alignment:** Consider completing Phase 38-04 snapshot work before Phase 66 B+Tree lookups

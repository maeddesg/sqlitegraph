# Codebase Mapping Summary

**Generated:** 2026-02-13
**Focus:** V3 Storage Architecture

## Documents Updated

| Document | Lines | Focus |
|-----------|--------|--------|
| `ARCHITECTURE.md` | 1,502 | V3 storage architecture, layers, data structures |

**Total V3 documentation: 1,502 lines**

## ARCHITECTURE.md Contents

### Layers Documented

1. **B+Tree Index Layer** (`v3/btree.rs`)
   - BTreeManager with page cache
   - IndexPage (Internal/Leaf variants)
   - O(log n) lookup operations

2. **Node Storage Layer** (`v3/node/`)
   - NodeRecordV3: 44-byte fixed metadata + variable inline data
   - NodePage: 4KB page with delta/varint compression
   - NodeStore: LRU cache with B+Tree integration

3. **Page Allocation Layer** (`v3/allocator.rs`)
   - PageAllocator with bitmap and free list
   - Double-free prevention
   - O(1) allocation/deallocation

4. **WAL Layer** (`v3/wal.rs`)
   - V3WALHeader: 64-byte header
   - 8 WAL record types (PageAllocate, PageFree, PageWrite, BTreeSplit, etc.)
   - WALRecovery and WALWriter

5. **Edge Compatibility Layer** (`v3/edge_compat.rs`)
   - V3EdgeStore using V2 CompactEdgeRecord format
   - Direction enum (Outgoing/Incoming)
   - PageType enum for page classification

6. **Compression Layer** (`v3/compression/`)
   - Delta encoding: `node_id - base_id`
   - Varint encoding: 1-10 bytes per value
   - ~40% space savings for typical nodes

7. **Header Module** (`v3/header.rs`)
   - PersistentHeaderV3: 112 bytes (80 V2 + 32 V3)
   - V3 fields: root_index_page, free_page_list_head, total_pages, page_size, btree_height

8. **Backend Module** (`v3/backend.rs`)
   - V3Backend implementing GraphBackend trait
   - RwLock-protected components
   - WAL integration optional

### Key Data Structures

- `NodeRecordV3`: In-memory node (44 bytes fixed + variable)
- `NodePage`: 4KB page (32-byte header + 4064-byte data region)
- `IndexPage`: B+Tree index page (Internal: keys+children, Leaf: entries+next_leaf)
- `V3WALRecord`: 8 enum variants for WAL logging
- `PageAllocator`: Bitmap + singly-linked free list

### File Organization

```
database.graph          # Main database (header + pages)
database.graph.v3wal    # Write-Ahead Log
database.graph.v3checkpoint # Checkpoint snapshot
```

### Page Types

- Free (0), BTreeIndex (1), NodeData (2), EdgeCluster (3), Wal (4), Checkpoint (5)

### Key Files Referenced

- `sqlitegraph/src/backend/native/v3/mod.rs`
- `sqlitegraph/src/backend/native/v3/header.rs`
- `sqlitegraph/src/backend/native/v3/constants.rs`
- `sqlitegraph/src/backend/native/v3/backend.rs`
- `sqlitegraph/src/backend/native/v3/btree.rs`
- `sqlitegraph/src/backend/native/v3/node/mod.rs`
- `sqlitegraph/src/backend/native/v3/node/record.rs`
- `sqlitegraph/src/backend/native/v3/node/page.rs`
- `sqlitegraph/src/backend/native/v3/wal.rs`
- `sqlitegraph/src/backend/native/v3/allocator.rs`
- `sqlitegraph/src/backend/native/v3/index/page.rs`
- `sqlitegraph/src/backend/native/v3/edge_compat.rs`
- `sqlitegraph/src/backend/native/v3/compression/mod.rs`
- `sqlitegraph/src/backend/native/v3/adjacency.rs`

---

*Codebase mapping summary: 2026-02-13*

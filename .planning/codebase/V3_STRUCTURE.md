# V3 Module Structure

**Analysis Date:** 2025-02-13

## Directory Layout

```
sqlitegraph/src/backend/native/v3/
├── mod.rs              # Main V3 module exports (69 lines)
├── header.rs          # Persistent header structure (635 lines)
├── constants.rs       # Page sizes, checksums, magic bytes (175 lines)
├── allocator.rs       # Page allocation/free tracking (545 lines)
├── wal.rs            # Write-Ahead Logging (1751 lines)
├── btree.rs         # B+Tree index manager (850 lines)
├── backend.rs        # NativeBackendV3 implementation (935 lines)
├── adjacency.rs      # V3 adjacency helpers - STUB (95 lines)
├── edge_compat.rs    # V2 edge format compatibility (379 lines)
├── node/            # Node storage subsystem
│   ├── mod.rs        # Node subsystem exports (70 lines)
│   ├── record.rs     # NodeRecordV3 type definition (828 lines)
│   ├── page.rs       # NodePage serialization (1204 lines)
│   ├── store.rs      # NodeStore with traversal cache (1169 lines)
│   └── tests.rs     # Node subsystem unit tests (773 lines)
├── index/           # B+Tree index pages
│   ├── mod.rs        # Index subsystem exports (64 lines)
│   └── page.rs      # IndexPage for tree nodes (759 lines)
└── compression/      # Data compression algorithms
    ├── mod.rs        # Compression exports (100 lines)
    ├── delta.rs      # Delta encoding for node IDs (373 lines)
    └── varint.rs     # Variable-length integer encoding (640 lines)
```

**Total Lines: 11,414** across 18 Rust files

## Module Purposes

**v3/mod.rs** (69 lines)
- Purpose: Main entry point and re-exports for V3 backend
- Contains: `pub use` declarations for all public V3 components
- Exports: `NativeBackendV3`, `PersistentHeaderV3`, `BTreeManager`, `NodeStore`, `PageAllocator`, `WALWriter`
- Feature-gated: `#[cfg(feature = "native-v3")]`

**v3/header.rs** (635 lines)
- Purpose: Database file header with metadata and magic bytes
- Types: `PersistentHeaderV3`, `StorageFormat`, `PageType`
- Key exports: `PersistentHeaderV3`, `MagicNumberV3`, `V3_FORMAT_VERSION`
- Dependencies: `constants`, `compression::varint`
- Operations: Serialization/deserialization, validation, checksum verification

**v3/constants.rs** (175 lines)
- Purpose: Page size limits, checksum algorithms, magic values
- Exports: `DEFAULT_PAGE_SIZE` (4096), `MAX_PAGE_SIZE` (65536), checksum functions
- Functions: `xor_checksum`, `fnv1a_checksum`, `validate_page_size`
- Constants: Magic bytes, version numbers, size limits

**v3/allocator.rs** (545 lines)
- Purpose: Track page allocation and free pages
- Types: `PageAllocator`, `FreePageList`, `PageAllocation`
- Dependencies: `header`, `constants`, `wal`
- Exports: `PageAllocator`, `FreePageList`
- Operations: `allocate_page`, `free_page`, `get_free_page`, checkpoint handling

**v3/wal.rs** (1751 lines)
- Purpose: Write-Ahead Logging for crash recovery
- Types: `V3WALHeader`, `V3WALRecord`, `WALWriter`, `WALRecovery`, `WALRecoveryStats`
- Record types: PageAllocate, PageFree, PageWrite, BTreeSplit, Checkpoint, TransactionBegin/Commit/Rollback
- Dependencies: `header`, `constants`, `bincode`, `serde`
- File format: 64-byte header + variable-length records

**v3/btree.rs** (850 lines)
- Purpose: B+Tree index for node_id -> page_id mapping
- Types: `BTreeManager`, `BTreeConfig`, `BTreeStats`, `BTreeIterator`
- Operations: `insert`, `lookup`, `delete`, `split`, `merge`
- Dependencies: `index/page`, `header`, `wal`, `allocator`
- Characteristics: O(log n) lookups, automatic splitting on overflow

**v3/backend.rs** (935 lines)
- Purpose: Main `NativeBackendV3` implementation
- Types: `NativeBackendV3`, `V3Config`
- Traits: Implements `GraphBackend`, `StorageBackend`
- Dependencies: All V3 subsystems (btree, node, allocator, wal, edge_compat)
- Operations: Full graph CRUD, traversal support, query operations

**v3/adjacency.rs** (95 lines)
- Purpose: Neighbor query interface (STUB - Phase 66)
- Types: `V3AdjacencyHelpers`
- Status: Returns empty vectors, TODO for B+Tree integration
- Dependencies: `types` (from parent), `snapshot`
- Functions: `get_outgoing_neighbors`, `get_incoming_neighbors`, `outgoing_degree`, `incoming_degree`

**v3/edge_compat.rs** (379 lines)
- Purpose: V2 EdgeCluster format compatibility layer
- Types: `V3EdgeCluster`, `V3EdgeStore`, `PageType`, `Direction`
- Dependencies: `v2::edge_cluster`, `btree`, `wal`
- Status: Temporary design for V3 migration
- Operations: `lookup_cluster`, `insert_edge`, `neighbors`

**v3/node/record.rs** (828 lines)
- Purpose: NodeRecordV3 type with inline/external data
- Types: `NodeRecordV3`, `InlineData`, `ExternalData`, `NodeData`
- Storage: Inline data (max 64 bytes) or external reference
- Dependencies: `types`, `compression::delta`, `compression::varint`
- Operations: `new_inline`, `new_external`, serialization/deserialization, size calculation

**v3/node/page.rs** (1204 lines)
- Purpose: NodePage container with pack/unpack serialization
- Types: `NodePage`, `PageHeader`, `PageStats`, `PageFlags`
- Operations: `add_node`, `pack`, `unpack`, checksum validation, space management
- Dependencies: `record`, `compression`, `constants`
- Structure: 32-byte header + variable node data + checksum

**v3/node/store.rs** (1169 lines)
- Purpose: NodeStore with LRU traversal cache
- Types: `NodeStore`, `TraversalCache`, `StoreStats`, `CacheConfig`
- Operations: `get_node`, `load_page`, `cache_get`, `cache_insert`, bulk operations
- Dependencies: `page`, `btree`, `compression`
- Cache: LRU-based with hit/miss tracking

**v3/index/page.rs** (759 lines)
- Purpose: B+Tree page structures (internal/leaf)
- Types: `IndexPage`, `IndexPageType`, `IndexEntry`, `InternalPage`, `LeafPage`
- Operations: Key search, insert, split, merge, serialization
- Dependencies: `compression::varint`
- Characteristics: Variable fanout, prefix compression

**v3/compression/delta.rs** (373 lines)
- Purpose: Delta encoding for node IDs
- Functions: `encode_id_delta`, `decode_id_delta`, `calculate_optimal_base_id`
- Formula: `id_delta = (node.id - base_id) as u32`
- Benefits: 4 bytes saved per node (i64 -> u32)

**v3/compression/varint.rs** (640 lines)
- Purpose: Variable-length integer encoding (7 bits per byte)
- Functions: `encode_varint`, `decode_varint`, `varint_size`
- Types: `VarintError`
- Encoding: MSB continuation bit, 7 data bits per byte
- Maximum: 10 bytes for u64::MAX

**v3/node/tests.rs** (773 lines)
- Purpose: Comprehensive unit tests for NodeStore V3 components
- Test coverage: B+Tree lookup, page loading/decompression, traversal cache, error handling
- Test utilities: `create_test_node`, `create_test_page`, `verify_round_trip`

## Key Type Definitions

**NodeRecordV3** (`node/record.rs`, 828 lines)
```rust
pub struct NodeRecordV3 {
    // Common fields (12 bytes)
    pub flags: NodeFlags,
    pub kind_offset: u16,      // Offset to kind string
    pub name_offset: u16,       // Offset to name string

    // Inline data (<= 64 bytes)
    pub data_inline: Option<Vec<u8>>,

    // External data (> 64 bytes) (16 bytes)
    pub data_page_id: u64,
    pub data_offset: u32,
    pub data_len: u32,

    // Edge cluster references (16 bytes)
    pub outgoing_page_id: u64,
    pub outgoing_len: u32,
    pub incoming_page_id: u64,
    pub incoming_len: u32,
}
```
Total: 44 bytes base + variable inline data

**NodePage** (`node/page.rs`, 1204 lines)
```rust
pub struct NodePage {
    pub page_id: u64,
    pub header: PageHeader,
    pub nodes: Vec<NodeRecordV3>,
    pub compressed_data: Option<Vec<u8>>,
}

struct PageHeader {  // 32 bytes
    pub page_type: PageType,       // 1 byte
    pub node_count: u16,            // 2 bytes
    pub compressed_size: u16,         // 2 bytes
    pub checksum: u32,               // 4 bytes
    pub base_node_id: i64,           // 8 bytes
    pub reserved: [u8; 15],         // 15 bytes
}
```
Page size: 4096 bytes (default)

**IndexPage** (`index/page.rs`, 759 lines)
```rust
pub enum IndexPage {
    Leaf(LeafPage),
    Internal(InternalPage),
}

pub struct IndexEntry {
    pub key: i64,        // node_id
    pub page_id: u64,    // target page
}

pub struct InternalPage {
    pub page_id: u64,
    pub entries: Vec<IndexEntry>,
    pub children: Vec<u64>,  // One more than entries
}

pub struct LeafPage {
    pub page_id: u64,
    pub entries: Vec<IndexEntry>,
    pub next_leaf: Option<u64>,
}
```

**V3WALRecord** (`wal.rs`, 1751 lines)
```rust
pub enum V3WALRecord {
    PageAllocate { lsn: u64, page_id: u64, timestamp: u64 },
    PageFree { lsn: u64, page_id: u64, checksum: u32, timestamp: u64 },
    PageWrite { lsn: u64, page_id: u64, offset: u32, data: Vec<u8>, checksum: u32, timestamp: u64 },
    BTreeSplit { lsn: u64, original_page_id: u64, new_page_id: u64, split_key: u64, page_type: u8, timestamp: u64 },
    Checkpoint { lsn: u64, root_page_id: u64, total_pages: u64, btree_height: u32, free_page_list_head: u64, header_snapshot: Vec<u8>, timestamp: u64 },
    TransactionBegin { tx_id: u64, lsn: u64, timestamp: u64 },
    TransactionCommit { tx_id: u64, lsn: u64, timestamp: u64 },
    TransactionRollback { tx_id: u64, lsn: u64, timestamp: u64 },
}
```

**PersistentHeaderV3** (`header.rs`, 635 lines)
```rust
pub struct PersistentHeaderV3 {
    pub magic: [u8; 8],           // "SQLTG\0\0"
    pub version: u32,                // Format version
    pub page_size: u32,              // Page size (4096)
    pub format_version: u8,          // V3 format
    pub root_page_id: u64,           // B+Tree root
    pub btree_height: u32,           // Tree height
    pub total_pages: u64,            // Total pages
    pub free_page_list_head: u64,    // Free list
    pub wal_page_id: u64,           // WAL root page
    pub checksum: u32,               // Header checksum
}
```
Header size: 64 bytes

## Module Dependencies

**Dependency Graph (lower depends on higher):**

```
backend.rs (935)
    +-> btree.rs (850)
    |       +-> index/page.rs (759)
    |               +-> compression/varint.rs (640)
    +-> node/store.rs (1169)
    |       +-> node/page.rs (1204)
    |       |       +-> node/record.rs (828)
    |       |               +-> compression/delta.rs (373)
    |       |               +-> compression/varint.rs (640)
    |       +-> btree.rs
    +-> allocator.rs (545)
    |       +-> header.rs (635)
    |       +-> wal.rs (1751)
    +-> wal.rs
    |       +-> header.rs
    |       +-> constants.rs (175)
    +-> edge_compat.rs (379)
    |       +-> btree.rs
    |       +-> wal.rs
    +-> adjacency.rs (95, STUB)
```

**Cross-subsystem dependencies:**
- `node/` depends on `compression/` (delta and varint encoding)
- `index/` depends on `compression/` (varint encoding)
- `btree.rs` depends on `index/page`
- `wal.rs` depends on `header`, `constants`
- `backend.rs` depends on all subsystems
- `edge_compat.rs` depends on `btree`, `wal`, and V2 edge_cluster

**External dependencies:**
- `bincode` - WAL record serialization
- `serde` - Derive macros for serialization
- Parent `types` module - NativeNodeId, NativeResult, NodeFlags
- V2 `edge_cluster` - CompactEdgeRecord for compatibility

## Naming Conventions

**Files:**
- Module: `kebab-case.rs` (e.g., `node/store.rs`, `compression/delta.rs`)
- Tests: `tests.rs` co-located with module (e.g., `node/tests.rs`)
- Subsystem directory: `name/` containing `mod.rs` + implementation files

**Types:**
- Structs: `PascalCase` (e.g., `NodeRecordV3`, `IndexPage`, `WALWriter`)
- Enums: `PascalCase` (e.g., `PageType`, `V3WALRecord`, `IndexPageType`)
- V3 types use `V3` suffix (e.g., `NodeRecordV3`, `PersistentHeaderV3`, `V3WALRecord`)
- Page types use `Page` suffix (e.g., `NodePage`, `IndexPage`)
- Manager types use `Manager` suffix (e.g., `BTreeManager`, `PageAllocator`)
- Helper types: `Helpers` suffix (e.g., `V3AdjacencyHelpers`)

**Functions:**
- Public: `snake_case` (e.g., `add_node`, `pack`, `unpack`, `allocate_page`)
- Private: `snake_case` (e.g., `compress_data`, `decompress_data`, `find_split_point`)
- Factory: `new`, `new_<variant>` (e.g., `IndexPage::new_leaf`, `WALWriter::new`)
- Predicates: `is_<state>` (e.g., `is_internal()`, `is_leaf()`, `is_empty()`)

**Constants:**
- Upper `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_PAGE_SIZE`, `MAX_INLINE_DATA`, `V3_WAL_MAGIC`)
- Prefixes by module: `PAGE_`, `WAL_`, `V3_`

**Type suffixes:**
- `V3` - V3-specific types (NodeRecordV3, PersistentHeaderV3)
- `Page` - Page structures (NodePage, IndexPage, WALHeader)
- `Manager` - Resource managers (BTreeManager, PageAllocator, WALWriter)
- `Config` - Configuration structs (BTreeConfig, CacheConfig)
- `Stats` - Statistics types (BTreeStats, StoreStats, WALRecoveryStats)

## Where to Add New Code

**New page type:**
- Implementation: `v3/new_page_type.rs`
- Tests: Inline `#[cfg(test)]` or separate module
- Export: Add `pub mod` to `v3/mod.rs`

**New compression algorithm:**
- Implementation: `v3/compression/algorithm.rs`
- Tests: Inline test module
- Export: Add to `v3/compression/mod.rs`

**B+Tree operation:**
- Internal: Add to `v3/btree.rs`
- Page-level: Add to `v3/index/page.rs`

**NodeStore feature:**
- Cache: Add to `v3/node/store.rs`
- Page: Add to `v3/node/page.rs`
- Record: Add to `v3/node/record.rs`

**WAL record type:**
- Add variant to `V3WALRecord` in `v3/wal.rs`
- Add `match` arm in `WALRecovery::apply_record`
- Add helper method (e.g., `WALWriter::new_record_type`)

**New subsystem:**
- Create directory: `v3/new_subsystem/`
- Add: `mod.rs` for exports
- Add: implementation files
- Export: Add to `v3/mod.rs`

## Special Directories

**v3/node/**
- Purpose: Node storage subsystem with records, pages, store, and cache
- Not generated: All hand-written Rust
- Tests co-located: `tests.rs` for integration-style unit tests
- Files: `mod.rs`, `record.rs`, `page.rs`, `store.rs`, `tests.rs`

**v3/index/**
- Purpose: B+Tree index pages (internal and leaf)
- Not generated: Hand-written Rust
- Key types: `IndexPage` (enum wrapping `InternalPage` and `LeafPage`)

**v3/compression/**
- Purpose: Data compression algorithms
- Not generated: Hand-written Rust
- Algorithms: `delta` (node ID encoding), `varint` (variable-length integers)

## Entry Points

**NativeBackendV3** (`v3/backend.rs`)
- Location: `sqlitegraph/src/backend/native/v3/backend.rs`
- Factory: `NativeBackendV3::new(config: V3Config) -> NativeResult<Self>`
- Responsibilities:
  - Initialize header, allocator, B+Tree, NodeStore, WAL
  - Route graph operations (add_node, get_node, etc.)
  - Coordinate checkpoint and recovery
- Trait implementations: `GraphBackend`, `StorageBackend`

**BTreeManager** (`v3/btree.rs`)
- Location: `sqlitegraph/src/backend/native/v3/btree.rs`
- Factory: `BTreeManager::new(config: BTreeConfig) -> Self`
- Operations: `insert`, `lookup`, `delete`, `split`, `merge`

**NodeStore** (`v3/node/store.rs`)
- Location: `sqlitegraph/src/backend/native/v3/node/store.rs`
- Factory: `NodeStore::new(allocator, btree, config) -> Self`
- Operations: `get_node`, `load_page`, `cache_get`, `cache_insert`

**WALWriter** (`v3/wal.rs`)
- Location: `sqlitegraph/src/backend/native/v3/wal.rs`
- Factory: `WALWriter::new(wal_path, start_lsn) -> Self`
- Operations: `append`, `flush`, `commit`, `truncate`, recovery helpers

**WALRecovery** (`v3/wal.rs`)
- Location: `sqlitegraph/src/backend/native/v3/wal.rs`
- Factory: `WALRecovery::new(wal_path) -> Self`
- Operations: `recover`, `apply_record`, `get_header_state`

## Data Structures Summary

| Component | Primary Type | Storage | Purpose |
|-----------|--------------|----------|---------|
| `node/record.rs` | `NodeRecordV3` | Inline (≤64B) or External | Node data with edge refs |
| `node/page.rs` | `NodePage` | 4096-byte pages | Node container with compression |
| `node/store.rs` | `NodeStore` | In-memory cache + pages | LRU cache + B+Tree lookup |
| `index/page.rs` | `IndexPage` | Internal/Leaf pages | B+Tree structure |
| `btree.rs` | `BTreeManager` | Tree metadata | node_id → page_id mapping |
| `wal.rs` | `V3WALRecord` | Append-only log | Crash recovery |
| `header.rs` | `PersistentHeaderV3` | 64-byte header | File metadata |
| `allocator.rs` | `PageAllocator` | In-memory | Page tracking |
| `compression/delta.rs` | Functions | N/A | Node ID compression |
| `compression/varint.rs` | Functions | N/A | Variable-length ints |

## File Size Ranks (by LOC)

1. `wal.rs` - 1,751 lines - Write-Ahead Log implementation
2. `node/store.rs` - 1,169 lines - NodeStore with traversal cache
3. `node/page.rs` - 1,204 lines - NodePage serialization
4. `backend.rs` - 935 lines - NativeBackendV3 main implementation
5. `node/record.rs` - 828 lines - NodeRecordV3 type definition
6. `btree.rs` - 850 lines - B+Tree index manager
7. `compression/varint.rs` - 640 lines - Variable-length integer encoding
8. `header.rs` - 635 lines - Persistent header structure
9. `index/page.rs` - 759 lines - IndexPage for tree nodes
10. `node/tests.rs` - 773 lines - Node subsystem unit tests
11. `allocator.rs` - 545 lines - Page allocation/free tracking
12. `compression/delta.rs` - 373 lines - Delta encoding for node IDs
13. `edge_compat.rs` - 379 lines - V2 edge format compatibility
14. `compression/mod.rs` - 100 lines - Compression exports
15. `adjacency.rs` - 95 lines - V3 adjacency helpers - STUB
16. `node/mod.rs` - 70 lines - Node subsystem exports
17. `mod.rs` - 69 lines - Main V3 module exports
18. `constants.rs` - 175 lines - Page sizes, checksums, magic bytes
19. `index/mod.rs` - 64 lines - Index subsystem exports

**Total: 11,414 lines**

---

*V3 Structure analysis: 2025-02-13*

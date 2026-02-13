# V3 Storage Architecture

**Analysis Date:** 2026-02-13

## Overview

V3 is the third-generation native storage format for sqlitegraph, designed to overcome V2's capacity limitations through B+Tree indexing and dynamic page allocation. V3 provides unlimited node capacity while maintaining space efficiency through compression.

**Key Design Goals:**
- **Unlimited capacity** via B+Tree node index (O(log n) lookup vs V2's O(1) array with fixed size)
- **Space efficiency** via delta/varint compression (~40% space savings for node records)
- **Crash recovery** via Write-Ahead Logging (WAL) for ACID-like durability
- **Page-based storage** with dynamic allocation and free list for space reuse

**Design Philosophy:**
- Logical separation: node IDs (i64) are independent from physical page locations (u64)
- Modularity: Index, storage, allocation, and WAL are independent components
- Evolutionary: Reuses V2 edge cluster format via compatibility layer

## Pattern Overview

**Overall:** B+Tree-indexed, page-based storage with WAL durability

**Key Characteristics:**
- **Page-oriented I/O**: 4KB default page size (configurable: 4096, 8192, 16384)
- **Copy-on-write**: WAL writes precede main file modifications
- **Two-level indexing**: B+Tree maps `node_id → page_id`, then page is loaded for node data
- **Compression**: Delta encoding for IDs, varint for variable-length fields
- **Free list management**: Singly-linked free pages for O(1) allocation/deallocation

## Layers

### B+Tree Index Layer

**Purpose:** Map logical node IDs (i64) to physical page IDs (u64) for O(log n) lookup

**Location:** `sqlitegraph/src/backend/native/v3/btree.rs`

**Key Types:**
- `BTreeManager`: Manages B+Tree operations with page cache
- `IndexPage`: Enum representing index pages
  - `IndexPage::Internal`: Contains keys (max 254) and child pointers (max 255)
  - `IndexPage::Leaf`: Contains (node_id, page_id) entries (max 254) with next_leaf pointer

**Operations:**
- `lookup(node_id)`: Traverse tree from root, binary search at each level
- `insert(node_id, page_id)`: Insert mapping, split pages if full
- `delete(node_id)`: Remove mapping (placeholder for full implementation)
- `split_page()`: Divide full page, propagate split key upward

**Constants:**
- `MAX_KEYS`: 254 (maximum keys per internal page)
- `MAX_ENTRIES`: 254 (maximum entries per leaf page)
- `MAX_CHILDREN`: 255 (children = keys + 1)
- `PAGE_HEADER_SIZE`: 32 bytes
- `MAX_TREE_HEIGHT`: 10 (safety limit)

**Depends on:** `PageAllocator` for page lifecycle

**Used by:** `V3Backend`, `NodeStore` for all index lookups

---

### Node Storage Layer

**Purpose:** Store `NodeRecordV3` entries in variable-size pages with compression

**Location:** `sqlitegraph/src/backend/native/v3/node/`

**Submodules:**
- `record.rs`: NodeRecordV3 definition (44 bytes fixed metadata + variable inline data)
- `page.rs`: NodePage with delta/varint compression
- `store.rs`: NodeStore with traversal cache and B+Tree integration
- `tests.rs`: Comprehensive unit tests

**Key Types:**
- `NodeRecordV3`: In-memory node representation
  - Fixed metadata: 44 bytes (id, flags, kind_offset, name_offset, data_len, cluster offsets, edge counts)
  - Inline data: 0-64 bytes stored in record
  - External data: >64 bytes stored separately (offset reference)
- `NodePage`: 4KB page containing multiple nodes
  - Header: 32 bytes (page_id, next_page_id, node_count, used_bytes, base_id, checksum)
  - Data region: 4064 bytes with compressed node records
  - Base ID: Minimum node_id in page used for delta encoding
- `NodeStore`: High-level node operations
  - `TraversalCache`: LRU cache for frequently accessed nodes
  - `DEFAULT_CACHE_CAPACITY`: 16 entries
  - `MAX_NODE_CAPACITY`: 50 nodes (conservative estimate)

**Compression:**
- Delta encoding: `node_id - base_id` encoded as varint (saves ~4 bytes/node)
- Varint encoding: Variable-length fields use 1-10 bytes instead of fixed 2/4/8

**Constants:**
- `FIXED_METADATA_SIZE`: 44 bytes
- `MAX_INLINE_DATA`: 64 bytes
- `PAGE_HEADER_SIZE`: 32 bytes
- `MAX_PAGE_SIZE`: 4096 bytes
- `USABLE_SIZE`: 4064 bytes
- `ESTIMATED_NODE_SLOT_SIZE`: 80 bytes (conservative)

**Depends on:** `PageAllocator` for page management, `BTreeManager` for index lookups

**Used by:** `V3Backend` for all node CRUD operations

---

### Page Allocation Layer

**Purpose:** Dynamic page allocation with free list management for space reuse

**Location:** `sqlitegraph/src/backend/native/v3/allocator.rs`

**Key Types:**
- `PageAllocator`: Manages page lifecycle
  - `bitmap: Vec<bool>`: O(1) allocation status lookup
  - `free_list_head: u64`: Head of singly-linked free pages
  - `total_pages: u64`: Total pages in database
- `PageState`: Allocation status enum
  - `Free`: Page is on free list
  - `Allocated`: Page is in use
  - `Pinned`: Page cannot be freed (WAL operation in progress)
- `FreePageHeader`: Header for free pages (16 bytes)
  - `next_free: u64`: Next free page in list
  - `checksum: u64`: XOR checksum for validation

**Operations:**
- `allocate()`: Check free list first, then append to file
- `deallocate(page_id)`: Add page to free list head, update bitmap
- `get_page_state(page_id)`: Query allocation status
- `pin_page(page_id)`: Mark page as non-freeable during WAL operations
- `page_offset(page_id)`: Calculate byte offset in file
  - Formula: `V3_HEADER_SIZE + (page_id - 1) * PAGE_SIZE`
  - Page 0 is header, data pages start at page_id 1

**Double-Free Prevention:**
- Bitmap tracking prevents freeing already-free pages
- Returns `CorruptionDetected` error on double-free attempt

**Depends on:** `PersistentHeaderV3` for free_list_head and total_pages

**Used by:** All V3 components requiring page allocation

---

### WAL Layer

**Purpose:** Write-Ahead Logging for crash recovery and atomic operations

**Location:** `sqlitegraph/src/backend/native/v3/wal.rs`

**Key Types:**
- `V3WALHeader`: 64-byte WAL file header
  - Magic: `V3WAL_MAGIC` = "V3WAL\0\0"
  - Version: 1 (u32)
  - Page size: 4096 (default)
  - LSN tracking: current_lsn, committed_lsn, checkpointed_lsn
- `V3WALRecord`: Enum of 8 record types
  - `PageAllocate`: New page assigned from allocator
  - `PageFree`: Page returned to free list
  - `PageWrite`: Data written to page with checksum
  - `BTreeSplit`: Index page split during tree growth
  - `Checkpoint`: Database header snapshot for recovery
  - `TransactionBegin/Commit/Rollback`: Transaction boundaries
- `WALWriter`: Appends records to WAL file with buffering
  - `flush_threshold`: 64KB default buffer before fsync
- `WALRecovery`: Reads and replays WAL records after crash

**WAL File Format:**
- File extension: `.v3wal` (e.g., `database.graph` → `database.graph.v3wal`)
- Header: 64 bytes
- Records: `[size: u32] [serialized_record]`
- Record serialization: Bincode for complex types

**Recovery Process:**
1. Read WAL header and validate magic/version
2. Sequentially read records until EOF
3. Apply each valid record to in-memory state
4. Skip corrupt/invalid records (counted in stats)
5. Return final state and checkpoint header

**Paths:**
- `V3WALPaths::wal_file(db_path)`: Returns `{db}.v3wal`
- `V3WALPaths::checkpoint_file(db_path)`: Returns `{db}.v3checkpoint`
- `V3WALPaths::temp_checkpoint_file(db_path)`: Returns `{db}.v3checkpoint.tmp.{random}`

**LSN (Log Sequence Number):**
- `LSN_BEGIN`: 1 (first valid LSN)
- `LSN_INVALID`: 0 (uninitialized)
- Monotonically increasing with each WAL record

**Depends on:** Filesystem for WAL file management

**Used by:** `V3Backend` for durable operations when WAL enabled

---

### Edge Compatibility Layer

**Purpose:** Reuse V2 edge cluster format within V3's page-based storage

**Location:** `sqlitegraph/src/backend/native/v3/edge_compat.rs`

**Key Types:**
- `V3EdgeStore`: Edge storage wrapper
  - `btree: BTreeManager`: Index for (src, direction) → edge_page_id
  - `cache: HashMap<(i64, Direction), V3EdgeCluster>`: In-memory edge cache
- `V3EdgeCluster`: Grouped edges by source node
  - `src: i64`: Source node ID
  - `edges: Vec<CompactEdgeRecord>`: Destination nodes with edge data
  - `direction: Outgoing/Incoming`: Edge direction
  - `format_version: 1`: V2 compatibility format
- `Direction`: Enum for edge direction
  - `Outgoing`: Edges from source to targets
  - `Incoming`: Edges from targets to source
- `PageType`: Enum for page types
  - `BTreeIndex = 1`, `NodeData = 2`, `EdgeCluster = 3`, `Wal = 4`, `Checkpoint = 5`

**Serialization:**
- Format: `[version: 1 byte] [count: 4 bytes] [edges...]`
- Edge format: V2 `CompactEdgeRecord` (neighbor_id, type_offset, data)

**Current Limitations:**
- Stub implementations return empty results (TODO for Phase 66)
- No edge page persistence (TODO for future phases)

**Depends on:** `BTreeManager` for future edge indexing

**Used by:** `V3Backend` for `neighbors()`, `outgoing()`, `incoming()` operations

---

### Compression Layer

**Purpose:** Delta and varint encoding for space efficiency

**Location:** `sqlitegraph/src/backend/native/v3/compression/`

**Submodules:**
- `delta.rs`: ID delta encoding/decoding
- `varint.rs`: Variable-length integer encoding/decoding

**Key Functions:**
- `encode_id_delta(id, base)`: Encode difference as u32 varint
- `decode_id_delta(delta, base)`: Reconstruct full ID from delta and base
- `encode_varint(value)`: Encode u64 as 1-10 byte varint
- `decode_varint(bytes)`: Decode varint to u64
- `varint_size(value)`: Calculate encoded size without encoding

**Space Savings:**
- Delta encoding: ~4 bytes per node (i64 → u32 delta)
- Varint encoding: ~3 bytes per small value (u64/u32 → 1 byte)
- Combined: ~40% reduction for typical node records

**CompressionStats:**
- `original_size`: Uncompressed byte count
- `compressed_size`: Compressed byte count
- `compression_ratio`: original / compressed
- `space_savings_pct`: Percentage saved

**Depends on:** None (pure encoding utilities)

**Used by:** `NodePage` for packing/unpacking node records

---

### Header Module

**Purpose:** Persistent header definition for V3 database files

**Location:** `sqlitegraph/src/backend/native/v3/header.rs`

**Key Type: `PersistentHeaderV3` (112 bytes)**

**V3-Specific Fields (bytes 80-111):**
- `root_index_page: u64` (offset 80): Root B+Tree page ID
- `free_page_list_head: u64` (offset 88): Head of free page list
- `total_pages: u64` (offset 96): Total pages allocated
- `page_size: u32` (offset 104): Page size (4096/8192/16384)
- `btree_height: u32` (offset 108): Current B+Tree height

**V2-Preserved Fields (bytes 0-79):**
- Magic: `V3_MAGIC` = "SQLTGF\3\0" (distinguished by magic[7] = 3)
- Version: `V3_FORMAT_VERSION` = 4
- Flags: Feature flags (V2 inherited + V3 B+Tree flag)
- Node/edge counts, offsets for node/edge/cluster data
- Schema version, reserved fields

**Validation:**
- Magic number verification (V3 vs V2 vs unknown)
- Version compatibility check
- Offset ordering validation
- Page size validation (must be 4096, 8192, or 16384)
- B+Tree height sanity check (≤ MAX_BTREE_HEIGHT)

**Serialization:**
- `to_bytes()`: Serialize to 112-byte array (big-endian)
- `from_bytes()`: Deserialize from byte slice
- `detect_version()`: Detect V2 vs V3 from magic bytes

**Depends on:** Constants from `v3/constants.rs`

**Used by:** `V3Backend`, `PageAllocator`, `BTreeManager`

---

### Constants Module

**Purpose:** V3-specific constants and magic numbers

**Location:** `sqlitegraph/src/backend/native/v3/constants.rs`

**Key Constants:**
- `V3_MAGIC`: `[b'S', b'Q', b'L', b'T', b'G', b'F', 0, 3]`
- `V3_FORMAT_VERSION`: 4
- `V3_HEADER_SIZE`: 112 bytes (80 V2 + 32 V3)
- `DEFAULT_PAGE_SIZE`: 4096 bytes
- `MAX_BTREE_HEIGHT`: 4 (sufficient for 4B nodes)
- `PAGE_HEADER_SIZE`: 32 bytes
- `USABLE_PAGE_SIZE`: 4064 bytes

**Feature Flags:**
- `FLAG_V3_BTREE_INDEX`: 0x00000004 (B+Tree enabled)
- `FLAG_V3_DYNAMIC_ALLOCATION`: 0x00000008 (Dynamic page allocation)
- `DEFAULT_V3_FEATURE_FLAGS`: V2 flags | V3 flags

**Checksum:**
- `xor_checksum(data)`: XOR-based checksum with seed 0x5A5A5A5A5A5A5A
- `XOR_SEED`: Initialization constant for checksum calculation

**v3_flags submodule:**
- Feature flag definitions for V3
- Extends V2 flags with B+Tree and dynamic allocation

**compression submodule:**
- `MAX_ID_DELTA`: u32::MAX (maximum delta value)
- `MAX_VARINT_BYTES`: 10 (maximum varint encoded bytes)
- `SINGLE_BYTE_VARINT_MAX`: 0x7F (threshold for 1-byte varint)

**Depends on:** None (constants only)

**Used by:** All V3 modules

---

### Backend Module

**Purpose:** V3Backend implementation of GraphBackend trait

**Location:** `sqlitegraph/src/backend/native/v3/backend.rs`

**Key Type: `V3Backend`**

**Internal Structure:**
- `db_path: PathBuf`: Database file path
- `btree: RwLock<BTreeManager>`: B+Tree for node lookups
- `node_store: RwLock<NodeStore>`: Node storage operations
- `edge_store: RwLock<V3EdgeStore>`: Edge storage (compat layer)
- `allocator: RwLock<PageAllocator>`: Page allocation
- `wal: Option<RwLock<WALWriter>>`: Optional WAL for durability
- `header: RwLock<PersistentHeaderV3>`: Persistent header

**GraphBackend Trait Implementation:**
- `insert_node(NodeSpec)`: Create node, update B+Tree index, increment header count
- `insert_edge(EdgeSpec)`: Add edge via V3EdgeStore (both directions)
- `update_node(i64, NodeSpec)`: Update node record in place
- `delete_entity(i64)`: Remove node from B+Tree, decrement header count
- `get_node(SnapshotId, i64)`: Lookup node via B+Tree, deserialize from page
- `neighbors()`: Query edge store for neighbors
- `bfs()`: BFS traversal using edge store
- `shortest_path()`: Pathfinding using BFS
- `node_degree()`: Get in/out edge counts
- `k_hop()`: K-hop neighborhood traversal
- `chain_query()`: Multi-hop chain traversal
- `checkpoint()`: Flush WAL, persist header state
- `flush()`: Flush WAL to disk
- `backup()`: Copy database file and WAL
- `snapshot_export()`: Export database as snapshot

**Creation Methods:**
- `create(path)`: Create new database with V3 header
- `open(path)`: Open existing database, validate header, recover WAL
- `create_with_wal(path, enable_wal)`: Create with WAL enabled

**Error Mapping:**
- `map_v3_error(NativeBackendError)`: Convert to SqliteGraphError
- Preserves error context while translating to generic types

**Depends on:** All V3 submodules

**Used by:** Applications via `open_graph()` with `BackendKind::Native`

---

### Index Page Module

**Purpose:** B+Tree index page structures

**Location:** `sqlitegraph/src/backend/native/v3/index/page.rs`

**Key Type: `IndexPage` enum**

**Internal Page Variant:**
- `page_id: u64`: Page identifier
- `keys: Vec<u64>`: Split keys (max 254)
  - keys[i] = minimum key in child i+1
- `children: Vec<u64>`: Child page pointers (max 255 = keys + 1)
- `checksum: u32`: XOR checksum for validation

**Leaf Page Variant:**
- `page_id: u64`: Page identifier
- `entries: Vec<(u64, u64)>`: (node_id, page_id) mappings (max 254)
- `next_leaf: u64`: Link to next leaf for range queries
- `checksum: u32`: XOR checksum for validation

**Page Constants:**
- `PAGE_HEADER_SIZE`: 32 bytes
- `MAX_KEYS`: 254 (internal page keys)
- `MAX_ENTRIES`: 254 (leaf page entries)
- `MAX_CHILDREN`: 255 (internal page children)
- `KEY_SIZE`: 8 bytes (u64)
- `PAGE_ID_SIZE`: 8 bytes (u64)
- `ENTRY_SIZE`: 16 bytes (node_id + page_id)

**Operations:**
- `new_internal(page_id)`: Create empty internal page
- `new_leaf(page_id)`: Create empty leaf page
- `pack()`: Serialize page to 4096-byte array
- `unpack(bytes)`: Deserialize from bytes with validation
- `binary_search_leaf(entries, target)`: Binary search in leaf
- `find_child_index(keys, target)`: Find appropriate child
- `is_full_internal()`: Check if at capacity
- `is_full_leaf()`: Check if at capacity

**Binary Search:**
- Uses `Vec::binary_search_by_key()` for O(log n) lookup
- Returns `Result<usize, usize>` (found index or insertion point)

**Checksum:**
- Calculated over header + data region
- Stored at fixed offset in header
- Uses `xor_checksum()` from constants

**Depends on:** `v3/constants.rs` for checksum

**Used by:** `BTreeManager` for index operations

---

### Adjacency Module

**Purpose:** V3 adjacency helpers (stub implementation)

**Location:** `sqlitegraph/src/backend/native/v3/adjacency.rs`

**Key Type: `V3AdjacencyHelpers` (stub)**

**Stub Methods:**
- `get_outgoing_neighbors()`: Returns empty Vec (TODO: Phase 66)
- `get_incoming_neighbors()`: Returns empty Vec (TODO: Phase 66)
- `outgoing_degree()`: Returns 0 (TODO: Phase 66)
- `incoming_degree()`: Returns 0 (TODO: Phase 66)

**Purpose:** Provides same interface as V2's adjacency helpers

**Current Limitation:** All methods return empty/zero results

**Planned Integration:** Full B+Tree-based neighbor lookup in Phase 66

**Depends on:** `graph_file::GraphFile` (placeholder)

**Used by:** Native backend during graph traversal

## Data Flow

### Node Lookup Flow

1. Client calls `V3Backend::get_node(snapshot_id, node_id)`
2. `V3Backend` delegates to `node_store.lookup_node(node_id)`
3. `NodeStore` queries `btree.lookup(node_id)` via B+Tree index
4. `BTreeManager` traverses from root:
   - Load `IndexPage` from cache or disk
   - Binary search for target in Internal pages
   - Follow child pointer based on search result
   - At Leaf page, binary search entries for exact match
5. Return `Some(page_id)` where node is stored
6. `NodeStore` loads `NodePage` containing target node
7. `NodePage::unpack()` deserializes with delta/varint decompression
8. Locate and deserialize target `NodeRecordV3`
9. Return node data to caller

**Complexity:** O(log n) for B+Tree traversal + O(1) for page load

---

### Node Insert Flow

1. Client calls `V3Backend::insert_node(node_spec)`
2. Create `NodeRecordV3` from spec (with inline data)
3. `V3Backend` delegates to `node_store.insert_node(record)`
4. `NodeStore` determines target page:
   - Calculate compressed size using delta/varint estimation
   - Check if current page has capacity via `remaining_capacity()`
   - If full, request new page from `allocator.allocate()`
5. Update `base_id` if new node has minimum ID in page
6. Pack node into page at calculated offset using compression
7. Update B+Tree index: `btree.insert(node_id, page_id)`
8. If B+Tree page splits:
   - Allocate new page for split
   - Redistribute entries between pages
   - Insert split key into parent
   - May propagate split up to root
9. Write page to disk (or buffer for batch writes)
10. If WAL enabled, append `V3WALRecord::PageWrite` before main file update
11. Update `header.node_count` and sync to disk

**Write Amplification:** Index write + page write + WAL write per node insert

---

### WAL Recovery Flow

1. `V3Backend::open()` detects existing WAL file
2. Creates `WALRecovery` engine with WAL path
3. Calls `recovery.recover()`:
   - Open WAL file, read 64-byte header
   - Validate magic (`V3WAL_MAGIC`) and version (1)
   - Read current/committed/checkpointed LSN values
   - Sequentially read records:
     - Read 4-byte size prefix
     - Read record bytes
     - Deserialize via `bincode::deserialize()`
     - Apply record to in-memory state
   - Track statistics (processed, applied, skipped)
   - Stop on EOF or corrupt record
4. Update recovery statistics
5. Apply recovered checkpoint header to `PersistentHeaderV3`
6. Continue normal operation with recovered state

**Recovery Statistics:**
- `records_processed`: Total records read
- `records_applied`: Successfully applied records
- `records_skipped`: Corrupt/invalid records
- `page_allocations`, `page_frees`, `page_writes`, `btree_splits`: Operation counts
- `checkpoints`: Checkpoint records encountered
- `success_rate`: Applied / processed ratio

---

### Edge Query Flow (Stub)

1. Client calls `V3Backend::neighbors(snapshot_id, node_id, query)`
2. `V3Backend` delegates to `edge_store.outgoing(node_id)` or `incoming(node_id)`
3. `V3EdgeStore` checks cache for `(node_id, direction)` key
4. If cache miss:
   - Currently returns empty Vec (stub implementation)
   - TODO: Phase 66 will implement B+Tree edge lookup
5. Returns neighbor list to caller

**Current Limitation:** All edge queries return empty results

## Key Abstractions

### NodeRecordV3

**Purpose:** In-memory node representation with full ID encoding

**Location:** `sqlitegraph/src/backend/native/v3/node/record.rs`

**Structure:**
```rust
pub struct NodeRecordV3 {
    pub id: i64,                              // Full node ID
    pub flags: NodeFlags,                      // Node flags
    pub kind_offset: u16,                     // String table offset
    pub name_offset: u16,                     // String table offset
    pub data_len: u16,                        // Inline data length (0-64)
    pub data_inline: Option<Vec<u8>>,          // Inline data or None
    pub data_external_offset: Option<u64>,       // External data offset
    pub outgoing_cluster_offset: u64,          // Outgoing edge cluster
    pub outgoing_edge_count: u32,             // Outgoing edge count
    pub incoming_cluster_offset: u64,          // Incoming edge cluster
    pub incoming_edge_count: u32,             // Incoming edge count
}
```

**Fixed Metadata Layout (44 bytes):**
- `id: 8` (offset 0)
- `flags: 4` (offset 8)
- `kind_offset: 2` (offset 12)
- `name_offset: 2` (offset 14)
- `data_len: 2` (offset 16)
- `reserved: 2` (offset 18)
- `outgoing_cluster_offset: 8` (offset 20)
- `outgoing_edge_count: 4` (offset 26)
- `incoming_cluster_offset: 8` (offset 30)
- `incoming_edge_count: 4` (offset 38)

**External Data Flag:**
- Bit 15 (0x8000) of `data_len` indicates external storage
- Maximum inline data: 64 bytes

**Creation Methods:**
- `new_inline()`: Create with inline data (≤64 bytes)
- `new_external()`: Create with external data reference (>64 bytes)

**Serialization:**
- `serialize()`: Big-endian encoding
- `deserialize()`: Parse from bytes with validation
- `serialized_size()`: Calculate size before encoding

---

### NodePage

**Purpose:** Fixed-size (4KB) container for variable-length compressed node records

**Location:** `sqlitegraph/src/backend/native/v3/node/page.rs`

**Structure:**
```rust
pub struct NodePage {
    pub page_id: u64,              // Page identifier
    pub next_page_id: u64,          // Overflow page link
    pub nodes: Vec<NodeRecordV3>,    // Node records in page
    pub used_bytes: u16,            // Actual bytes used in data region
    pub base_id: i64,              // Minimum ID for delta encoding
    pub checksum: u32,              // Page checksum
}
```

**Page Header Layout (32 bytes):**
- `page_id: 8` (offset 0): Page ID
- `next_page_id: 8` (offset 8): Overflow link (0 if none)
- `node_count: 2` (offset 16): Number of nodes
- `used_bytes: 2` (offset 18): Bytes used in data region
- `base_id: 8` (offset 20): Base for delta encoding
- `checksum: 4` (offset 28): XOR checksum
- Reserved to 32 bytes total

**Compression:**
- Delta encoding: All node IDs encoded as `id - base_id`
- Varint encoding: Variable-length fields use 1-10 bytes
- Estimated compressed size: ~12 bytes minimum per node

**Page Capacity:**
- `MAX_PAGE_SIZE`: 4096 bytes
- `PAGE_HEADER_SIZE`: 32 bytes
- `USABLE_SIZE`: 4064 bytes
- `MAX_NODE_CAPACITY`: 50 nodes (conservative estimate)
- `ESTIMATED_NODE_SLOT_SIZE`: 80 bytes (fixed metadata + 32 avg data)

**Operations:**
- `new(page_id)`: Create empty page
- `with_capacity(page_id, capacity)`: Create with pre-allocated vector
- `add_node(node)`: Add node with capacity check
- `pack()`: Serialize to 4096-byte array
- `unpack(bytes)`: Deserialize with checksum validation
- `is_full()`: Check if adding node would exceed capacity
- `remaining_capacity()`: Return free bytes
- `space_efficiency()`: Calculate used/total ratio

**Overflow Handling:**
- `next_page_id` links to continuation page
- Large datasets span multiple pages
- Sequential pages scanned during node lookup

---

### IndexPage (B+Tree)

**Purpose:** B+Tree index page for node_id → page_id mapping

**Location:** `sqlitegraph/src/backend/native/v3/index/page.rs`

**Enum Definition:**
```rust
pub enum IndexPage {
    Internal {
        page_id: u64,
        keys: Vec<u64>,        // Max 254 split keys
        children: Vec<u64>,     // Max 255 child pointers
        checksum: u32,
    },
    Leaf {
        page_id: u64,
        entries: Vec<(u64, u64)>,  // (node_id, page_id) pairs, max 254
        next_leaf: u64,         // Link to next leaf
        checksum: u32,
    },
}
```

**Internal Page:**
- Keys divide the key space: `keys[i]` = min key in child i+1
- Children count = keys count + 1
- Search: Binary search to find child index

**Leaf Page:**
- Entries stored sorted by node_id for binary search
- `next_leaf` enables sequential scans and range queries
- Linked list structure for ordered iteration

**Page Header (32 bytes):**
- `page_id: 8` (offset 0)
- `is_leaf: 1` (offset 8): 1 for leaf, 0 for internal
- `count: 2` (offset 9): Number of keys/entries
- `checksum: 4` (offset 11): XOR checksum
- `reserved: 17` (offset 15): Padding to 32 bytes

**Data Region (4064 bytes):**
- Internal: 16 bytes per key + 8 bytes per child
- Leaf: 16 bytes per entry (8 key + 8 page_id)

**Capacity Calculations:**
- Max keys in internal: floor((4096 - 32 - 8) / 16) = 254
- Max entries in leaf: floor((4096 - 32 - 8) / 16) = 254
- One slot reserved for next_leaf pointer in leaf

**Operations:**
- `new_internal(page_id)`: Create empty internal page
- `new_leaf(page_id)`: Create empty leaf page
- `pack()`: Serialize with big-endian encoding
- `unpack(bytes)`: Deserialize with checksum validation
- `binary_search_leaf()`: O(log n) lookup in leaf
- `find_child_index()`: Find child for key in internal

**Splitting:**
- Handled by `BTreeManager::split_and_insert_leaf()`
- Creates new page, redistributes entries
- Propagates split key upward
- May cause parent split if parent full

---

### V3WALRecord

**Purpose:** Immutable record of a single state-changing operation for WAL

**Location:** `sqlitegraph/src/backend/native/v3/wal.rs`

**Enum Variants (8 types):**
```rust
pub enum V3WALRecord {
    PageAllocate { lsn: u64, page_id: u64, timestamp: u64 },
    PageFree { lsn: u64, page_id: u64, checksum: u32, timestamp: u64 },
    PageWrite { lsn: u64, page_id: u64, offset: u32, data: Vec<u8>, checksum: u32, timestamp: u64 },
    BTreeSplit { lsn: u64, original_page_id: u64, new_page_id: u64, split_key: u64, page_type: u8, timestamp: u64 },
    Checkpoint { lsn: u64, root_page_id: u64, total_pages: u64, btree_height: u32, free_page_list_head: u64, header_snapshot: Vec<u8>, timestamp: u64 },
    TransactionBegin { lsn: u64, tx_id: u64, timestamp: u64 },
    TransactionCommit { lsn: u64, tx_id: u64, timestamp: u64 },
    TransactionRollback { lsn: u64, tx_id: u64, timestamp: u64 },
}
```

**Record Properties:**
- `lsn`: Log sequence number (monotonically increasing)
- `timestamp`: Unix epoch timestamp of operation
- All records implement `record_type()`, `lsn()`, `is_data_modifying()`, `is_transaction_control()`, `is_checkpoint()`

**Serialization:**
- Uses `bincode` for complex types
- `to_bytes()`: Returns Result<Vec<u8>> with size prefix
- `MAX_RECORD_SIZE`: 1MB safety limit

**Record Types:**
- Data-modifying: PageAllocate, PageFree, PageWrite, BTreeSplit
- Transaction control: TransactionBegin, TransactionCommit, TransactionRollback
- Checkpoint: Checkpoint (persists full header snapshot)

---

### PageAllocator

**Purpose:** Dynamic page allocation with free list management

**Location:** `sqlitegraph/src/backend/native/v3/allocator.rs`

**Structure:**
```rust
pub struct PageAllocator {
    bitmap: Vec<bool>,       // Allocation bitmap
    free_list_head: u64,      // Head of free page list
    total_pages: u64,         // Total pages in database
    page_size: u64,           // Page size (from header)
}
```

**Page State Enum:**
- `Free`: Page is on free list
- `Allocated`: Page is in use
- `Pinned`: Page cannot be freed (WAL operation)

**Allocation Strategy:**
1. Check free list for reusable page
2. If free list empty, append new page at end
3. Mark page as allocated in bitmap
4. Return page_id to caller

**Deallocation Strategy:**
1. Validate page_id ≠ 0 (header page cannot be freed)
2. Check bitmap for double-free detection
3. Mark page as free in bitmap
4. Add to free list head (singly-linked)
5. Return error on double-free

**Free List Management:**
- Singly-linked via page headers
- `FreePageHeader` (16 bytes) at start of free page
- `next_free: u64`: Next free page
- `checksum: u64`: Page integrity validation

**Page Offset Calculation:**
- Page 0: Header (offset 0, not data page)
- Data pages: `V3_HEADER_SIZE + (page_id - 1) * PAGE_SIZE`
- Example: Page 1 → offset 112, Page 2 → offset 4208

**Initialization:**
- From `PersistentHeaderV3`: Read free_list_head and total_pages
- Reserves pages 0 (header) and 1 (first data) as allocated
- Remaining pages marked free in bitmap

**Validation:**
- Double-free prevention via bitmap
- Checksum validation via `xor_checksum()`
- Page state queries via `get_page_state()`

---

### V3EdgeStore

**Purpose:** Edge storage using V2 EdgeCluster format (compatibility layer)

**Location:** `sqlitegraph/src/backend/native/v3/edge_compat.rs`

**Structure:**
```rust
pub struct V3EdgeStore {
    btree: BTreeManager,                              // Index for (src, dir) → page_id
    wal: Option<WALWriter>,                        // Optional WAL
    cache: HashMap<(i64, Direction), V3EdgeCluster>,  // In-memory cache
}
```

**Edge Cluster:**
```rust
pub struct V3EdgeCluster {
    pub src: i64,                              // Source node ID
    pub edges: Vec<CompactEdgeRecord>,           // Destination nodes
    pub direction: Direction,                     // Outgoing/Incoming
    pub format_version: u8,                    // V2 compat (1)
    pub page_id: u64,                           // Storage page
}
```

**Direction Enum:**
- `Outgoing`: Edges from source to targets
- `Incoming`: Edges from targets to source
- Maps to V2 Direction for compatibility

**Operations:**
- `new()`: Create empty edge store
- `insert_edge(src, dst, dir)`: Add edge to cluster
- `lookup_cluster(src, dir)`: Find cluster by (src, dir)
- `neighbors(src, dir)`: Get destination nodes
- `outgoing(src)`: Get outgoing neighbors
- `incoming(src)`: Get incoming neighbors

**Serialization:**
- Format: `[version: 1] [count: 4] [edges...]`
- Edge format: V2 `CompactEdgeRecord`
- Version allows future format migration

**Current Limitations:**
- No edge page persistence (TODO)
- In-memory cache only
- Stub implementations for V2 integration

**Depends on:** `BTreeManager` for future edge page indexing

**Used by:** `V3Backend` for graph traversal operations

## Entry Points

### V3Backend

**Location:** `sqlitegraph/src/backend/native/v3/backend.rs`

**Public Creation Methods:**
```rust
impl V3Backend {
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self, SqliteGraphError>
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, SqliteGraphError>
    pub fn create_with_wal<P: AsRef<Path>>(path: P, enable_wal: bool) -> Result<Self, SqliteGraphError>
}
```

**Creation Process:**
1. Create `PersistentHeaderV3::new_v3()`
2. Serialize header to bytes (112 bytes)
3. Write header to new file
4. Sync file to disk
5. Initialize `PageAllocator` from header
6. Initialize `BTreeManager` with allocator
7. Initialize `NodeStore` with btree, allocator, and db_path
8. Initialize `V3EdgeStore` with btree
9. Optionally create `WALWriter` if WAL enabled
10. Wrap components in `V3Backend` with RwLocks

**Open Process:**
1. Read first 112 bytes from file
2. Deserialize `PersistentHeaderV3::from_bytes()`
3. Validate header (magic, version, flags, field ordering)
4. Initialize components from existing header state
5. Detect and recover WAL if `.v3wal` file exists
6. Return error if file not found or validation fails

**Internal Accessors:**
- `db_path()`: Returns database file path reference
- `is_wal_enabled()`: Check if WAL is active
- `header()`: Get current header state (clone)
- `flush_to_disk()`: Flush WAL buffer
- `sync_header()`: Write header to disk at offset 0

**GraphBackend Implementation:**
All `GraphBackend` trait methods are implemented, mapping to:
- Node operations → `NodeStore` with B+Tree lookups
- Edge operations → `V3EdgeStore` (currently stub)
- Traversal → BFS/shortest_path using edge store
- System ops → Checkpoint, flush, backup, snapshot

---

### WALRecovery

**Location:** `sqlitegraph/src/backend/native/v3/wal.rs` (struct definition)

**Purpose:** Recover database state from WAL file after crash

**Structure:**
```rust
pub struct WALRecovery {
    wal_path: PathBuf,                                    // WAL file location
    page_cache: HashMap<u64, Vec<u8>>,               // In-memory page cache
    stats: WALRecoveryStats,                             // Recovery statistics
    checkpoint_header: Option<PersistentHeaderV3>,       // Last checkpoint
    last_lsn: u64,                                      // Last LSN processed
}
```

**Recovery Process:**
1. Open WAL file for read access
2. Read and validate 64-byte header
3. Sequentially read records until EOF:
   - Read 4-byte size (little-endian u32)
   - Read N bytes of record data
   - Deserialize via `bincode::deserialize()`
   - Call `apply_record()` to update state
4. On error, increment `records_skipped`, continue
5. Return final statistics and recovered state

**Record Application:**
- `PageAllocate`: Insert empty page into cache
- `PageFree`: Remove page from cache
- `PageWrite`: Update page data in cache
- `BTreeSplit`: Insert new page into cache
- `Checkpoint`: Restore `PersistentHeaderV3` from snapshot
- Transaction records: Update LSN tracking only

**Statistics Tracking:**
- Records processed, applied, skipped
- Operation counts (allocations, frees, writes, splits, checkpoints)
- Success rate calculation

**Public Methods:**
- `new()`: Create recovery engine with WAL path
- `recover()`: Execute recovery, update state
- `stats()`: Get recovery statistics reference
- `checkpoint_header()`: Get restored header (if available)
- `last_lsn()`: Get final LSN processed

---

### WALWriter

**Location:** `sqlitegraph/src/backend/native/v3/wal.rs`

**Purpose:** Append WAL records to disk with buffering

**Structure:**
```rust
pub struct WALWriter {
    wal_path: PathBuf,          // Path to .v3wal file
    current_lsn: u64,           // Current log sequence number
    committed_lsn: u64,          // Last committed LSN
    buffer: Vec<u8>,           // Record buffer before fsync
    flush_threshold: usize,       // Buffer size before auto-flush (64KB)
}
```

**Buffering Strategy:**
- Records buffered in memory
- Auto-flush when threshold exceeded
- `flush()` explicitly flushes buffer
- `commit()` updates committed_lsn in WAL header

**Write Operations:**
All return `Result<u64>` (new LSN):
- `append(record)`: Add record to buffer
- `page_allocate(page_id)`: Log page allocation
- `page_free(page_id, checksum)`: Log page deallocation
- `page_write(page_id, offset, data)`: Log page write
- `btree_split(orig, new, key, is_leaf, lsn)`: Log B+Tree split
- `checkpoint(root, total, height, free_head, header)`: Log checkpoint
- `transaction_begin/commit/rollback(tx_id)`: Transaction boundaries

**File Operations:**
- `write_header()`: Initialize new WAL file with 64-byte header
- `update_header()`: Update LSN fields in existing header
- `flush()`: Write buffer + fsync
- `truncate()`: Delete WAL after successful checkpoint

**WAL File Paths:**
- `V3WALPaths::wal_file(db)`: `{db}.v3wal`
- `V3WALPaths::checkpoint_file(db)`: `{db}.v3checkpoint`
- `V3WALPaths::temp_checkpoint_file(db)`: `{db}.v3checkpoint.tmp.{random}`

---

### BTreeManager

**Location:** `sqlitegraph/src/backend/native/v3/btree.rs`

**Purpose:** Manage B+Tree index for node_id → page_id lookups

**Structure:**
```rust
pub struct BTreeManager {
    allocator: PageAllocator,                    // Page lifecycle
    wal: Option<Arc<RwLock<WALWriter>>>, // Optional WAL
    root_page_id: u64,                        // Root page (0 if empty)
    tree_height: u32,                          // Tree height (0 if empty)
    page_cache: HashMap<u64, IndexPage>,    // LRU page cache
    cache_capacity: usize,                      // Max cached pages (16)
}
```

**Tree State:**
- `EMPTY_TREE_ROOT`: u64::MAX (sentinel for empty tree)
- `MAX_TREE_HEIGHT`: 10 (safety limit)
- Root page grows upward on splits

**Lookup Process:**
1. Return `None` if tree empty
2. Start at root page
3. For each level:
   - Load page via `load_page()`
   - If Internal: binary search keys, follow child pointer
   - If Leaf: binary search entries for match
   - Return `Some(page_id)` if found
4. Return `None` if key not in tree

**Insert Process:**
1. If tree empty, create first leaf as root
2. Find leaf path tracking (page_id, child_idx) pairs
3. Load leaf page
4. If key exists, update entry and return
5. If leaf full, call `split_and_insert_leaf()`
6. Allocate new page for split
7. Redistribute entries between pages
8. Update parent or create new root if needed
9. Log all operations to WAL

**Cache Strategy:**
- LRU-style page cache (16 pages default)
- `load_page()`: Check cache first, then disk
- `write_page()`: Evict if at capacity, update cache
- `clear_cache()`: Empty all cached pages

**Splitting:**
- Leaf split: Divide entries at midpoint
- Internal page split: Divide keys and children
- Propagation: Insert split key into parent
- Root growth: New root allocated if root splits

---

### NodeStore

**Location:** `sqlitegraph/src/backend/native/v3/node/store.rs`

**Purpose:** High-level node operations with B+Tree integration

**Structure:**
```rust
pub struct NodeStore {
    db_path: PathBuf,                    // Database file path
    btree: Arc<RwLock<BTreeManager>>,   // B+Tree for lookups
    allocator: Arc<RwLock<PageAllocator>>, // Page allocator
    page_cache: HashMap<u64, NodePage>,  // LRU page cache
    traversal_cache: TraversalCache,        // Node traversal cache
}
```

**Cache Types:**
- `TraversalCache`: LRU cache for frequently accessed nodes
  - `DEFAULT_CACHE_CAPACITY`: 16 entries
  - `MAX_CACHE_CAPACITY`: 1024 entries
  - `MIN_CACHE_CAPACITY`: 4 entries

**Operations:**
- `initialize(btree, allocator, wal)`: Set up storage components
- `lookup_node(node_id)`: Find node via B+Tree + page load
- `insert_node(record)`: Insert node with page management
- `update_node(id, record)`: Update in-place
- `delete_node(id)`: Remove from B+Tree and free page

**Page Management:**
- Pages cached in `HashMap<u64, NodePage>`
- Cache evicted when at capacity
- Page loads use B+Tree to get page_id, then load page

**WAL Integration:**
- Optional WAL writer passed during initialization
- Page writes logged to WAL before main file update
- Checkpoint updates WAL header with latest state

## Error Handling

**Strategy:** Result-based error propagation with specific error variants

**NativeBackendError Variants:**
- `InvalidMagic`: File magic doesn't match V3_MAGIC
- `UnsupportedVersion`: Format version not V3_FORMAT_VERSION (4)
- `InvalidHeader`: Header field validation failed with field name and reason
- `InvalidChecksum`: Checksum mismatch (expected vs found)
- `CorruptionDetected`: Structural corruption (e.g., double-free)
- `InvalidNodeId`: Node ID out of valid range
- `InvalidEdgeId`: Edge ID out of valid range
- `CorruptNodeRecord`: Node record deserialization failed
- `CorruptEdgeRecord`: Edge record deserialization failed
- `SerializationError`: Bincode serialization failed
- `DeserializationError`: Bincode deserialization failed
- `IoError`: Wrapped I/O error with context string
- `RecordTooLarge`: WAL record exceeds MAX_RECORD_SIZE
- `BincodeError`: Wrapped bincode error
- `LockError`: Lock acquisition failed
- `UnsupportedVersion`: Version mismatch

**Error Conversion:**
- `map_v3_error()` in backend.rs converts to SqliteGraphError
- Preserves error context while translating to generic types

**Recovery:**
- WAL recovery continues past corrupt records (marks as skipped)
- Checksum validation fails return error but don't crash
- Page allocation failures propagate immediately (no silent fallback)

## Cross-Cutting Concerns

### Checksumming

**Algorithm:** XOR checksum with seed constant

**Implementation:**
- `xor_checksum(data)`: XOR each byte with index and seed
- Seed: `0x5A5A5A5A5A5A5A` (from constants.rs)
- Applied to: All pages (index, node, free)
- Storage: 4 bytes in page header (offset 28-31)

**Validation:**
- Checked on page unpack after reading from disk
- Failure returns `InvalidChecksum` error
- Prevents silent data corruption

---

### Validation

**Header Validation (`PersistentHeaderV3::validate()`):**
- Magic number: Must be `V3_MAGIC`
- Version: Must be `V3_FORMAT_VERSION` (4)
- Required flags: `FLAG_V2_FRAMED_RECORDS | FLAG_V2_ATOMIC_COMMIT | FLAG_V3_BTREE_INDEX`
- Offset ordering: `node_data_offset >= V3_HEADER_SIZE`
- Cluster offsets: Non-decreasing order
- Page size: Must be 4096, 8192, or 16384
- B+Tree height: Must be ≤ `MAX_BTREE_HEIGHT`

**Page Validation:**
- Size bounds: 4096 bytes exactly
- Checksum: XOR must match calculated value
- Capacity: Used bytes ≤ usable size
- Node count: Header count matches actual entries

**Bounds Checking:**
- All array accesses use `try_into().unwrap()` for conversion
- Length checks before slice operations
- Enum variant validation

---

### Serialization

**Encoding:** Big-endian (network byte order) for all multi-byte values

**Rationale:** Cross-platform compatibility (x86, ARM, etc.)

**Implementation:**
- All integers: `to_be_bytes()` / `from_be_bytes()`
- Single-byte fields: Direct byte values (flags, page type)
- Page headers: Big-endian
- Node records: Big-endian (not yet varint, which is little-endian)

**Consistency:**
- Page pack/unpack use same endianness
- Cross-platform data exchange guaranteed

---

### Concurrency

**Locking Strategy:** `RwLock<T>` for interior mutability

**Components with Locks:**
- `V3Backend.btree: RwLock<BTreeManager>`
- `V3Backend.node_store: RwLock<NodeStore>`
- `V3Backend.edge_store: RwLock<V3EdgeStore>`
- `V3Backend.allocator: RwLock<PageAllocator>`
- `V3Backend.wal: Option<RwLock<WALWriter>>`
- `V3Backend.header: RwLock<PersistentHeaderV3>`

**Lock Granularity:**
- Component-level locks (not single global lock)
- Allows concurrent read operations
- Write operations acquire exclusive locks

**WAL Concurrency:**
- WAL writer uses `Arc<RwLock<WALWriter>>` for shared access
- Append operations atomic via lock
- Header updates serialized

**Cache Concurrency:**
- Page cache: `HashMap<u64, NodePage>` (no locking in current design)
- Traversal cache: LRU with `RwLock<T>`

---

### Platform Considerations

**File System:**
- Uses `std::fs::File` for I/O
- `std::io::{Read, Write, Seek, SeekFrom}` traits
- Path manipulation via `std::path::PathBuf`

**Memory Mapping:**
- Currently using read/write (not mmap)
- Future: Memory-mapped I/O for performance (TODO)

**Atomic Operations:**
- File sync via `file.sync_all()` for durability
- WAL ensures atomic multi-page updates

**Endianness:**
- Big-endian serialization for portability
- Native byte order used on disk

---

### Constants

**Magic Numbers:**
- `V3_MAGIC`: `[0x53, 0x51, 0x4C, 0x54, 0x47, 0x46, 0x00, 0x03]` ("SQLTGF\3\0")
- `V3_WAL_MAGIC`: `[0x56, 0x33, 0x57, 0x41, 0x4C, 0x00, 0x00]` ("V3WAL\0\0")

**Sizes:**
- `V3_HEADER_SIZE`: 112 bytes (80 V2 + 32 V3)
- `DEFAULT_PAGE_SIZE`: 4096 bytes (4KB)
- `PAGE_HEADER_SIZE`: 32 bytes (for all page types)
- `USABLE_PAGE_SIZE`: 4064 bytes
- `MAX_BTREE_HEIGHT`: 4 (sufficient for 4B+ nodes)

**Compression Thresholds:**
- `MAX_INLINE_DATA`: 64 bytes (max inline data in node)
- `MAX_VARINT_BYTES`: 10 bytes (max varint encoded u64)
- `SINGLE_BYTE_VARINT_MAX`: 127 (one-byte varint threshold)

---

### Version Management

**Version Numbers:**
- `V2_FORMAT_VERSION`: 3 (V2 native backend)
- `V3_FORMAT_VERSION`: 4 (V3 native backend)

**Version Detection:**
- `PersistentHeaderV3::detect_version()` checks magic[7] byte
- V2 magic: magic[7] = 0
- V3 magic: magic[7] = 3

**Migration:**
- No automatic migration from V2 to V3
- Separate formats require manual export/import
- Backward compatibility not maintained (breaking change)

---

### Page Types

**PageType Enum (`v3/edge_compat.rs`):**
- `Free = 0`: Unallocated page
- `BTreeIndex = 1`: B+Tree index page
- `NodeData = 2`: Node storage page
- `EdgeCluster = 3`: Edge cluster page
- `Wal = 4`: WAL page
- `Checkpoint = 5`: Checkpoint page

**Type Discrimination:**
- Stored in page headers (for future multi-type page files)
- Used for page validation and allocation decisions

---

### B+Tree Properties

**Branching Factor:**
- Internal pages: Up to 255 children (keys + 1)
- Leaf pages: Up to 254 entries
- Effective fanout: ~128 (average)

**Height Analysis:**
- Height 0: Empty tree
- Height 1: Root leaf page (up to ~254 nodes)
- Height 2: One internal level (up to ~64K nodes)
- Height 3: Two internal levels (up to ~8M nodes)
- Height 4: Three internal levels (up to ~1B nodes)
- MAX_BTREE_HEIGHT = 4: Sufficient for 4B+ nodes

**Space Utilization:**
- Internal page: ~4064 / 254 ≈ 16 bytes per key
- Leaf page: ~4064 / 254 ≈ 16 bytes per entry
- Minimal overhead: 32-byte header per page

**Performance Characteristics:**
- Lookup: O(log n) comparisons
- Insert: O(log n) for traversal + O(n) for page split (worst case)
- Range scan: O(1) per page via next_leaf pointers
- Cache-friendly: 4KB pages fit in typical cache lines

---

### Compression Efficiency

**Delta Encoding:**
- Space savings: ~4 bytes per node (i64 → u32 delta)
- Best case: Sequential IDs compress to 1-2 bytes
- Worst case: Random IDs may need full varint encoding

**Varint Encoding:**
- Small values (<128): 1 byte
- Medium values (<16384): 2 bytes
- Large values (<2M): 3 bytes
- Maximum: 10 bytes for u64

**Combined Effect:**
- Node metadata: ~44 bytes → ~12 bytes compressed
- ~73% size reduction for typical node records
- Trade-off: CPU cost for compression vs space savings

**When Compression Helps:**
- Dense ID ranges (sequential node creation)
- Small metadata values (kind/name offsets < 256)
- Low edge counts (most nodes have few edges)

**When Compression Hurts:**
- Random ID allocation (non-sequential)
- Large metadata values (offsets > 2^14)
- High edge counts (requires multi-byte varint)

---

### File Organization

**V3 Database File:**
- Extension: `.graph` (configurable)
- Layout: Header + sequential pages
- Page 0: Header (112 bytes, not a full page)
- Pages 1+: Data pages (4KB each, offset = V3_HEADER_SIZE + (page_id - 1) * PAGE_SIZE)

**WAL File:**
- Extension: `.v3wal`
- Location: Same directory as database file
- Layout: WAL header (64 bytes) + sequential records
- Record format: [4-byte size] [bincode serialized record]
- Managed separately from main file

**Checkpoint File:**
- Extension: `.v3checkpoint`
- Created during WAL checkpoint operation
- Contains database file state snapshot
- Used for recovery after WAL truncation

**Offset Calculations:**
- Page 0 (header): Always at offset 0
- Data pages: `V3_HEADER_SIZE + (page_id - 1) * PAGE_SIZE`
- Example: Page 5 = 112 + 4 * 4096 = 16496 bytes
- Page 1 = 112 + 0 * 4096 = 112 bytes (first data page)

**Atomicity:**
- Header writes: Not atomic (risk of corruption)
- WAL writes: Atomic per record (buffered then fsynced)
- Checkpoint: Atomic header snapshot + WAL truncation

---

### Component Lifecycle

**Initialization Order:**
1. `PersistentHeaderV3::new_v3()` - Create default header
2. `PageAllocator::new(&header)` - Init from header fields
3. `BTreeManager::new(allocator, wal)` - Create empty tree
4. `NodeStore::new()` - Create with btree and allocator
5. `V3EdgeStore::new(btree, wal)` - Create edge store
6. `V3Backend` wraps all components

**Runtime Lifecycle:**
- B+Tree grows with node insertions (height increases)
- Pages allocated/freed via `PageAllocator`
- WAL records flushed on commit or threshold
- Caches evicted when at capacity

**Shutdown:**
- WAL flushed via `WALWriter::flush()`
- Checkpoint written before close
- File handles closed (implicit via Drop)

---

### Data Integrity

**Header Integrity:**
- Magic number validation on file open
- Version compatibility check
- Field ordering validation (non-decreasing offsets)
- Checksum in page data (not header)

**Page Integrity:**
- XOR checksum over entire page (header + data)
- Validated on unpack after disk read
- Corruption returns error, prevents bad data access

**WAL Integrity:**
- Bincode serialization ensures valid records
- Size prefix validated (≤ MAX_RECORD_SIZE)
- Record type validation before application

**Recovery Safety:**
- Corrupt WAL records skipped (not applied)
- Partial recovery continues from last valid state
- Checkpoint provides consistent recovery point

---

### Integration Points

**V3 → V2 Compatibility:**
- `V3EdgeStore` reuses V2 `CompactEdgeRecord` format
- `Direction::to_v2()` converts to V2 direction
- Enables gradual migration (V3 storage with V2 edges)

**V3 → Native Backend:**
- `V3Backend` implements `GraphBackend` trait
- Selected via `BackendKind::Native` in config
- Coexists with SQLite backend option

**V3 → Graph Core:**
- Uses standard `GraphEntity` for node representation
- Uses `SnapshotId` for read isolation
- Integrates with existing graph algorithms

**WAL Integration:**
- Optional component in `V3Backend`
- Enabled via `create_with_wal(path, true)`
- All page operations logged when WAL present

---

### Future Extensions

**Planned Features:**
- Phase 66: Full B+Tree edge indexing (currently stub)
- Checkpoint module: Dedicated checkpoint management (not yet created)
- Edge page persistence: Separate edge page type
- Memory-mapped I/O: Performance optimization for large databases

**Extensibility Points:**
- Page types: Reserved for future use (Free, BTreeIndex, NodeData, EdgeCluster, Wal, Checkpoint)
- Reserved header fields: 2 bytes in header reserved for V3
- WAL record types: Extensible enum for new operations

---

*Architecture analysis: 2026-02-13*

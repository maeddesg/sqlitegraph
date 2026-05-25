# Native V2 Binary Format (DEPRECATED)

**Status:** ⚠️ **DEPRECATED**  
**Last Updated:** 2026-02-12  
**Version:** v2.0.0  
**Removal Target:** v1.7.0

---

## ⚠️ Deprecation Notice

The Native V2 backend is **deprecated as of v2.0.0** and will be removed in v1.7.0.

### Migration Path

1. **V2 → V3 (Recommended)**: Native V3 is the modern replacement
2. **V2 → SQLite**: Stable, mature alternative

### V2 Limitations

- No KV store support (added in V3 via lazy initialization)
- No Pub/Sub support (added in SQLite via in-memory publisher)
- No HNSW vector storage (added in V3 via KV store)
- Binary format not compatible with V3

### Why Deprecate?

- V3 provides all V2 features with better architecture
- Maintaining three backends is unsustainable
- V3's lazy initialization means no overhead for unused features

---

## Format Documentation (For Reference Only)

### File Header

```rust
/// Native V2 file header
#[repr(C)]
pub struct NativeFileHeader {
    /// Magic bytes: "SQLG" (0x53, 0x51, 0x4C, 0x47)
    pub magic: [u8; 4],
    
    /// Format version (current: 2)
    pub version: u32,
    
    /// Page size in bytes (typically 4096)
    pub page_size: u32,
    
    /// Number of pages in file
    pub page_count: u64,
    
    /// Root page of B-tree index
    pub root_page: u64,
    
    /// Creation timestamp
    pub created_at: u64,
    
    /// Reserved for future use
    pub reserved: [u8; 32],
}
```

### Page Structure

```
┌─────────────────────────────────────┐
│           Page Header (16 bytes)    │
├─────────────────────────────────────┤
│  type: u8      │ Page type          │
│  flags: u8     │ Page flags         │
│  count: u16    │ Number of entries  │
│  free: u16     │ Free space offset  │
│  next: u64     │ Next page pointer  │
├─────────────────────────────────────┤
│           Page Data                 │
│  ┌─────────┐  ┌─────────┐          │
│  │ Entry 1 │  │ Entry 2 │  ...     │
│  └─────────┘  └─────────┘          │
├─────────────────────────────────────┤
│           Free Space                │
└─────────────────────────────────────┘
```

### Page Types

| Type | Value | Description |
|------|-------|-------------|
| META | 0x01  | Metadata page |
| NODE | 0x02  | Node data page |
| EDGE | 0x03  | Edge data page |
| INDEX | 0x04 | B-tree index page |
| FREE | 0x05  | Free page list |

### Node Record Format

```rust
/// Variable-length node record
pub struct NodeRecord {
    /// Record header
    pub header: NodeHeader,
    
    /// Node kind (variable length, null-terminated)
    pub kind: CString,
    
    /// Node name (variable length, null-terminated)
    pub name: CString,
    
    /// File path (variable length, null-terminated)
    pub file_path: CString,
    
    /// JSON data (variable length)
    pub data: JsonBlob,
}

#[repr(C)]
pub struct NodeHeader {
    /// Node ID (8 bytes)
    pub id: i64,
    
    /// Data offset in file (8 bytes)
    pub offset: u64,
    
    /// Total record size (4 bytes)
    pub size: u32,
    
    /// Flags (4 bytes)
    pub flags: u32,
}
```

### Edge Record Format

```rust
/// Edge record (fixed size: 32 bytes)
#[repr(C)]
pub struct EdgeRecord {
    /// Edge ID
    pub id: i64,
    
    /// Source node ID
    pub source: i64,
    
    /// Target node ID
    pub target: i64,
    
    /// Edge kind (hash for lookup)
    pub kind_hash: u32,
    
    /// Flags
    pub flags: u32,
    
    /// Weight (optional, NaN if not set)
    pub weight: f64,
}
```

---

## B-Tree Index

V2 uses a B+tree for node lookup by ID:

```
                    ┌─────┐
                    │Root │
                    │Page │
                    └──┬──┘
           ┌─────────┼─────────┐
           ▼         ▼         ▼
        ┌─────┐   ┌─────┐   ┌─────┐
        │Internal   │Internal   │Leaf   │
        │Page 1     │Page 2     │Page N │
        └──┬──┘   └──┬──┘   └─────┘
           │         │
           ▼         ▼
        ┌─────┐   ┌─────┐
        │Leaf   │   │Leaf   │
        │Page 1 │   │Page 2 │
        └─────┘   └─────┘
```

### B-Tree Node

```rust
#[repr(C)]
pub struct BTreeNode {
    /// Node header
    pub header: BTreeHeader,
    
    /// Keys (sorted array)
    pub keys: Vec<i64>,
    
    /// Child pointers (internal nodes) or values (leaf nodes)
    pub children: Vec<u64>,
}

#[repr(C)]
pub struct BTreeHeader {
    /// Number of keys
    pub key_count: u16,
    
    /// Is leaf node?
    pub is_leaf: u8,
    
    /// Reserved
    pub reserved: u8,
    
    /// Right sibling (leaf nodes only)
    pub right_sibling: u64,
}
```

---

## API Usage (Deprecated)

```rust
use sqlitegraph::backend::v2::V2Backend;

// ⚠️ Deprecated - use V3Backend instead
let backend = V2Backend::create("data.v2")?;

// Operations work but emit deprecation warnings
let node_id = backend.insert_node(node_spec)?;
let edges = backend.get_edges_from(node_id)?;
```

---

## Migration Examples

### V2 → V3 Migration

```rust
// Old (V2) - DEPRECATED
use sqlitegraph::backend::v2::V2Backend;
let v2 = V2Backend::create("data.v2")?;

// New (V3) - RECOMMENDED
use sqlitegraph::backend::v3::V3Backend;
let v3 = V3Backend::create("data.graph")?;

// Same API, better performance, more features
let node_id = v3.insert_node(node_spec)?;
```

### V2 → SQLite Migration

```rust
// Old (V2)
use sqlitegraph::backend::v2::V2Backend;
let v2 = V2Backend::open("legacy.v2")?;

// New (SQLite)
use sqlitegraph::backend::sqlite::SqliteGraphBackend;
let sqlite = SqliteGraphBackend::create("migrated.db")?;

// Migrate data
for node in v2.find_nodes(None, None, None, None)? {
    sqlite.insert_node(NodeSpec::from(node))?;
}
```

---

## Historical Context

V2 was introduced in v1.3.0 as the first native binary format, providing:
- Faster operations than SQLite for simple graphs
- Smaller file sizes
- Direct memory mapping

V3 (v2.0.0) supersedes V2 with:
- Lazy-initialized KV store
- HNSW vector storage support
- Better memory management
- Cleaner architecture

---

## See Also

- [V3 Backend Architecture](../../ARCHITECTURE.md) - Current recommended backend
- [SQLite Backend](../../ARCHITECTURE.md) - Stable alternative
- [CHANGELOG](../../CHANGELOG.md) - Deprecation timeline

# Native V2 Binary Format Guide

**Last Updated:** 2026-02-03
**Version:** v1.4.1

This guide explains the Native V2 binary file format, including cluster layout, node/edge storage, WAL format, and recovery procedures.

---

## Overview

The Native V2 backend is a custom file format optimized for graph workloads with clustered adjacency for optimal I/O locality and storage efficiency.

### Key Characteristics

| Characteristic | Value |
|----------------|-------|
| **Magic Bytes** | `S Q L T G F 0 0` |
| **Format Version** | 3 (schema_version: u32) |
| **Storage Model** | Clustered edge records |
| **Compression** | Delta encoding + bit packing |
| **Transaction Model** | WAL-based with checkpointing |
| **Isolation** | MVCC snapshot isolation |

---

## File Structure

### File Organization

```
graph.db
├── Header (fixed 4096 bytes)
│   ├── Magic bytes (8 bytes)
│   ├── Schema version (4 bytes)
│   ├── Reserved (4 bytes)
│   ├── Node count (8 bytes)
│   ├── Edge count (8 bytes)
│   ├── Next cluster offset (8 bytes)
│   ├── Free space info (...)
│   └── Checkpoint LSN (...)
│
├── String Table
│   └── Deduplicated strings (kind, name)
│
├── Node Records Area
│   └── NodeRecordV2 records (variable size)
│
├── Edge Clusters Area
│   ├── Outgoing clusters (contiguous)
│   └── Incoming clusters (contiguous)
│
├── Free Space
│   └── Reusable blocks from deletions
│
└── WAL (graph.db.wal)
    └── Append-only transaction log
```

---

## Header Format

### V2Header Structure

```rust
#[repr(C)]
pub struct V2Header {
    /// Magic bytes: "SQLTGF" + 2 null bytes
    pub magic: [u8; 8],

    /// Schema version (u32, currently 3)
    pub schema_version: u32,

    /// Reserved space (4 bytes)
    pub reserved: [u8; 4],

    /// Total number of nodes in the graph
    pub node_count: u64,

    /// Total number of edges in the graph
    pub edge_count: u64,

    /// File offset for next cluster allocation
    pub next_cluster_offset: u64,

    /// Offset to free space manager data
    pub free_space_offset: u64,

    /// LSN of last checkpoint
    pub checkpoint_lsn: u64,

    /// LSN of current WAL head
    pub wal_lsn: u64,

    /// File creation time (Unix timestamp)
    pub created_at: u64,

    /// Last modification time (Unix timestamp)
    pub modified_at: u64,

    /// Database flags
    pub flags: u64,

    /// Reserved space for future use
    pub reserved2: [u8; 4048],
}
```

### Magic Bytes

```rust
pub const V2_MAGIC: [u8; 8] = [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0];
```

- **Never changes**: Magic bytes identify the file format across all versions
- **Bytes 0-5**: "SQLTGF" (SQLiteGraph)
- **Bytes 6-7**: Null bytes (version 2+)

---

## Node Records

### NodeRecordV2 Structure

Located in `node_record_v2/core.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRecordV2 {
    /// Node ID (1-based)
    pub id: i64,

    /// Node flags (deleted, etc.)
    pub flags: NodeFlags,

    /// Node kind (e.g., "Class", "Function")
    pub kind: String,

    /// Node name (unique identifier)
    pub name: String,

    /// JSON payload
    pub data: serde_json::Value,

    /// Outgoing cluster file offset
    pub outgoing_cluster_offset: FileOffset,

    /// Outgoing cluster size in bytes
    pub outgoing_cluster_size: u32,

    /// Outgoing edge count
    pub outgoing_edge_count: u32,

    /// Incoming cluster file offset
    pub incoming_cluster_offset: FileOffset,

    /// Incoming cluster size in bytes
    pub incoming_cluster_size: u32,

    /// Incoming edge count
    pub incoming_edge_count: u32,
}
```

### Node Storage

Nodes are stored contiguously starting after the string table:

```
Offset 0:          String Table
Offset X:          Node Records (variable size)
├── Node 1 (ID=1)
│   ├── Flags (1 byte)
│   ├── Kind offset (2 bytes)
│   ├── Name offset (2 bytes)
│   ├── Data offset (2 bytes)
│   ├── Outgoing cluster offset (8 bytes)
│   ├── Outgoing cluster size (4 bytes)
│   ├── Outgoing edge count (4 bytes)
│   ├── Incoming cluster offset (8 bytes)
│   ├── Incoming cluster size (4 bytes)
│   └── Incoming edge count (4 bytes)
├── Node 2 (ID=2)
└── ...
```

---

## Edge Clusters

### Cluster Organization

Edges are stored in **clusters** - contiguous blocks of all edges for a specific node and direction:

```
For Node 5:
├── Outgoing Cluster: [ (5→1), (5→2), (5→3), ... ]
└── Incoming Cluster: [ (1→5), (2→5), (3→5), ... ]
```

This provides:
- **I/O locality**: Reading all neighbors = 1 disk read
- **Compression**: Delta encoding works well on sorted IDs
- **Parallel writes**: Outgoing and incoming written separately

### CompactEdgeRecord

Located in `edge_cluster/cluster.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompactEdgeRecord {
    /// Target node ID (delta-encoded in clusters)
    pub neighbor_id: i64,

    /// Offset to edge type in string table
    pub type_offset: u16,

    /// Length of edge data
    pub data_len: u16,

    /// Edge data payload (optional)
    pub edge_data: Vec<u8>,
}
```

### Cluster Serialization Format

```
Cluster Header (8 bytes):
├── Edge count (u32, 4 bytes, big-endian)
└── Reserved (4 bytes)

Edge Records (variable per edge):
For each edge:
├── Neighbor ID (i64, 8 bytes, big-endian)
├── Type offset (u16, 2 bytes, big-endian)
├── Data length (u16, 2 bytes, big-endian)
└── Edge data (variable, data_len bytes)
```

### Compression

1. **Delta Encoding**: `neighbor_id` stored as difference from previous
2. **Bit Packing**: Small IDs use fewer bytes
3. **String Deduplication**: Edge types stored in string table

---

## String Table

### Purpose

Deduplicate commonly repeated strings (node kind, node name, edge types) to save space.

### Structure

```
String Table Format:
├── Entry count (u32, 4 bytes)
└── String Entries
    ├── Length (u16, 2 bytes)
    ├── String bytes (variable)
    └── ... repeat for each entry
```

### Usage

Instead of storing full "Class" string for every node:

```rust
// Store offset instead
node.kind_offset = 5;  // Points to string table entry 5
```

---

## WAL (Write-Ahead Log)

### WAL File Format

```
graph.db.wal
├── WAL Header
│   ├── Magic (8 bytes)
│   ├── Format version (4 bytes)
│   ├── Page size (4 bytes)
│   └── Checkpoint sequence number
│
└── WAL Records (append-only)
    ├── Record header (type + length)
    ├── Record body (variable)
    └── ... repeat
```

### WAL Record Types

Located in `wal/record.rs`:

| Record Type | Code | Description |
|-------------|------|-------------|
| `NodeInsert` | 1 | Node creation with initial data |
| `NodeUpdate` | 2 | Node modification/update |
| `NodeDelete` | 3 | Node deletion (logical) |
| `ClusterCreate` | 4 | Edge cluster creation |
| `EdgeInsert` | 5 | Edge insertion into cluster |
| `EdgeUpdate` | 6 | Edge modification |
| `EdgeDelete` | 7 | Edge deletion (logical) |
| `StringInsert` | 8 | String table entry |
| `FreeSpaceAllocate` | 9 | Free space block allocation |
| `FreeSpaceDeallocate` | 10 | Free space block deallocation |
| `TransactionBegin` | 11 | Transaction begin marker |
| `TransactionCommit` | 12 | Transaction commit marker |
| `TransactionRollback` | 13 | Transaction rollback marker |
| `Checkpoint` | 14 | Checkpoint marker |
| `HeaderUpdate` | 15 | Database header update |
| `SegmentEnd` | 16 | End of WAL segment |
| `KvSet` | 31 | KV key-value set |
| `KvDelete` | 32 | KV key delete |

### WAL Record Format

```rust
pub struct V2WALRecord {
    /// Record type
    pub record_type: V2WALRecordType,

    /// Log Sequence Number (assigned on commit)
    pub lsn: u64,

    /// Transaction ID (optional)
    pub tx_id: Option<u64>,

    /// Record-specific data
    pub data: V2WALRecordData,
}
```

### Example: NodeInsert Record

```
Record Header:
├── Type: NodeInsert (1 byte)
├── LSN: (8 bytes)
├── TxID: (8 bytes, optional)
└── Data length: (4 bytes)

Record Body:
├── Node ID (8 bytes)
├── Flags (1 byte)
├── Kind (string)
├── Name (string)
├── Data (JSON)
└── Cluster references
```

---

## Checkpointing

### Checkpoint Process

Located in `wal/checkpoint/`:

```
1. Acquire exclusive lock
2. Flush all pending WAL records
3. Scan WAL from last checkpoint LSN
4. Apply records to main database file
5. Update header checkpoint_lsn
6. Truncate WAL (optional)
7. Release lock
```

### Checkpoint Strategies

```rust
pub enum CheckpointStrategy {
    /// Passive: Automatic when WAL reaches threshold
    Passive,

    /// Full: Force full checkpoint now
    Full,

    /// Incremental: Checkpoint N records
    Incremental(u64),
}
```

---

## Recovery Procedure

### Crash Recovery

Located in `wal/recovery/`:

```
1. Open database file
2. Read header, get last_checkpoint_lsn
3. Open WAL file
4. Scan WAL for records after last_checkpoint_lsn
5. For each record:
   a. Deserialize record
   b. Validate checksum
   c. Apply to in-memory structures
   d. Handle rollback records
6. Verify consistency
7. Open database for normal operation
```

### Recovery States

```rust
pub enum RecoveryState {
    /// No recovery needed (clean shutdown)
    Clean,

    /// Recovery in progress
    InProgress {
        wal_lsn: u64,
        records_processed: usize,
    },

    /// Recovery completed with warnings
    Recovered {
        warnings: Vec<String>,
    },

    /// Recovery failed
    Failed {
        error: String,
    },
}
```

---

## Free Space Management

### FreeSpaceManager

Located in `free_space/manager.rs`:

```rust
pub struct FreeSpaceManager {
    /// Available free blocks sorted by offset
    blocks: Vec<FreeBlock>,

    /// Next block ID to allocate
    next_block_id: u64,
}

pub struct FreeBlock {
    /// Block identifier
    pub id: u64,

    /// File offset
    pub offset: u64,

    /// Block size in bytes
    pub size: u64,

    /// Allocation status
    pub status: BlockStatus,
}
```

### Allocation Strategy

```
1. Search for block with size >= requested
2. Use first-fit (simple, fast)
3. If found:
   a. Mark block as allocated
   b. If block much larger than needed, split it
4. If not found:
   a. Extend file (allocate at EOF)
```

---

## File Format Validation

### Validation Checks

Located in `mod.rs`:

```rust
pub struct ValidationMetrics {
    /// Storage efficiency (0.0 - 1.0)
    pub storage_efficiency: f64,

    /// I/O locality score (0.0 - 1.0)
    pub io_locality_score: f64,

    /// Average edge size in bytes
    pub avg_edge_size: usize,

    /// Cluster space utilization (0.0 - 1.0)
    pub cluster_utilization: f64,
}

impl ValidationMetrics {
    /// Validate that V2 implementation meets performance targets
    pub fn validate_targets(&self) -> NativeResult<()> {
        if self.storage_efficiency < 0.7 {
            return Err(NativeBackendError::ValidationFailed { /* ... */ });
        }
        if self.avg_edge_size > 100 {
            return Err(NativeBackendError::ValidationFailed { /* ... */ });
        }
        Ok(())
    }
}
```

### Performance Targets

```rust
pub mod performance_targets {
    /// Compact edge records should be < 100 bytes average
    pub const MAX_AVG_EDGE_SIZE: usize = 100;

    /// Storage improvement should be > 70%
    pub const MIN_STORAGE_IMPROVEMENT: f64 = 0.7;

    /// I/O operations should be reduced by > 10x
    pub const MIN_IO_REDUCTION_FACTOR: f64 = 10.0;

    /// Adjacency operations should be > 2x faster
    pub const MIN_ADJACENCY_SPEEDUP: f64 = 2.0;
}
```

---

## I/O Patterns

### Reading Node Neighbors

```
1. Read node record (single read)
2. Get outgoing_cluster_offset
3. Read cluster (single contiguous read)
4. Decompress edges
```

### Writing New Edge

```
Transaction Path:
1. Add EdgeInsert record to WAL
2. On commit:
   a. Allocate space in cluster (or extend)
   b. Serialize edge record
   c. Write to cluster
   d. Update node record edge count
```

---

## Migration

### Format Version Detection

Located in `migration/detect.rs`:

```rust
pub fn detect_format_version(path: &Path) -> Result<FormatVersion, MigrationError> {
    let mut file = File::open(path)?;

    // Read and verify magic bytes
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;

    if magic != V2_MAGIC {
        return Err(MigrationError::InvalidMagic);
    }

    // Read schema version
    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let schema_version = u32::from_be_bytes(version_bytes);

    match schema_version {
        1 => Ok(FormatVersion::V1),
        2 => Ok(FormatVersion::V2),
        3 => Ok(FormatVersion::V3),
        _ => Err(MigrationError::UnknownVersion(schema_version)),
    }
}
```

### Migration Execution

Located in `migration/execute.rs`:

```rust
pub fn migrate_file(
    from_path: &Path,
    to_path: &Path,
    from_version: FormatVersion,
    to_version: FormatVersion,
) -> Result<MigrationResult, MigrationError> {
    // 1. Open source file
    // 2. Read all nodes and edges
    // 3. Transform to new format
    // 4. Write to destination
    // 5. Verify migration
}
```

---

## Testing

### Test Files

| File | Description |
|------|-------------|
| `tests/v2_*_regression.rs` | Format-specific regression tests |
| `snapshot_tests.rs` | Snapshot isolation tests |
| `wal/tests.rs` | WAL format tests |
| `wal/checkpoint/*_tests.rs` | Checkpoint tests |

### Key Test Scenarios

```rust
#[test]
fn test_header_magic() {
    let mut header = V2Header::default();
    header.magic.copy_from_slice(&V2_MAGIC);

    assert_eq!(&header.magic[..8], &V2_MAGIC[..]);
}

#[test]
fn test_cluster_serialization() {
    let edges = vec![
        CompactEdgeRecord { neighbor_id: 1, /* ... */ },
        CompactEdgeRecord { neighbor_id: 2, /* ... */ },
    ];

    let serialized = serialize_cluster(&edges)?;
    let deserialized = deserialize_cluster(&serialized)?;

    assert_eq!(deserialized, edges);
}

#[test]
fn test_wal_replay() {
    // Create WAL with NodeInsert record
    // Crash (simulate)
    // Recover from WAL
    // Verify node exists
}
```

---

## Common Patterns

### Creating a New Database

```rust
let file = OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .open("graph.db")?;

// Initialize header
let mut header = V2Header::default();
header.magic.copy_from_slice(&V2_MAGIC);
header.schema_version = V2_FORMAT_VERSION;
header.created_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

// Write header (padded to 4096 bytes)
file.write_all(serialize_header(&header)?)?;
```

### Reading Node Neighbors

```rust
let node_record = node_store.read_node(node_id)?;

// Read outgoing cluster
let cluster_data = read_cluster(
    &file,
    node_record.outgoing_cluster_offset,
    node_record.outgoing_cluster_size,
)?;

// Decompress edges
let edges = decompress_cluster(&cluster_data)?;

for edge in edges {
    println!("{} -> {}", node_id, edge.neighbor_id);
}
```

---

## Performance Considerations

### Storage Efficiency

| Component | V1 | V2 | Improvement |
|-----------|----|----|-------------|
| Edge Record | ~200 bytes | ~50-100 bytes | 2-4x |
| Adjacency Read | N random I/Os | 1 sequential I/O | 10x+ |
| String Storage | Duplicated | Deduplicated | 2-3x |

### I/O Locality

Clustered edges mean:
- **Sequential reads**: Prefetching works well
- **Page alignment**: Fewer page faults
- **Cache efficiency**: Better CPU cache usage

---

## Troubleshooting

### Issue: "Invalid magic bytes"

**Symptoms:** `MigrationError::InvalidMagic`

**Causes:**
1. Not a V2 database file
2. File corrupted
3. Wrong file opened

**Solutions:**
1. Verify file path
2. Check file isn't SQLite backend
3. Run recovery tool if corrupted

### Issue: "Schema version too new"

**Symptoms:** `MigrationError::UnknownVersion(version)`

**Causes:**
1. File created with newer SQLiteGraph version
2. Need to upgrade

**Solutions:**
1. Upgrade SQLiteGraph
2. Run migration if available

### Issue: Recovery fails

**Symptoms:** `RecoveryError` during open

**Causes:**
1. WAL truncated
2. Checkpoint inconsistency
3. File system corruption

**Solutions:**
1. Check WAL file integrity
2. Try truncating WAL to last valid record
3. Restore from backup

---

## References

- **Source:** `src/backend/native/v2/` - Full implementation
- **Header:** `mod.rs` - V2_MAGIC, V2_FORMAT_VERSION
- **WAL:** `wal/record.rs` - WAL record types
- **Nodes:** `node_record_v2/` - Node record format
- **Edges:** `edge_cluster/` - Cluster format
- **Related:** `docs/ARCHITECTURE.md` - High-level architecture

---

## Format Evolution

### Version History

| Version | Changes |
|---------|---------|
| 1 | Initial V2 format |
| 2 | Added schema_version as u32 |
| 3 | Current format - full KV + pub/sub support |

### Backward Compatibility

- **Magic bytes never change** - identifies file as SQLiteGraph
- **Schema version** - used for feature detection and migration
- **Unknown record types** - ignored during WAL replay (future-proof)

---

## Future Enhancements

Potential areas for extension (not currently planned):

1. **Compression** - Zstandard for clusters
2. **Encryption** - At-rest encryption for sensitive data
3. **Sharding** - Multi-file partitioning
4. **Replication** - Multi-master replication
5. **Streaming** - Append-only streaming mode

Note: These are **not currently planned** - the V2 format is focused on single-machine performance and reliability.

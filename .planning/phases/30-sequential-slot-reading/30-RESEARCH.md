# Phase 30: Sequential Slot Reading - Research

**Researched:** 2026-01-21
**Domain:** Sequential I/O batch reading for graph traversals
**Confidence:** HIGH

## Summary

This phase implements batch slot reading for sequential I/O coalescing during graph traversals. The core insight is that linear chains (degree <= 1 for 3+ consecutive steps) read one 4KB node slot per hop, decode adjacency, extract the single neighbor ID, then discard everything before the next hop. This "read-drop-repeat" pattern is pathological for performance.

**Primary recommendation:** Implement `NodeStore::read_slots_batch()` to read 8 sequential slots (32KB) in a single I/O operation, coupled with `SequentialReadBuffer` to cache decoded adjacency data across traversal hops.

## Standard Stack

### Core
| Component | Approach | Why Standard |
|-----------|----------|--------------|
| Batch I/O | Single `read_exact()` for 32KB (8 slots) | Reduces system calls from 8 to 1 |
| Buffer storage | Per-traversal `HashMap<node_id, NodeRecord>` | Follows existing `TraversalCache` pattern from v1.3 |
| Slot size | 4096 bytes per node (`NODE_SLOT_SIZE`) | Already defined in `constants::node::NODE_SLOT_SIZE` |
| Prefetch window | 8 slots (32KB) | Per STATE.md v1.4 decision based on RocksDB/LMDB research |

### Supporting
| Library | Purpose | When to Use |
|---------|---------|-------------|
| `ahash::AHashMap` | Per-traversal buffer storage | Same as existing `TraversalCache` (v1.3) |
| `std::io::Read::read_exact` | Batch reading | Standard Rust I/O |

### Constants Already Defined
```rust
// src/backend/native/constants.rs
pub const NODE_SLOT_SIZE: u64 = 4096;  // Node slot size
pub const EDGE_SLOT_SIZE: u64 = 256;    // Edge slot size
pub const HEADER_SIZE: u64 = 80;        // File header size
```

## Architecture Patterns

### Recommended Module Structure
```
src/backend/native/
├── node_store.rs           # Add read_slots_batch() method
└── adjacency/
    └── sequential_buffer.rs # NEW: SequentialReadBuffer
```

### Pattern 1: Batch Slot Reading (NodeStore extension)

**What:** Extend `NodeStore<'a>` with `read_slots_batch()` method

**When to use:** When `LinearDetector.is_linear_confirmed()` returns `true` (after 3+ consecutive degree-1 steps)

**Slot offset calculation (already exists in codebase):**
```rust
// From node_edge_access.rs:186-192
pub fn calculate_node_offset(
    graph_file: &GraphFile,
    node_id: NativeNodeId,
) -> u64 {
    graph_file.persistent_header.node_data_offset
        + ((node_id - 1) as u64 * NODE_SLOT_SIZE)  // NODE_SLOT_SIZE = 4096
}
```

**Batch read implementation pattern:**
```rust
// In NodeStore<'a>
pub fn read_slots_batch(
    &mut self,
    start_node_id: NativeNodeId,
    count: usize,
) -> NativeResult<Vec<NativeNodeId>> {
    let node_data_offset = self.graph_file.persistent_header().node_data_offset;
    let start_offset = node_data_offset + ((start_node_id - 1) as u64 * NODE_SLOT_SIZE);
    let total_bytes = count as u64 * NODE_SLOT_SIZE;

    // Bounds check
    let file_size = self.graph_file.file_size()?;
    if start_offset + total_bytes > file_size {
        return Err(NativeBackendError::FileTooSmall {
            size: file_size,
            min_size: start_offset + total_bytes,
        });
    }

    // Single read for all slots
    let mut buffer = vec![0u8; total_bytes as usize];
    self.graph_file.read_bytes(start_offset, &mut buffer)?;

    // Decode each slot
    let mut results = Vec::with_capacity(count);
    for i in 0..count {
        let slot_offset = i * NODE_SLOT_SIZE as usize;
        let slot_data = &buffer[slot_offset..slot_offset + NODE_SLOT_SIZE as usize];
        // Parse V2 node record
        let node = NodeRecordV2::deserialize_from_slice(slot_data)?;
        results.push(node.id);
    }

    Ok(results)
}
```

### Pattern 2: SequentialReadBuffer (new module)

**What:** Per-traversal buffer for decoded node slots

**Design:**
```rust
// src/backend/native/adjacency/sequential_buffer.rs
use ahash::AHashMap;
use crate::backend::native::v2::node_record_v2::NodeRecordV2;
use crate::backend::native::types::NativeNodeId;

/// Per-traversal buffer for sequential I/O optimization
///
/// - Scoped to single traversal (evaporates when function returns)
/// - Prefetches 8 slots (32KB) after LinearDetector confirms linear pattern
/// - Stores decoded NodeRecordV2 for rapid access without re-decoding
pub struct SequentialReadBuffer {
    /// Decoded node records from batched reads
    slots: AHashMap<NativeNodeId, NodeRecordV2>,

    /// Prefetch window (default: 8 slots = 32KB)
    prefetch_window: usize,

    /// Next node ID to prefetch (for sequential windows)
    next_prefetch_id: Option<NativeNodeId>,
}

impl SequentialReadBuffer {
    pub fn new() -> Self {
        Self {
            slots: AHashMap::new(),
            prefetch_window: 8,  // 32KB
            next_prefetch_id: None,
        }
    }

    /// Get node from buffer, returns None if not cached
    pub fn get(&self, node_id: NativeNodeId) -> Option<&NodeRecordV2> {
        self.slots.get(&node_id)
    }

    /// Insert batched nodes into buffer
    pub fn insert_batch(&mut self, nodes: Vec<NodeRecordV2>) {
        for node in nodes {
            self.slots.insert(node.id, node);
        }
    }

    /// Check if node is in buffer
    pub fn contains(&self, node_id: NativeNodeId) -> bool {
        self.slots.contains_key(&node_id)
    }

    /// Trigger prefetch (called after LinearDetector confirms)
    pub fn prefetch_from(
        &mut self,
        graph_file: &mut GraphFile,
        start_node_id: NativeNodeId,
    ) -> NativeResult<()> {
        let mut node_store = NodeStore::new(graph_file);
        let batch = node_store.read_slots_batch(start_node_id, self.prefetch_window)?;

        // Decode and cache
        // (implementation details depend on read_slots_batch return type)
        self.next_prefetch_id = Some(start_node_id + self.prefetch_window as i64);
        Ok(())
    }
}
```

### Pattern 3: Integration with LinearDetector

**What:** Only trigger prefetch AFTER `LinearDetector.is_linear_confirmed()` returns `true`

**Design:**
```rust
// In traversal hot path (e.g., chain_queries.rs)
use crate::backend::native::adjacency::{LinearDetector, SequentialReadBuffer};

pub fn traverse_chain_optimized(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
) -> NativeResult<Vec<NativeNodeId>> {
    let mut detector = LinearDetector::new();
    let mut buffer = SequentialReadBuffer::new();
    let mut result = Vec::new();
    let mut current = start;

    loop {
        // Get degree
        let degree = AdjacencyHelpers::outgoing_degree(graph_file, current)?;

        // Observe for pattern detection
        let pattern = detector.observe(current, degree);

        // Check if linear confirmed - trigger prefetch
        if detector.is_linear_confirmed() && !buffer.contains(current) {
            buffer.prefetch_from(graph_file, current)?;
        }

        // Try buffer first (after linear confirmed)
        let node = if buffer.contains(current) {
            buffer.get(current).unwrap()
        } else {
            // Fallback to normal read
            let mut node_store = NodeStore::new(graph_file);
            node_store.read_node(current)?
        };

        result.push(node.id);

        // Follow single outgoing edge
        if node.outgoing_edge_count != 1 {
            break;  // Not linear, or end of chain
        }

        // Get neighbor (from cached cluster or edge store)
        let neighbors = AdjacencyHelpers::get_outgoing_neighbors(graph_file, current)?;
        if neighbors.len() != 1 {
            break;
        }

        current = neighbors[0];
    }

    Ok(result)
}
```

### Anti-Patterns to Avoid

- **Global buffer sharing**: Breaks MVCC isolation. Must use per-traversal scoping.
- **Prefetch before threshold confirmation**: Wastes I/O on branching patterns. Wait for `is_linear_confirmed()`.
- **Arc<NodeRecord> storage**: Creates reference cycles. Use owned `NodeRecordV2` copies (cheap).
- **Thread-local storage**: Unnecessary overhead. Traversals are single-threaded; per-traversal stack allocation is sufficient.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Per-traversal cache | Custom `HashMap` wrapper | `TraversalCache` pattern from v1.3 | Already proven pattern, uses `ahash::AHashMap` |
| Buffer scoping | Thread-local or global | Stack-allocated per-function | Evaporates automatically, no cleanup needed |
| Hash-based lookups | Custom hash function | `ahash::AHashMap` | Fastest non-crypto hash, already in dependencies |

**Key insight:** The v1.3 `TraversalCache` already solved per-traversal scoping. Follow that exact pattern.

## Common Pitfalls

### Pitfall 1: File Offset Overflow

**What goes wrong:** Calculating `start_offset + (count * SLOT_SIZE)` can overflow `u64` for large counts

**Why it happens:** Missing checked arithmetic for slot offset calculation

**How to avoid:**
```rust
let total_bytes = (count as u64)
    .checked_mul(NODE_SLOT_SIZE)
    .ok_or(NativeBackendError::InvalidNodeId {
        id: start_node_id,
        max_id: i64::MAX,
    })?;
```

**Warning signs:** Panics on very large `count` values, or incorrect reads near end of file

### Pitfall 2: Reading Beyond File Bounds

**What goes wrong:** Batch read attempts to read past EOF for nodes near file end

**Why it happens:** Prefetch window (8 slots) extends past valid node range

**How to avoid:**
```rust
let max_slots = ((file_size - start_offset) / NODE_SLOT_SIZE) as usize;
let actual_count = count.min(max_slots);
```

**Warning signs:** `FileTooSmall` errors for valid node IDs near max node count

### Pitfall 3: Incorrect Slot Size Usage

**What goes wrong:** Using `EDGE_SLOT_SIZE` (256) instead of `NODE_SLOT_SIZE` (4096)

**Why it happens:** Both constants exist in `constants.rs`, easy to confuse

**How to avoid:** Always use `constants::node::NODE_SLOT_SIZE` with explicit path

**Warning signs:** Truncated reads, incorrect offset calculations

### Pitfall 4: Breaking MVCC Isolation

**What goes wrong:** Buffer shares data across transactions, returning stale data

**Why it happens:** Using global or thread-local storage instead of per-traversal scoping

**How to avoid:** Buffer must be stack-allocated in traversal function, drop on return

**Warning signs:** Test failures in `mvcc_cache_isolation_tests.rs`

## Code Examples

### Batch Read Implementation

```rust
// Source: Based on node_store.rs read_node_v2() pattern
// File: src/backend/native/node_store.rs

/// Read multiple sequential node slots in a single I/O operation
///
/// # Parameters
/// - `start_node_id`: First node ID to read (must be >= 1)
/// - `count`: Number of sequential slots to read (max 8 recommended)
///
/// # Returns
/// Vector of successfully decoded NodeRecordV2 instances
///
/// # Preconditions
/// - All node IDs must be valid (>= 1 and <= node_count)
/// - start_node_id + count - 1 <= node_count
pub fn read_slots_batch(
    &mut self,
    start_node_id: NativeNodeId,
    count: usize,
) -> NativeResult<Vec<NodeRecordV2>> {
    let header = self.graph_file.header();

    // Validate start node
    if start_node_id < 1 || start_node_id > header.node_count as NativeNodeId {
        return Err(NativeBackendError::InvalidNodeId {
            id: start_node_id,
            max_id: header.node_count as NativeNodeId,
        });
    }

    // Clamp count to available nodes
    let max_available = (header.node_count as NativeNodeId - start_node_id + 1) as usize;
    let actual_count = count.min(max_available);

    // Calculate batch offset and size
    let node_data_offset = header.node_data_offset;
    let start_offset = node_data_offset + ((start_node_id - 1) as u64 * NODE_SLOT_SIZE);
    let total_bytes = (actual_count as u64)
        .checked_mul(NODE_SLOT_SIZE)
        .ok_or(NativeBackendError::CorruptNodeRecord {
            node_id: start_node_id,
            reason: "Slot count overflow".to_string(),
        })?;

    // Validate file size
    let file_size = self.graph_file.file_size()?;
    if start_offset + total_bytes > file_size {
        return Err(NativeBackendError::FileTooSmall {
            size: file_size,
            min_size: start_offset + total_bytes,
        });
    }

    // Single batch read
    let mut buffer = vec![0u8; total_bytes as usize];
    self.graph_file.read_bytes(start_offset, &mut buffer)?;

    // Decode each slot
    let mut results = Vec::with_capacity(actual_count);
    for i in 0..actual_count {
        let slot_start = i * NODE_SLOT_SIZE as usize;
        let slot_end = slot_start + NODE_SLOT_SIZE as usize;
        let slot_data = &buffer[slot_start..slot_end];

        // Parse V2 node record from slot
        let record = NodeRecordV2::deserialize(slot_data)?;
        results.push(record);
    }

    Ok(results)
}
```

### V2 NodeRecord Deserialization Pattern

```rust
// Source: v2/node_record_v2.rs (existing pattern)
// The V2 node record has a specific serialization format:

// V2 Record Layout:
// [version: 1] [kind_len: 2] [name_len: 2] [data_len: 4]
// [kind_bytes] [name_bytes] [data_bytes]
// [outgoing_offset: 8] [outgoing_count: 4]
// [incoming_offset: 8] [incoming_count: 4]

// Key insight: Need to parse variable-length fields first,
// then extract the fixed adjacency metadata at the end.
```

### Existing I/O Patterns to Follow

```rust
// From node_store.rs:304-372
// Current single-slot read pattern:

// 1. Seek to slot offset
let slot_offset = node_data_offset + ((node_id - 1) as u64 * 4096);

// 2. Read header to get exact size
let mut header_buffer = vec![0u8; 21];
self.graph_file.read_bytes(slot_offset, &mut header_buffer)?;

// 3. Calculate actual record size
let (kind_len, name_len, data_len) = parse_v2_header_lengths(&header_buffer)?;
let actual_record_size = 21 + kind_len + name_len + data_len + 32;

// 4. Read full record
let mut buffer = vec![0u8; actual_record_size];
self.graph_file.read_bytes(slot_offset, &mut buffer)?;

// 5. Deserialize
let record = NodeRecordV2::deserialize(&buffer)?;

// BATCH VERSION: Same pattern, but read 8 slots at once in step 2
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single-slot reads (4KB per I/O) | Batch reads (32KB per I/O) | Phase 30 | ~8x fewer I/O syscalls for linear chains |
| No pattern detection | LinearDetector FSM (Phase 29) | Phase 29 | Enables selective prefetch |
| Per-node decode overhead | Batch decode with single I/O | Phase 30 | Reduced CPU overhead per hop |

**Deprecated/outdated:**
- V1 format support: Removed in Phase 31, V2-only now
- Global traversal cache: Rejected in v1.3 for MVCC reasons

## Open Questions

1. **V2 deserialization from slices**: Need to verify `NodeRecordV2::deserialize()` accepts `&[u8]` slice or requires owned `Vec<u8>`
   - **What we know:** `NodeRecordV2::deserialize()` exists and takes `&[u8]`
   - **What's unclear:** Whether deserialization handles in-slice data correctly or requires copying
   - **Recommendation:** Implement `read_slots_batch()` to return `Vec<NodeRecordV2>`, verify slice-based deserialization works

2. **Cluster metadata vs slot reading**: The V2 format has both node slots (4KB each) and edge clusters. For chain traversal, we need:
   - Node slots (to get cluster metadata offsets)
   - Edge cluster data (to get the single neighbor ID)
   - **What we know:** NodeRecordV2 contains `outgoing_cluster_offset` and `outgoing_edge_count`
   - **What's unclear:** Should buffer store both NodeRecordV2 AND neighbor lists, or just nodes?
   - **Recommendation:** Buffer stores `NodeRecordV2` only; neighbor lookup follows existing cluster path

## Sources

### Primary (HIGH confidence)

- **src/backend/native/node_store.rs**: Lines 228-390 show `read_node_v2()` implementation pattern for V2 slot reading
- **src/backend/native/constants.rs**: Lines 42-69 define `NODE_SLOT_SIZE = 4096` and related constants
- **src/backend/native/graph_file/node_edge_access.rs**: Lines 186-201 show `calculate_node_offset()` pattern
- **src/backend/native/adjacency/linear_detector.rs**: Complete Phase 29 implementation showing state machine interface
- **src/backend/native/graph_ops/cache.rs**: Lines 1-267 show per-traversal cache pattern for MVCC-safe scoping
- **.planning/STATE.md**: v1.4 milestone decisions (8-slot prefetch, 3-step threshold, per-traversal scoping)

### Secondary (MEDIUM confidence)

- **src/backend/native/adjacency/helpers.rs**: Lines 62-72 show `outgoing_degree()` implementation for LinearDetector integration
- **src/backend/native/graph_file/io_operations.rs**: Lines 59-72 show `read_with_ahead()` pattern for sequential reads
- **src/backend/native/v2/mod.rs**: Re-exports for V2 edge cluster types

### Tertiary (LOW confidence)

- Standard Rust I/O patterns (`std::io::Read::read_exact`)
- Common buffer eviction policies (LRU, etc.) - not needed for per-traversal design

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All constants and patterns exist in codebase
- Architecture: HIGH - LinearDetector completed, buffer pattern from v1.3 is proven
- Pitfalls: HIGH - Based on existing node_store.rs error handling patterns

**Research date:** 2026-01-21
**Valid until:** 2026-02-20 (30 days - stable API surface)

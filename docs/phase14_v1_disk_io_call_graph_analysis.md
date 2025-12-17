# Phase 14: V1 Native Disk I/O Call Graph + Data Path Analysis

## Overview

This document provides a comprehensive call chain analysis from benchmarks down to GraphFile and ReadBuffer operations, documenting function signatures, key fields, offsets, capacities, and algorithms for V1 native disk I/O profiling.

## Complete Call Chain: K-Hop Benchmark → Disk I/O

### 1. Benchmark Entry Point
**File**: `benches/k_hop.rs`
**Function**: `main()` → `k_hop_benchmark()`
**Signature**: `k_hop_benchmark(graph: &SqliteGraph, start_node: i64, depth: u32, direction: BackendDirection)`
**Algorithm**: Wrapper that calls high-level k-hop API with timing

### 2. High-Level K-Hop API
**File**: `src/multi_hop.rs:23`
**Function**: `k_hop(graph: &SqliteGraph, start: i64, depth: u32, direction: BackendDirection) → Result<Vec<i64>, SqliteGraphError>`
**Key Fields**: `start: i64`, `depth: u32`, `direction: BackendDirection`
**Algorithm**: Delegates to `k_hop_internal()` with allowed_lookup optimization

### 3. K-Hop Internal Implementation
**File**: `src/multi_hop.rs:125`
**Function**: `k_hop_internal(graph: &SqliteGraph, start: i64, depth: u32, direction: BackendDirection, allowed_lookup: Option<&AHashSet<&str>>) → Result<Vec<i64>, SqliteGraphError>`
**Key Fields**: `allowed_lookup: Option<&AHashSet<&str>>` (type filtering)
**Algorithm**: Multi-level traversal with adjacency resolution at each hop

### 4. Adjacency Resolution
**File**: `src/multi_hop.rs:146`
**Function**: `adjacency_for(graph: &SqliteGraph, node: i64, direction: BackendDirection, allowed_types: Option<&AHashSet<&str>>) → Result<Vec<i64>, SqliteGraphError>`
**Key Fields**: `node: i64`, `direction: BackendDirection`
**Algorithm**: Dispatches to backend-specific adjacency fetching

### 5. SQLite Backend Dispatch
**File**: `src/graph/adjacency.rs`
**Function**: `fetch_outgoing(&self, id: i64) → Result<Vec<i64>, SqliteGraphError>`
**Algorithm**: Routes to native backend for SQLiteGraph instances

### 6. Native Backend K-Hop Implementation
**File**: `src/backend/native/graph_backend.rs:163`
**Function**: `k_hop(&self, start: i64, depth: u32, direction: BackendDirection) → Result<Vec<i64>, SqliteGraphError>`
**Signature**: Calls `native_k_hop(graph_file, start as NativeNodeId, depth, direction.into())`
**Key Fields**: `start as NativeNodeId` (u64 conversion), `direction.into()` (Direction conversion)

### 7. Native K-Hop Core Algorithm
**File**: `src/backend/native/graph_ops.rs:288`
**Function**: `native_k_hop(graph_file: &mut GraphFile, start: NativeNodeId, depth: u32, direction: Direction) → Result<Vec<NativeNodeId>, NativeBackendError>`
**Key Fields**:
- `start: NativeNodeId` (u64)
- `depth: u32`
- `direction: Direction` (enum: Outgoing/Incoming)
**Algorithm**: Multi-level breadth-first traversal with visited set tracking

### 8. Adjacency Helpers - Critical I/O Entry Point
**File**: `src/backend/native/adjacency.rs:415`
**Function**: `get_outgoing_neighbors(graph_file: &mut GraphFile, node_id: NativeNodeId) → NativeResult<Vec<NativeNodeId>>`
**Signature**: Creates `AdjacencyIterator::new_outgoing(graph_file, node_id)?` then calls `iterator.collect()`
**Key Fields**: `node_id: NativeNodeId` (u64)

### 9. Adjacency Iterator Creation - Primary Disk I/O Trigger
**File**: `src/backend/native/adjacency.rs:74`
**Function**: `AdjacencyIterator::new_outgoing(graph_file: &'a mut GraphFile, node_id: NativeNodeId) → NativeResult<Self>`
**Key Operations**:
- Line 79: `let node_hot = get_node_hot(node_id)` (cache check)
- Line 82: `let edge_offsets = get_outgoing_edge_offsets(node_id)` (pointer table)
- Line 91-92: **DISK I/O PATH** - Falls back to NodeStore if cache/ptr table miss:
  ```rust
  let mut node_store = NodeStore::new(graph_file);
  let node = node_store.read_node(node_id)?;
  ```

### 10. Node Store Read Operation - Disk I/O Core
**File**: `src/backend/native/node_store.rs:124`
**Function**: `read_node(&mut self, node_id: NativeNodeId) → NativeResult<NodeRecord>`
**Key Operations**:
- Line 128: `self.rebuild_index_for_node(node_id)?` (offset calculation)
- Line 131: `let node = self.read_node_internal(node_id, offset)?` (actual disk read)

### 11. Node Index Rebuilding - Offset Calculation
**File**: `src/backend/native/node_store.rs:323`
**Function**: `rebuild_index_for_node(&mut self, target_id: NativeNodeId) → NativeResult<FileOffset>`
**Key Calculation**:
```rust
let offset = node_data_offset + ((id - 1) as u64 * 4096);
```
**Algorithm**: V1 fixed 4KB slot allocation per node
**Key Fields**:
- `node_data_offset: u64` (from file header)
- `id: NativeNodeId` (u64)
- `4096`: Fixed V1 node slot size (4KB)

### 12. Internal Node Read - Buffer Management
**File**: `src/backend/native/node_store.rs:182`
**Function**: `read_node_internal(&mut self, node_id: NativeNodeId, offset: FileOffset) → NativeResult<NodeRecord>`
**Key Calculations**:
```rust
let total_size = 1 + 4 + 8 + 2 + 2 + 4 + kind_len + name_len + data_len + 8 + 4 + 8 + 4;
```
**Algorithm**: Dynamic size calculation based on string lengths
**Key Fields**:
- `total_size: usize` (calculated node record size)
- `offset: FileOffset` (byte position in file)

### 13. Graph File Read Operation - Primary Disk I/O
**File**: `src/backend/native/graph_file.rs:233`
**Function**: `read_bytes(&mut self, offset: u64, buffer: &mut [u8]) → NativeResult<()>`
**Key Operations**:
- Line 236-241: **Write buffer coherence flush** before reading
- Line 244: `if !self.read_buffer.read(offset, buffer)` (cache check)
- Line 246: `self.read_with_ahead(offset, buffer)?` (disk I/O if cache miss)

### 14. Read-Ahead Logic - Performance Optimization
**File**: `src/backend/native/graph_file.rs:267`
**Function**: `read_with_ahead(&mut self, offset: u64, buffer: &mut [u8]) → NativeResult<()>`
**Key Fields**:
- `self.read_buffer.capacity: usize` (64KB from graph_file.rs:103)
- `read_ahead_size: usize` (min of buffer size and capacity)
**Algorithm**: Reads larger chunk than requested for future cache hits
**Boundary Validation** (Phase 14 Step 9 fixes):
```rust
let file_size = self.file.metadata().map(|m| m.len()).unwrap_or(0);
let remaining_bytes = file_size.saturating_sub(offset);
let adjusted_read_size = std::cmp::min(read_ahead_size as u64, remaining_bytes) as usize;
```

### 15. Read Buffer Structure - Cache Management
**File**: `src/backend/native/graph_file.rs:14-45`
**Struct**: `ReadBuffer`
**Key Fields**:
- `data: Vec<u8>` (capacity: 64KB)
- `offset: u64` (start position of cached data)
- `size: usize` (actual cached data size)
- `capacity: usize` (maximum cache capacity)
**Algorithm**: LRU-style read-ahead caching with range checking

## Data Structures and Key Constants

### V1 File Layout (from phase14_kernel_redesign_plan.md)
```
[Header: 64B] [Node Slots: 4KB per ID] [Edge Slots: 256B per ID]
```

### Key Constants and Sizes
- **V1 Node Slot Size**: 4096 bytes (4KB)
- **V1 Edge Slot Size**: 256 bytes
- **Read Buffer Capacity**: 65536 bytes (64KB)
- **Write Buffer Capacity**: 32 operations
- **Header Size**: 64 bytes
- **Thread-Local Node Cache**: 100 entries

### Critical Types
```rust
type NativeNodeId = u64;
type FileOffset = u64;
type NativeEdgeId = u64;
```

### Node Record V1 Serialization Size
Formula from node_store.rs:182:
```
total_size = 1 + 4 + 8 + 2 + 2 + 4 + kind_len + name_len + data_len + 8 + 4 + 8 + 4
         = 41 + kind_len + name_len + data_len
```
Where:
- `1`: Version byte
- `4`: Flags (u32)
- `8`: Node ID (u64)
- `2+2`: Kind and name length (u16 each)
- `4`: Data length (u32)
- `kind_len + name_len + data_len`: Variable string/data
- `8+4+8+4`: Adjacency metadata (offsets + counts as u64/u32)

## I/O Performance Characteristics

### Read Patterns
1. **Sequential Node Access**: `rebuild_index_for_node()` calculates deterministic offsets
2. **Read-Ahead Optimization**: `read_with_ahead()` reads 64KB chunks
3. **Cache Coherence**: Write buffer flushed before reads for consistency

### Buffer Management
1. **ReadBuffer**: 64KB circular buffer with offset-based range checking
2. **WriteBuffer**: 32 pending operations, sorted by offset for better I/O patterns
3. **Thread-Local Cache**: 100-entry LRU cache for hot nodes

### Boundary Conditions (Phase 14 Step 9 Fixes)
1. **File EOF Protection**: Prevents reading beyond file end
2. **Buffer Size Validation**: Ensures requested data fits in available space
3. **Corruption Detection**: Clear error messages for boundary violations

## Disk I/O Bottlenecks Identified

### Primary Bottlenecks
1. **Node Store Reading**: `node_store.read_node()` - triggered for every uncached node access
2. **Read-Ahead Buffer**: `read_with_ahead()` - 64KB reads for every cache miss
3. **Offset Calculation**: `rebuild_index_for_node()` - 4KB slot math for every node

### Secondary Bottlenecks
1. **Adjacency Iteration**: Multiple `get_outgoing_neighbors()` calls per k-hop level
2. **Cache Misses**: Thread-local cache misses cause full disk reads
3. **Write Buffer Flushes**: Forced coherence flushes before reads

### I/O Amplification
- **Best Case**: All nodes cached in thread-local cache → 0 disk I/O
- **Typical Case**: Cache misses → 64KB reads per unique node
- **Worst Case**: Random node access → 64KB read per node, high fragmentation

## Algorithm Summary

### K-Hop I/O Pattern
1. **Level 0**: Start node read (64KB if uncached)
2. **Level 1-N**: For each node at current level, read adjacency (64KB per uncached node)
3. **Total I/O**: `O(unique_nodes * 64KB)` in worst case

### Optimization Levers
1. **Read-Ahead Size**: Currently 64KB, could be tuned for sequential vs random patterns
2. **Node Cache Size**: Currently 100 entries, could be increased for memory-rich systems
3. **Pointer Tables**: Fast path bypassing full node reads when available
4. **Hot Metadata**: Cached adjacency counts for degree filtering

---

**Status**: Complete V1 disk I/O call graph analysis
**Files Analyzed**: 5 core V1 files (benchmarks, k-hop, adjacency, node_store, graph_file, graph_ops)
**Key Finding**: 64KB read-ahead buffer with 4KB node slot allocation creates significant I/O amplification for large graphs
**Next Step**: Run additional benchmarks to quantify I/O characteristics and create regression harness